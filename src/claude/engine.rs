use std::borrow::Cow;
use std::hash::Hasher;

use rustc_hash::FxHasher;

use super::constants::{BOW, EOW, SHIFT};
use super::model::{FamilyConfig, FrameTail, State, Vocabulary, char_cost, scalar_only};
use super::normalize::normalize;
use super::stream::raw_head_space;
#[cfg(test)]
use super::stream::stream_normalized;
use super::stream::{Output, emit_normalized, mark_word};

pub(crate) fn count(text: &str, config: FamilyConfig, vocabulary: &Vocabulary) -> usize {
    let prepared = prepare(text, config);
    let trailing_newlines = prepared.trailing_newlines;
    let mut counter = Counter::new(vocabulary);
    emit_normalized(
        &prepared.normalized,
        config,
        raw_head_space(text),
        &mut counter,
    );
    counter.dfa.token_count() + frame_tail_cost(trailing_newlines)
}

// Direct-mapped word cache, scoped to one normalized document.
const CACHE_SLOTS: usize = 1024;

struct Counter<'a, 'v> {
    dfa: DfaCounter<'v>,
    // Checkpoint after EOW, before space + SHIFT*. A following BOW absorbs that
    // space; restoring the state also restores the exact segmentation cost.
    space: Option<(State, isize, usize)>,
    cache: [Option<(&'a str, usize)>; CACHE_SLOTS],
    after_eow: bool,
}

impl<'v> Counter<'_, 'v> {
    fn new(vocabulary: &'v Vocabulary) -> Self {
        Self {
            dfa: DfaCounter::new(vocabulary),
            space: None,
            cache: [None; CACHE_SLOTS],
            after_eow: false,
        }
    }

    fn independent_scalars<'b>(&mut self, body: &'b str) -> &'b str {
        // A scalar absent from all multi-scalar pieces cannot merge with either neighbor.
        // Consume that independent prefix once, then let the DFA handle the rest.
        let mut cost = 0;
        let mut remaining = "";
        for (offset, ch) in body.char_indices() {
            if !scalar_only(ch) {
                remaining = &body[offset..];
                break;
            }
            cost += char_cost(ch);
        }
        if cost > 0 {
            self.dfa.total += cost as isize;
            self.dfa.state = State::ROOT;
            self.space = None;
            self.after_eow = false;
        }
        remaining
    }

    fn absorb_space(&mut self) {
        if let Some((state, total, shifts)) = self.space.take() {
            self.dfa.state = state;
            self.dfa.total = total;
            for _ in 0..shifts {
                self.dfa.byte(SHIFT as u8);
            }
        }
    }
}

impl<'a> Output<'a> for Counter<'a, '_> {
    fn push(&mut self, ch: char) {
        if ch == SHIFT {
            if let Some((_, _, shifts)) = &mut self.space {
                *shifts += 1;
            }
        } else {
            self.space =
                (ch == ' ' && self.after_eow).then_some((self.dfa.state, self.dfa.total, 0));
        }
        self.after_eow = ch == EOW;
        self.dfa.push(ch);
    }
    fn push_str(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.space = (text == " " && self.after_eow).then_some((self.dfa.state, self.dfa.total, 0));
        self.after_eow = text.ends_with(EOW);
        self.dfa.push_str(text);
    }
    fn bow(&mut self) {
        self.absorb_space();
        self.dfa.byte(BOW as u8);
        self.after_eow = false;
    }
    fn digits(&mut self, body: &str) {
        // ASCII digits never share a vocabulary piece with another class, and
        // every one-, two- and three-digit sequence is present.
        self.dfa.total += body.len().div_ceil(3) as isize;
        self.dfa.state = State::ROOT;
        self.space = None;
        self.after_eow = false;
    }
    #[inline]
    fn hard(&mut self, body: &str) {
        // An extra scalar scan costs more than a few DFA transitions on short runs.
        let remaining = if body.len() >= 16 {
            self.independent_scalars(body)
        } else {
            body
        };
        self.push_str(remaining);
    }
    fn space(&mut self, body: &str) {
        if !body.is_ascii() || body == " " && self.after_eow {
            // Keep the checkpoint for a single space that a following BOW may absorb.
            self.push_str(body);
            return;
        }
        let bytes = body.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            let start = offset;
            let byte = bytes[offset];
            while offset < bytes.len() && bytes[offset] == byte {
                offset += 1;
            }
            self.dfa.total += whitespace_cost(byte, offset - start) as isize;
        }
        self.dfa.state = State::ROOT;
        self.space = None;
        self.after_eow = false;
    }
    fn word(&mut self, body: &'a str, fused: bool, bow: bool) {
        if !bow {
            mark_word(self, body, fused, bow);
            return;
        }
        debug_assert!(!fused);
        self.absorb_space();
        if let Some(cost) = super::model::short_word_cost(body) {
            self.dfa.total += cost as isize;
            self.dfa.state = State::ROOT;
            self.after_eow = true;
            return;
        }
        let mut hash = FxHasher::default();
        hash.write(body.as_bytes());
        let slot = hash.finish() as usize % CACHE_SLOTS;
        let cost = if let Some((key, cost)) = self.cache[slot]
            && key == body
        {
            cost
        } else {
            let cost = word_cost(body, self.dfa.vocabulary);
            self.cache[slot] = Some((body, cost));
            cost
        };
        // No vocabulary piece crosses a BOW except its own leading SHIFT, and
        // EOW only occurs at piece ends. Thus this whole marked word is an
        // independent segmentation, even beside punctuation or another word.
        self.dfa.total += cost as isize;
        self.dfa.state = State::ROOT;
        self.after_eow = true;
    }
}

/// A counting cursor also serves as the sink for an independently marked word.
/// Sharing marker emission keeps cached words and streamed contractions identical.
struct DfaCounter<'v> {
    vocabulary: &'v Vocabulary,
    state: State,
    total: isize,
}

impl<'v> DfaCounter<'v> {
    fn new(vocabulary: &'v Vocabulary) -> Self {
        Self {
            vocabulary,
            state: State::ROOT,
            total: 0,
        }
    }

    #[inline]
    fn byte(&mut self, byte: u8) {
        let (state, delta) = self.vocabulary.step(self.state, byte);
        self.state = state;
        self.total += isize::from(delta);
    }

    fn token_count(&self) -> usize {
        usize::try_from(self.total).expect("DFA token count cannot be negative")
    }
}

impl Output<'_> for DfaCounter<'_> {
    #[inline]
    fn push(&mut self, ch: char) {
        self.push_str(ch.encode_utf8(&mut [0; 4]));
    }
    #[inline]
    fn push_str(&mut self, text: &str) {
        for byte in text.bytes() {
            self.byte(byte);
        }
    }
    #[inline]
    fn bow(&mut self) {
        self.byte(BOW as u8);
    }
}

fn word_cost(body: &str, vocabulary: &Vocabulary) -> usize {
    let mut counter = DfaCounter::new(vocabulary);
    mark_word(&mut counter, body, false, true);
    counter.token_count()
}

struct PreparedText<'a> {
    normalized: Cow<'a, str>,
    trailing_newlines: usize,
}

fn prepare(text: &str, config: FamilyConfig) -> PreparedText<'_> {
    // Apply frame absorption to raw text first. A control/private-use character can hide a
    // newline from this step; after normalization that newline still pays the ordinary ladder.
    // Folded spaces (e.g. NBSP) are likewise never absorbed as raw ASCII whitespace.
    let text = match config.frame_tail {
        FrameTail::Keep => text,
        FrameTail::StripTwoNewlines => text.strip_suffix("\n\n").unwrap_or(text),
        FrameTail::StripWhitespace => {
            text.trim_end_matches([' ', '\t', '\n', '\r', '\u{000c}', '\u{000b}'])
        }
    };
    let normalized = normalize(text);
    let trailing_newlines = normalized
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\n')
        .count();
    PreparedText {
        normalized,
        trailing_newlines,
    }
}

fn whitespace_cost(byte: u8, length: usize) -> usize {
    // Spaces and tabs have pieces of every length through 16. Newlines add exact 24- and 32-byte
    // pieces; the remainder cases below are the optimal tiling for that coin system.
    if byte != b'\n' {
        return length.div_ceil(16);
    }
    let full = length / 32;
    full + match length % 32 {
        0 => 0,
        1..=16 | 24 => 1,
        _ => 2,
    }
}

fn frame_tail_cost(newlines: usize) -> usize {
    if newlines == 0 {
        return 0;
    }
    // Two frame newlines join the normalized content tail. One tile is already included
    // in the fixed message frame.
    whitespace_cost(b'\n', newlines + 2) - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::ClaudeVersion;
    use crate::claude::model::vocabulary;

    #[test]
    fn short_word_table_matches_counter() {
        for a in (0..128u8).filter(u8::is_ascii_alphabetic) {
            for b in
                std::iter::once(None).chain((0..128u8).filter(u8::is_ascii_alphabetic).map(Some))
            {
                let mut word = String::from(char::from(a));
                if let Some(b) = b {
                    word.push(char::from(b));
                }
                assert_eq!(
                    super::super::model::short_word_cost(&word),
                    Some(word_cost(&word, vocabulary())),
                    "{word:?}"
                );
            }
        }
    }

    #[test]
    fn run_shortcuts_preserve_boundaries_and_frames() {
        let vocabulary = vocabulary();
        for length in (0..=96).chain([999, 1024]) {
            for run in ["0", "123", " ", "\t", "\n", " \t\n"] {
                for (left, right) in [
                    ("", ""),
                    ("Hello", "World"),
                    ("a'", "s"),
                    ("!", "?"),
                    ("é", "漢"),
                    ("٣", "١"),
                    ("\u{301}", "a"),
                    ("A", "\r"),
                ] {
                    let text = format!("{left}{}{right}", run.repeat(length));
                    for version in [
                        ClaudeVersion::V4_7,
                        ClaudeVersion::V4_8,
                        ClaudeVersion::V5,
                        ClaudeVersion::FableV5_1,
                        ClaudeVersion::V5_5,
                    ] {
                        let config = FamilyConfig::for_version(version);
                        let prepared = prepare(&text, config);
                        let marked =
                            stream_normalized(&prepared.normalized, config, raw_head_space(&text));
                        let mut counter = DfaCounter::new(vocabulary);
                        counter.push_str(&marked);
                        let expected =
                            counter.token_count() + frame_tail_cost(prepared.trailing_newlines);
                        assert_eq!(
                            count(&text, config, vocabulary),
                            expected,
                            "{version:?}: {text:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn preserves_normalization_markers_and_counts() {
        // These detect accidental implementation changes; API truth lives in tests/claude_api.rs.
        // Fingerprint each stage separately over every Unicode scalar, boundary combinations,
        // deterministic fuzz, and whitespace ladder lengths.
        let mut fingerprints = [[0xcbf2_9ce4_8422_2325_u64; 3]; 2];
        let mut inputs = 0;
        let feed = |hash: &mut u64, bytes: &[u8]| {
            for &byte in bytes {
                *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        crate::claude::test_cases::visit(|text| {
            for (index, version) in [ClaudeVersion::V4_7, ClaudeVersion::V5]
                .into_iter()
                .enumerate()
            {
                let config = FamilyConfig::for_version(version);
                let prepared = prepare(text, config);
                let stream = stream_normalized(&prepared.normalized, config, raw_head_space(text));
                for (stage, text) in [prepared.normalized.as_ref(), &stream]
                    .into_iter()
                    .enumerate()
                {
                    feed(
                        &mut fingerprints[index][stage],
                        &(text.len() as u64).to_le_bytes(),
                    );
                    feed(&mut fingerprints[index][stage], text.as_bytes());
                }
                let tokens = config.message_overhead + count(text, config, vocabulary());
                feed(&mut fingerprints[index][2], &(tokens as u64).to_le_bytes());
            }
            inputs += 1;
        });
        // To inspect intentional changes: cargo test preserves_normalization -- --nocapture
        // Review API fixtures and generator checks before copying new values into this assertion.
        eprintln!("stage fingerprints: {fingerprints:#018x?}");
        assert_eq!(inputs, 39_850);
        assert_eq!(
            fingerprints,
            [
                [
                    0x8157_d493_abc3_b1db,
                    0x6965_ac5b_8eef_c7f5,
                    0x0ae8_70e4_6144_fabf,
                ],
                [
                    0x3b01_6046_1451_0d5a,
                    0x3c64_ccdf_7245_01f1,
                    0x8fb4_b9a8_2949_03d2,
                ],
            ]
        );
    }
}
