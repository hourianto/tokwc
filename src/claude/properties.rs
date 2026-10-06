//! Unicode classification rules shared by the runtime and table generator.
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::char::canonical_combining_class;

use super::constants::{
    is_ideographic_punctuation, is_letter, is_mark, is_punct_symbol, is_punctuation, is_separator,
    is_symbol,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Class {
    Wordy,
    Hard,
    Digit,
    Punct,
    Space,
    StrayMark,
}

#[derive(Clone, Copy)]
pub(crate) struct Character(pub(crate) u16);

impl Character {
    pub(crate) fn compute(ch: char) -> Self {
        use unicode_normalization::{IsNormalized, is_nfc_quick};
        let class = classify(ch);
        let cc = canonical_combining_class(ch);
        let safe = cc == 0
            && is_nfc_quick(std::iter::once(ch)) == IsNormalized::Yes
            && !needs_rewrite(ch)
            && ch != '\u{e4d}';
        Self(
            class as u16
                | u16::from(class == Class::Wordy && stray_mark(ch)) << 3
                | u16::from(marks_like_punct(ch)) << 4
                | u16::from(digit_border(ch)) << 5
                | u16::from(get_general_category(ch) == GeneralCategory::DecimalNumber) << 6
                | u16::from(get_general_category(ch) == GeneralCategory::OtherNumber) << 7
                | u16::from(safe) << 8,
        )
    }

    #[inline]
    pub(crate) fn class(self) -> Class {
        match self.0 & 7 {
            0 => Class::Wordy,
            1 => Class::Hard,
            2 => Class::Digit,
            3 => Class::Punct,
            4 => Class::Space,
            _ => Class::StrayMark,
        }
    }
    pub(crate) fn stray(self) -> bool {
        self.0 & 8 != 0
    }
    pub(crate) fn punct(self) -> bool {
        self.0 & 16 != 0
    }
    pub(crate) fn digit_border(self) -> bool {
        self.0 & 32 != 0
    }
    pub(crate) fn decimal(self) -> bool {
        self.0 & 64 != 0
    }
    pub(crate) fn other_number(self) -> bool {
        self.0 & 128 != 0
    }
    pub(crate) fn normal(self) -> bool {
        self.0 & 256 != 0
    }
}

pub(super) fn is_stripped_control(ch: char) -> bool {
    matches!(ch as u32, 0x01..=0x08 | 0x0b..=0x1f | 0x7f..=0x9f)
}

pub(super) fn is_folded_space(ch: char) -> bool {
    matches!(
        ch as u32,
        0x00a0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f
    )
}

pub(super) fn needs_rewrite(ch: char) -> bool {
    is_stripped_control(ch)
        || ch == '\0'
        || is_folded_space(ch)
        || (0xe000..=0xf8ff).contains(&(ch as u32))
}

fn is_hard_codepoint(cp: u32) -> bool {
    cp >= 0x10000
        || (0x4e00..=0x9fff).contains(&cp)
        || (0x3400..=0x4dbf).contains(&cp)
        || (0xf900..=0xfaff).contains(&cp)
        || (0xac00..=0xd7a3).contains(&cp)
        || matches!(cp, 0x3005 | 0x3006 | 0x3031..=0x3035 | 0x303b | 0x303c)
}

// Unicode categories provide the broad split, while the small explicit ranges capture measured
// exceptions where visually similar codepoints take different boundary behavior.
pub(super) fn classify(ch: char) -> Class {
    // Source code and most English text stay on this branch, avoiding Unicode table lookups.
    if ch.is_ascii() {
        return if ch.is_ascii_alphabetic() {
            Class::Wordy
        } else if ch.is_ascii_digit() {
            Class::Digit
        } else if ch.is_ascii_whitespace() {
            Class::Space
        } else if ch.is_ascii_punctuation() {
            Class::Punct
        } else {
            Class::Hard
        };
    }
    if matches!(ch, '\u{1c89}' | '\u{1c8a}' | '\u{a7cb}' | '\u{0264}') {
        return Class::Wordy;
    }
    let cp = ch as u32;
    let category = get_general_category(ch);
    if is_separator(category) || matches!(ch, '\t' | '\n' | '\r' | '\u{000c}' | '\u{000b}') {
        return Class::Space;
    }
    if matches!(
        cp,
        0x16ee..=0x16f0 | 0x2160..=0x2188 | 0x24b6..=0x24e9 | 0xa6e6..=0xa6ef
    ) {
        return Class::Wordy;
    }
    if category == GeneralCategory::DecimalNumber
        && ((0x0660..=0x0669).contains(&cp) || (0x06f0..=0x06f9).contains(&cp))
    {
        return Class::Digit;
    }
    if is_punct_symbol(ch) {
        return Class::Punct;
    }
    if (0xfe00..=0xfe0f).contains(&cp) {
        return Class::Hard;
    }
    if is_mark(category) {
        return if cp < 0x10000 && ch.is_alphabetic() {
            Class::Wordy
        } else {
            Class::Hard
        };
    }
    if is_letter(category) && !is_hard_codepoint(cp) {
        return Class::Wordy;
    }
    Class::Hard
}

// BMP non-Alphabetic marks use the punctuation boundary rules. Alphabetic marks
// remain in words; variation selectors and astral marks use their existing hard path.
pub(super) fn is_separator_mark(ch: char) -> bool {
    (ch as u32) < 0x10000
        && !(0xfe00..=0xfe0f).contains(&(ch as u32))
        && is_mark(get_general_category(ch))
        && !ch.is_alphabetic()
}

pub(super) fn stray_mark(ch: char) -> bool {
    !ch.is_ascii()
        && !syriac_vowel(ch)
        && canonical_combining_class(ch) != 0
        && !is_separator_mark(ch)
}

fn syriac_vowel(ch: char) -> bool {
    matches!(ch as u32, 0x0711 | 0x0730..=0x073f)
}

fn borders(ch: char) -> bool {
    (ch as u32) < 0x10000
}

pub(super) fn digit_border(ch: char) -> bool {
    let category = get_general_category(ch);
    (category == GeneralCategory::OtherNumber
        || (category == GeneralCategory::DecimalNumber && !ch.is_ascii()))
        && borders(ch)
}

pub(super) fn marks_like_punct(ch: char) -> bool {
    if (ch as u32) >= 0x10000 {
        return false;
    }
    if is_separator_mark(ch) {
        return true;
    }
    let category = get_general_category(ch);
    (is_punctuation(category)
        || is_symbol(category)
        || matches!(
            category,
            GeneralCategory::Format | GeneralCategory::Unassigned
        ))
        && !is_ideographic_punctuation(ch)
}
