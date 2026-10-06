use super::character::{Character, CharacterTable};

use super::constants::{BOW, CONTRACTIONS, EOW, SHIFT};
use super::model::FamilyConfig;
use super::normalize::{ascii_case_marked, mark_case};

use super::properties::Class;

#[derive(Debug, Clone, Copy)]
struct Run<'a> {
    class: Class,
    body: &'a str,
}

struct AsciiRuns<'a> {
    remaining: &'a str,
}

impl<'a> Iterator for AsciiRuns<'a> {
    type Item = Run<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.remaining.as_bytes();
        let &first = bytes.first()?;
        let class = classify_ascii(first);
        let mut end = 1;
        match class {
            Class::Wordy => {
                while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
                    end += 1;
                }
            }
            Class::Digit => {
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            _ => {
                while end < bytes.len() && classify_ascii(bytes[end]) == class {
                    end += 1;
                }
            }
        }
        let (body, remaining) = self.remaining.split_at(end);
        self.remaining = remaining;
        Some(Run { class, body })
    }
}

struct UnicodeRuns<'a> {
    remaining: &'a str,
    characters: &'static CharacterTable,
}

impl<'a> Iterator for UnicodeRuns<'a> {
    type Item = Run<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        // Most runs in a Unicode document are still entirely ASCII. Scan them
        // by byte, only decoding scalars when a non-ASCII byte can extend the run.
        let bytes = self.remaining.as_bytes();
        if let Some(&first) = bytes.first()
            && first.is_ascii()
        {
            let class = classify_ascii(first);
            let mut end = 1;
            while end < bytes.len() && bytes[end].is_ascii() && classify_ascii(bytes[end]) == class
            {
                end += 1;
            }
            if end == bytes.len() || bytes[end].is_ascii() {
                let (body, remaining) = self.remaining.split_at(end);
                self.remaining = remaining;
                return Some(Run { class, body });
            }
        }
        let mut chars = self.remaining.char_indices();
        let (_, first) = chars.next()?;
        let info = self.characters.get(first);
        let class = if info.stray() {
            Class::StrayMark
        } else {
            info.class()
        };
        let kind = if class == Class::Hard {
            hard_kind(info)
        } else {
            HardKind::Letter
        };
        let mut end = self.remaining.len();
        for (offset, ch) in chars {
            let info = self.characters.get(ch);
            if !(info.class() == class
                || class == Class::StrayMark && info.class() == Class::Wordy && info.stray())
                || class == Class::Hard && !is_selector(ch) && hard_kind(info) != kind
            {
                end = offset;
                break;
            }
        }
        let (body, remaining) = self.remaining.split_at(end);
        self.remaining = remaining;
        Some(Run { class, body })
    }
}

fn opens(run: Run<'_>, next: Option<Run<'_>>) -> bool {
    run.body == "'"
        && next.is_some_and(|next| matches!(next.class, Class::Wordy | Class::StrayMark))
}

fn is_selector(ch: char) -> bool {
    (0xfe00..=0xfe0f).contains(&(ch as u32))
}

fn hard_bow(body: &str) -> bool {
    body.chars()
        .next()
        .is_some_and(|ch| is_selector(ch) || Character::get(ch).punct())
}

fn hard_eow(body: &str) -> bool {
    body.chars()
        .next_back()
        .is_some_and(|ch| is_selector(ch) || Character::get(ch).punct())
}

fn takes_right_border(run: &Run) -> bool {
    run.class == Class::Punct
        || hard_eow(run.body)
        || (matches!(run.class, Class::Digit | Class::Hard) && digit_eow(run.body))
}

fn nd_run(body: &str) -> bool {
    let characters = super::character::table();
    body.chars().all(|ch| characters.get(ch).decimal())
}

fn no_run(body: &str) -> bool {
    let characters = super::character::table();
    !body.is_empty() && body.chars().all(|ch| characters.get(ch).other_number())
}

fn digit_run(body: &str) -> bool {
    if body.is_ascii() {
        body.bytes().all(|byte| byte.is_ascii_digit())
    } else {
        nd_run(body) || no_run(body)
    }
}

fn digit_bow(body: &str) -> bool {
    body.chars()
        .next()
        .is_some_and(|ch| Character::get(ch).digit_border())
        && digit_run(body)
}

fn digit_eow(body: &str) -> bool {
    body.chars()
        .next_back()
        .is_some_and(|ch| Character::get(ch).digit_border())
        && digit_run(body)
}

#[inline]
fn classify_ascii(byte: u8) -> Class {
    static CLASSES: [Class; 256] = {
        let mut classes = [Class::Hard; 256];
        let mut byte = 0;
        while byte < 256 {
            classes[byte] = match byte as u8 {
                b'A'..=b'Z' | b'a'..=b'z' => Class::Wordy,
                b'0'..=b'9' => Class::Digit,
                b'\t'..=b'\r' | b' ' => Class::Space,
                b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~' => Class::Punct,
                _ => Class::Hard,
            };
            byte += 1;
        }
        classes
    };
    CLASSES[byte as usize]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardKind {
    Punct,
    Number,
    Letter,
}

fn hard_kind(info: Character) -> HardKind {
    if info.punct() {
        HardKind::Punct
    } else if info.digit_border() {
        HardKind::Number
    } else {
        HardKind::Letter
    }
}

pub(crate) fn raw_head_space(text: &str) -> bool {
    text.starts_with(' ')
}

#[cfg(test)]
pub(crate) fn stream_normalized(
    normalized: &str,
    config: FamilyConfig,
    raw_head_space: bool,
) -> String {
    let mut output = String::with_capacity(normalized.len() + normalized.len() / 4 + 4);
    emit_normalized(normalized, config, raw_head_space, &mut output);
    output
}

pub(super) fn emit_normalized<'a>(
    normalized: &'a str,
    config: FamilyConfig,
    raw_head_space: bool,
    output: &mut impl Output<'a>,
) {
    let mut protected = normalized.trim_end_matches('\n');
    if config.frame_bow
        && raw_head_space
        && protected.starts_with(' ')
        && !protected[1..].starts_with(' ')
    {
        protected = &protected[1..];
    }

    if protected.is_ascii() {
        emit_ascii(protected, config, output);
        return;
    }

    emit_general(protected, config, output);
}

#[cfg(test)]
fn stream_general(protected: &str, config: FamilyConfig) -> String {
    let mut output = String::new();
    emit_general(protected, config, &mut output);
    output
}

fn emit_general<'a>(protected: &'a str, config: FamilyConfig, output: &mut impl Output<'a>) {
    let mut runs = UnicodeRuns {
        remaining: protected,
        characters: super::character::table(),
    };
    let Some(mut run) = runs.next() else {
        if config.frame_bow {
            output.bow();
        }
        return;
    };
    let mut next = runs.next();
    let first = &run;
    let head_quote = opens(run, next);
    let has_own_bow = !head_quote
        && (matches!(first.class, Class::Wordy | Class::Punct | Class::StrayMark)
            || hard_bow(first.body)
            || (matches!(first.class, Class::Digit | Class::Hard) && digit_bow(first.body))
            || (first.class == Class::Space && first.body.starts_with(' ')));

    if !has_own_bow && config.frame_bow {
        if head_quote {
            output.push(' ');
        } else {
            output.bow();
        }
    }

    let mut previous: Option<Run<'_>> = None;
    let mut before_previous: Option<Run<'_>> = None;
    loop {
        let borders_space = |side: isize| -> bool {
            let neighbor = if side < 0 {
                let Some(previous) = previous else {
                    return config.frame_bow;
                };
                previous
            } else {
                let Some(next) = next else {
                    return false;
                };
                next
            };
            if neighbor.class != Class::Space {
                return false;
            }
            if side < 0 {
                neighbor.body.ends_with(' ')
            } else {
                neighbor.body.starts_with(' ') && !neighbor.body.starts_with("  ")
            }
        };

        match run.class {
            Class::Wordy => {
                let fused = previous.is_some_and(|prev| prev.class == Class::StrayMark);
                let contraction = CONTRACTIONS.contains(&run.body)
                    && previous.is_some_and(|prev| prev.class == Class::Punct && prev.body == "'")
                    && !before_previous.is_some_and(|prev| takes_right_border(&prev));
                output.word(run.body, fused, !fused && !contraction);
            }
            Class::StrayMark => {
                output.bow();
                output.push_str(run.body);
                if !next.is_some_and(|next| next.class == Class::Wordy) {
                    output.push(EOW);
                }
            }
            Class::Punct => {
                if borders_space(-1) && !opens(run, next) {
                    output.bow();
                }
                output.push_str(run.body);
                if borders_space(1) {
                    output.push(EOW);
                }
            }
            Class::Hard if hard_bow(run.body) || hard_eow(run.body) => {
                if borders_space(-1) && !opens(run, next) && hard_bow(run.body) {
                    output.bow();
                }
                output.push_str(run.body);
                if borders_space(1) && hard_eow(run.body) {
                    output.push(EOW);
                }
            }
            Class::Space => output.space(run.body),
            Class::Digit if run.body.is_ascii() => output.digits(run.body),
            Class::Digit | Class::Hard if digit_run(run.body) => {
                if digit_bow(run.body) && borders_space(-1) {
                    output.bow();
                }
                output.push_str(run.body);
                if digit_eow(run.body) && borders_space(1) {
                    output.push(EOW);
                }
            }
            Class::Hard => output.hard(run.body),
            Class::Digit => output.push_str(run.body),
        }
        before_previous = previous;
        previous = Some(run);
        let Some(following) = next else {
            break;
        };
        run = following;
        next = runs.next();
    }
}

#[cfg(test)]
fn stream_ascii(text: &str, config: FamilyConfig) -> String {
    let mut output = String::new();
    emit_ascii(text, config, &mut output);
    output
}

fn emit_ascii<'a>(text: &'a str, config: FamilyConfig, output: &mut impl Output<'a>) {
    // Only two preceding runs and one following run affect ASCII boundaries. Keep that window
    // instead of allocating and revisiting a run array proportional to the whole document.
    let mut runs = AsciiRuns { remaining: text };
    let Some(mut run) = runs.next() else {
        if config.frame_bow {
            output.bow();
        }
        return;
    };
    let mut next = runs.next();
    let head_quote = ascii_opens_word(run, next);
    let has_own_bow = !head_quote
        && (matches!(run.class, Class::Wordy | Class::Punct)
            || (run.class == Class::Space && run.body.starts_with(' ')));

    if !has_own_bow && config.frame_bow {
        if head_quote {
            output.push(' ');
        } else {
            output.bow();
        }
    }
    let mut previous: Option<Run<'_>> = None;
    let mut before_previous: Option<Run<'_>> = None;
    loop {
        let body = run.body;
        match run.class {
            Class::Wordy => {
                let contraction_seam = previous.is_some_and(|prev| {
                    prev.class == Class::Punct
                        && prev.body == "'"
                        && CONTRACTIONS.contains(&body)
                        && !before_previous.is_some_and(|before| before.class == Class::Punct)
                });
                output.word(body, false, !contraction_seam);
            }
            Class::Punct => {
                let left_space = previous.map_or(config.frame_bow, |prev| {
                    prev.class == Class::Space && prev.body.ends_with(' ')
                });
                if left_space && !ascii_opens_word(run, next) {
                    output.bow();
                }
                output.push_str(body);
                if next.is_some_and(|next| {
                    next.class == Class::Space
                        && next.body.starts_with(' ')
                        && !next.body.starts_with("  ")
                }) {
                    output.push(EOW);
                }
            }
            Class::Space => output.space(body),
            Class::Digit => output.digits(body),
            _ => output.push_str(body),
        }
        before_previous = previous;
        previous = Some(run);
        let Some(following) = next else {
            break;
        };
        run = following;
        next = runs.next();
    }
}

fn ascii_opens_word(run: Run<'_>, next: Option<Run<'_>>) -> bool {
    run.body == "'" && next.is_some_and(|next| next.class == Class::Wordy)
}

pub(super) trait Output<'a> {
    fn push(&mut self, ch: char);
    fn push_str(&mut self, text: &str);
    fn bow(&mut self);
    fn hard(&mut self, body: &str) {
        self.push_str(body);
    }
    fn space(&mut self, body: &str) {
        self.push_str(body);
    }
    fn digits(&mut self, body: &str) {
        self.push_str(body);
    }
    fn word(&mut self, body: &'a str, fused: bool, bow: bool) {
        mark_word(self, body, fused, bow);
    }
}

#[inline]
pub(super) fn mark_word<'a>(
    output: &mut (impl Output<'a> + ?Sized),
    body: &str,
    fused: bool,
    bow: bool,
) {
    if body.is_ascii() {
        push_ascii_word(output, body, ascii_case_marked(body, fused), bow);
    } else {
        let marked = mark_case(body, fused);
        let marked = if let Some(body) = marked.strip_prefix(SHIFT) {
            output.push(SHIFT);
            body
        } else {
            &marked
        };
        if bow {
            output.bow();
        }
        output.push_str(marked);
        output.push(EOW);
    }
}

impl Output<'_> for String {
    fn push(&mut self, ch: char) {
        String::push(self, ch);
    }
    fn push_str(&mut self, text: &str) {
        String::push_str(self, text);
    }
    fn bow(&mut self) {
        push_bow(self);
    }
}

fn push_ascii_word<'a>(
    output: &mut (impl Output<'a> + ?Sized),
    body: &str,
    shifted: bool,
    bow: bool,
) {
    if shifted {
        output.push(SHIFT);
    }
    if bow {
        output.bow();
    }
    if shifted {
        // Only the head is uppercase when an ASCII word takes a shift.
        output.push(body.as_bytes()[0].to_ascii_lowercase() as char);
        output.push_str(&body[1..]);
    } else {
        output.push_str(body);
    }
    output.push(EOW);
}

fn push_bow(output: &mut String) {
    // One ASCII space between a closing and opening word boundary is represented by the boundary
    // pair itself. Remove that space when its following BOW is emitted instead of rewriting the
    // entire marked stream afterward.
    let remove_at = {
        let bytes = output.as_bytes();
        let mut offset = bytes.len();
        while offset > 0 && bytes[offset - 1] == SHIFT as u8 {
            offset -= 1;
        }
        (offset >= 2 && bytes[offset - 1] == b' ' && bytes[offset - 2] == EOW as u8)
            .then(|| offset - 1)
    };
    if let Some(offset) = remove_at {
        output.remove(offset);
    }
    output.push(BOW);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::ClaudeVersion;

    fn v47() -> FamilyConfig {
        FamilyConfig::for_version(ClaudeVersion::V4_7)
    }

    fn mark(text: &str) -> String {
        stream_normalized(
            &crate::claude::normalize::normalize(text),
            v47(),
            raw_head_space(text),
        )
    }

    #[test]
    fn marks_words_punctuation_and_seams() {
        assert_eq!(
            mark("hello, world"),
            format!("{BOW}hello{EOW},{EOW}{BOW}world{EOW}")
        );
    }

    #[test]
    fn separates_terminal_marks() {
        assert_eq!(mark("क्ष"), format!("{BOW}क{EOW}्{BOW}ष{EOW}"));
    }

    #[test]
    fn ascii_stream_matches_general_marker_rules() {
        let check = |text: &str| {
            let normalized = crate::claude::normalize::normalize(text);
            if normalized.is_ascii() {
                for version in [
                    ClaudeVersion::V4_7,
                    ClaudeVersion::V4_8,
                    ClaudeVersion::V5,
                    ClaudeVersion::FableV5_1,
                    ClaudeVersion::V5_5,
                ] {
                    let config = FamilyConfig::for_version(version);
                    assert_eq!(
                        stream_ascii(&normalized, config),
                        stream_general(&normalized, config),
                        "{version:?}: {text:?}"
                    );
                }
            }
        };
        crate::claude::test_cases::visit(check);
        // Every ASCII pair around apostrophes, title case, and a whitespace boundary.
        for a in 0..128u8 {
            for b in 0..128u8 {
                check(&format!("{}'s {}Hello're\n", char::from(a), char::from(b)));
            }
        }
    }
}
