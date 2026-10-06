use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

use super::constants::SHIFT;
use super::properties::{is_folded_space, is_stripped_control};

/// Claude applies NFC, removes most control/BMP-private-use characters, and folds a small measured
/// set of separators to ASCII space. Thai SARA AM is the one composition NFC itself does not make.
pub(crate) fn normalize(text: &str) -> Cow<'_, str> {
    if text.is_ascii() {
        // OR reduction lets the compiler scan clean ASCII in vector-sized blocks.
        // CRLF source files need only a bulk-copy rewrite, not scalar-by-scalar NFC.
        let flags = text.bytes().fold(0u8, |flags, byte| {
            flags
                | u8::from(byte == b'\r')
                | (u8::from(matches!(byte, 0..=8 | 11..=12 | 14..=31 | 127)) << 1)
        });
        return match flags {
            0 => Cow::Borrowed(text),
            1 => {
                let mut output = String::with_capacity(text.len());
                let mut start = 0;
                for offset in memchr::memchr_iter(b'\r', text.as_bytes()) {
                    output.push_str(&text[start..offset]);
                    start = offset + 1;
                }
                output.push_str(&text[start..]);
                Cow::Owned(output)
            }
            _ => Cow::Owned(rewrite_normalized(text)),
        };
    }

    // A scalar with NFC_QC=Yes and combining class zero starts an independent NFC
    // segment. Only normalize the spans containing exceptional scalars, retaining
    // the preceding starter so composition and canonical ordering remain exact.
    let mut output = String::new();
    let mut copied = 0;
    let mut previous_offset = 0;
    let mut previous = '\0';
    let mut dirty = None;
    let characters = super::character::table();
    for (offset, ch) in text.char_indices() {
        let safe = characters.get(ch).normal() && !(previous == '\u{e4d}' && ch == '\u{e32}');
        if !safe && dirty.is_none() {
            let start = previous_offset;
            if output.capacity() == 0 {
                output.reserve(text.len());
            }
            output.push_str(&text[copied..start]);
            dirty = Some(start);
        } else if safe && let Some(start) = dirty.take() {
            rewrite_into(text[start..offset].nfc(), &mut output);
            copied = offset;
        }
        previous_offset = offset;
        previous = ch;
    }
    if let Some(start) = dirty {
        rewrite_into(text[start..].nfc(), &mut output);
    } else if output.capacity() != 0 {
        output.push_str(&text[copied..]);
    } else {
        return Cow::Borrowed(text);
    }
    Cow::Owned(output)
}

fn rewrite_normalized(normalized: &str) -> String {
    let mut output = String::with_capacity(normalized.len());
    rewrite_into(normalized.chars(), &mut output);
    output
}

fn rewrite_into(chars: impl Iterator<Item = char>, output: &mut String) {
    let mut chars = chars.peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{0e4d}' && chars.peek() == Some(&'\u{0e32}') {
            chars.next();
            output.push('\u{0e33}');
            continue;
        }
        if is_stripped_control(ch) || (0xe000..=0xf8ff).contains(&(ch as u32)) {
            continue;
        }
        if ch == '\0' || is_folded_space(ch) {
            output.push(' ');
        } else {
            output.push(ch);
        }
    }
}

fn lower_char(ch: char, output: &mut String) {
    match ch {
        // Claude treats this Unicode uppercase character as uncased.
        '\u{03f4}' => output.push(ch),
        '\u{1c89}' => output.push('\u{1c8a}'),
        '\u{a7cb}' => output.push('\u{0264}'),
        _ => output.extend(ch.to_lowercase()),
    }
}

pub(super) fn ascii_case_marked(span: &str, head_mark: bool) -> bool {
    !head_mark
        && span.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && !span.as_bytes()[1..].iter().any(u8::is_ascii_uppercase)
}

fn is_upper(ch: char) -> bool {
    ch != '\u{03f4}'
        && (matches!(ch, '\u{1c89}' | '\u{a7cb}') || ch.is_uppercase())
        && has_lowercase_mapping(ch)
}

fn has_lowercase_mapping(ch: char) -> bool {
    if matches!(ch, '\u{1c89}' | '\u{a7cb}') {
        return true;
    }
    let mut lowered = ch.to_lowercase();
    lowered.next() != Some(ch) || lowered.next().is_some()
}

pub(super) fn mark_case(span: &str, head_mark: bool) -> Cow<'_, str> {
    if head_mark {
        return Cow::Borrowed(span);
    }
    if span.is_ascii() {
        if ascii_case_marked(span, false) {
            let mut output = String::with_capacity(span.len() + SHIFT.len_utf8());
            output.push(SHIFT);
            output.extend(
                span.as_bytes()
                    .iter()
                    .map(|byte| byte.to_ascii_lowercase() as char),
            );
            return Cow::Owned(output);
        }
        return Cow::Borrowed(span);
    }
    let mut chars = span.chars();
    let Some(head) = chars.next() else {
        return Cow::Borrowed(span);
    };
    // A shift requires a changing uppercase head. Reject other words before scanning their tails
    // for the sharp-S and dotted-I exceptions (especially common in lowercase Unicode prose).
    if !is_upper(head) || matches!(head, 'İ' | 'ẞ') {
        return Cow::Borrowed(span);
    }
    if span.contains(['İ', 'ẞ']) {
        let tail = chars;
        if !tail
            .clone()
            .any(|ch| !matches!(ch, 'İ' | 'ẞ') && is_upper(ch))
        {
            let mut output = String::with_capacity(span.len() + SHIFT.len_utf8());
            output.push(SHIFT);
            lower_char(head, &mut output);
            for ch in tail {
                if matches!(ch, 'İ' | 'ẞ') {
                    output.push(ch);
                } else {
                    lower_char(ch, &mut output);
                }
            }
            return Cow::Owned(output);
        }
        return Cow::Borrowed(span);
    }

    if !chars.any(is_upper) {
        let mut output = String::with_capacity(span.len() + SHIFT.len_utf8());
        output.push(SHIFT);
        for ch in span.chars() {
            lower_char(ch, &mut output);
        }
        return Cow::Owned(output);
    }
    Cow::Borrowed(span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_the_measured_folds() {
        assert_eq!(normalize("ทํางาน\0a\u{a0}b\u{e000}c"), "ทำงาน a bc");
        assert_eq!(normalize("привет\r\n"), "привет\n");
        assert_eq!(normalize("don’t"), "don’t");
    }

    #[test]
    fn fused_scan_preserves_composition_and_rewrite_order() {
        for text in [
            "",
            "plain ASCII",
            "Привет, мир!",
            "é",
            "e\u{301}",
            "A\u{30a}",
            "A\u{1}\u{30a}",
            "\u{301}\u{323}",
            "ทํางาน",
            "ทํ\u{1}างาน",
            "\u{e4d}\u{e32}\u{e4d}\u{e32}",
            "\u{a0}e\u{301}",
            "\u{e000}a",
            "\u{340}",
            "\u{1100}\u{1161}\u{11a8}",
        ] {
            let nfc = text.nfc().collect::<String>();
            assert_eq!(normalize(text), rewrite_normalized(&nfc), "{text:?}");
        }
        // Removed controls must not cause a second round of composition.
        assert_eq!(normalize("A\u{1}\u{30a}"), "A\u{30a}");
        assert_eq!(normalize("ทํ\u{1}างาน"), "ทํางาน");
    }

    #[test]
    fn does_not_mark_uppercase_characters_without_lowercase_mappings() {
        let characters = "ϒϓϔℂℇℋℌℍℐℑℒℕℙℚℛℜℝℤℨℬℭℰℱℳℾℿⅅ";
        for character in characters.chars() {
            let mut span = String::from(character);
            span.push_str("wordİ");
            assert_eq!(mark_case(&span, false), span);
        }

        assert_eq!(mark_case("Ωmega", false), format!("{SHIFT}ωmega"));
    }

    #[test]
    fn case_head_shortcut_preserves_unicode_exceptions() {
        for text in ["abİ", "İstanbul", "ẞtraße", "AẞA", "ϴabc", "ϒabcİ", "ΩΜega"] {
            assert_eq!(mark_case(text, false), text);
        }
        assert_eq!(mark_case("Ωİ", false), format!("{SHIFT}ωİ"));
        assert_eq!(mark_case("Aİbc", false), format!("{SHIFT}aİbc"));
        assert_eq!(mark_case("Aẞbc", false), format!("{SHIFT}aẞbc"));
        assert_eq!(mark_case("\u{1c89}bc", false), format!("{SHIFT}\u{1c8a}bc"));
        assert_eq!(mark_case("\u{a7cb}bc", false), format!("{SHIFT}\u{0264}bc"));
        assert_eq!(mark_case("Ωmega", true), "Ωmega");
    }
}

#[cfg(test)]
#[test]
fn segmented_nfc_matches_full_normalization() {
    crate::claude::test_cases::visit(|text| {
        let nfc = text.nfc().collect::<String>();
        assert_eq!(normalize(text), rewrite_normalized(&nfc), "{text:?}");
    });
}
