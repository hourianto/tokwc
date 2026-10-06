use unicode_general_category::{GeneralCategory, get_general_category};

// Normalization removes these control characters from input, leaving them available as compact
// single-byte internal markers.
pub(crate) const BOW: char = '\u{1}';
pub(crate) const EOW: char = '\u{2}';
pub(crate) const SHIFT: char = '\u{3}';

pub(crate) const CONTRACTIONS: [&str; 7] = ["s", "t", "d", "m", "ll", "re", "ve"];

pub(crate) fn is_punct_symbol(ch: char) -> bool {
    matches!(
        ch,
        '£' | '§'
            | '«'
            | '°'
            | '·'
            | '»'
            | '།'
            | '–'
            | '—'
            | '„'
            | '†'
            | '•'
            | '…'
            | '€'
            | '№'
            | '→'
            | '−'
            | '√'
            | '─'
            | '│'
            | '└'
            | '═'
            | '█'
            | '（'
    )
}

pub(crate) fn is_letter(category: GeneralCategory) -> bool {
    matches!(
        category,
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
    )
}

pub(crate) fn is_mark(category: GeneralCategory) -> bool {
    matches!(
        category,
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}

pub(crate) fn is_punctuation(category: GeneralCategory) -> bool {
    matches!(
        category,
        GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation
    )
}

pub(crate) fn is_symbol(category: GeneralCategory) -> bool {
    matches!(
        category,
        GeneralCategory::MathSymbol
            | GeneralCategory::CurrencySymbol
            | GeneralCategory::ModifierSymbol
            | GeneralCategory::OtherSymbol
    )
}

pub(crate) fn is_separator(category: GeneralCategory) -> bool {
    matches!(
        category,
        GeneralCategory::SpaceSeparator
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator
    )
}

pub(crate) fn is_ideographic_punctuation(ch: char) -> bool {
    (0x3001..=0x303f).contains(&(ch as u32))
        && (is_punctuation(get_general_category(ch))
            // These category-So symbols have the same markerless behavior as the block's
            // punctuation characters.
            || matches!(ch, '〄' | '〒' | '〓' | '〠' | '〶' | '〷' | '〾' | '〿'))
}
