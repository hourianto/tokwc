//! Token counting for the supported Claude models.
//!
//! Use [`crate::Tokenizer`] for an API shared with OpenAI encodings. This module
//! also exposes direct selection of Claude message profiles.
//!
//! The implementation and vocabulary are adapted from
//! [ctok](https://github.com/sanderland/ctok), licensed under the MIT License. The original license
//! text is included in `src/claude/data/LICENSE.claudetok`.

mod character;
mod compression;
mod constants;
mod engine;
mod model;
mod normalize;
mod properties;
mod stream;

#[cfg(test)]
#[path = "../../tests/support/claude_cases.rs"]
mod test_cases;

use std::error::Error;
use std::fmt;

use model::{FamilyConfig, vocabulary};

/// Claude message-framing and whitespace rules for a shared vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClaudeVersion {
    /// Opus 4.7: eleven-token frame with trailing whitespace preserved.
    V4_7,
    /// Opus 4.8, Sonnet 5, and Fable 5: six-token frame with a newline tail.
    V4_8,
    /// Opus 5: six-token frame that absorbs raw trailing ASCII whitespace.
    V5,
    /// Fable 5.1: Opus 5 whitespace behavior with an eight-token frame.
    FableV5_1,
    /// Opus 5.5 and Sonnet 5.5: eight-token frame that absorbs raw trailing ASCII whitespace.
    V5_5,
}

impl ClaudeVersion {
    /// Resolve a supported model name, optionally followed by an eight-digit snapshot date.
    /// Unknown families and versions are rejected rather than assigned approximate rules.
    ///
    /// # Errors
    ///
    /// Returns [`ClaudeVersionError`] if the model name or its suffix is unsupported.
    pub fn for_model(model: &str) -> Result<Self, ClaudeVersionError> {
        let name = model
            .rsplit_once('-')
            .filter(|(_, suffix)| {
                suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
            .map_or(model, |(name, _)| name);
        match name {
            "claude-opus-4-7" => Ok(Self::V4_7),
            "claude-opus-4-8" | "claude-sonnet-5" | "claude-fable-5" => Ok(Self::V4_8),
            "claude-opus-5" => Ok(Self::V5),
            "claude-fable-5-1" => Ok(Self::FableV5_1),
            "claude-opus-5-5" | "claude-sonnet-5-5" => Ok(Self::V5_5),
            _ => Err(ClaudeVersionError::new(model)),
        }
    }

    /// Canonical profile name accepted by [`crate::Tokenizer::for_encoding`].
    #[must_use]
    pub const fn encoding_name(self) -> &'static str {
        match self {
            Self::V4_7 => "claude_v4_7",
            Self::V4_8 => "claude_v4_8",
            Self::V5 => "claude_v5",
            Self::FableV5_1 => "claude_fable_v5_1",
            Self::V5_5 => "claude_v5_5",
        }
    }
}

/// An unsupported Claude model name or snapshot suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeVersionError {
    requested: String,
}

impl ClaudeVersionError {
    fn new(requested: &str) -> Self {
        Self {
            requested: requested.to_owned(),
        }
    }
}

impl fmt::Display for ClaudeVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unsupported Claude model {:?}", self.requested)
    }
}

impl Error for ClaudeVersionError {}

/// A counter for one Claude message profile, sharing vocabulary tables across instances.
#[derive(Debug, Clone, Copy)]
pub struct ClaudeTokenizer {
    version: ClaudeVersion,
}

impl ClaudeTokenizer {
    /// Select a Claude message profile without loading its vocabulary yet.
    #[must_use]
    pub const fn new(version: ClaudeVersion) -> Self {
        Self { version }
    }

    /// Counts one user message, including the family-specific message frame.
    #[must_use]
    pub fn token_count(self, text: &str) -> usize {
        let config = FamilyConfig::for_version(self.version);
        config.message_overhead + engine::count(text, config, vocabulary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_supported_model_families() {
        assert_eq!(
            ClaudeVersion::for_model("claude-opus-4-7-20260814"),
            Ok(ClaudeVersion::V4_7)
        );
        assert_eq!(
            ClaudeVersion::for_model("claude-sonnet-5"),
            Ok(ClaudeVersion::V4_8)
        );
        assert!(ClaudeVersion::for_model("claude-3-5-sonnet").is_err());
        for model in ["claude-opus-4-8", "claude-fable-5-20260904"] {
            assert_eq!(ClaudeVersion::for_model(model), Ok(ClaudeVersion::V4_8));
        }
        assert_eq!(
            ClaudeVersion::for_model("claude-opus-5"),
            Ok(ClaudeVersion::V5)
        );
        assert_eq!(
            ClaudeVersion::for_model("claude-fable-5-1-20260904"),
            Ok(ClaudeVersion::FableV5_1)
        );
        for model in [
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-opus-5-5-20260921",
            "claude-sonnet-5-5-20260928",
        ] {
            assert_eq!(ClaudeVersion::for_model(model), Ok(ClaudeVersion::V5_5));
        }
        assert_eq!(
            ClaudeVersion::for_model("claude-opus-5-20260724"),
            Ok(ClaudeVersion::V5)
        );
        assert_eq!(
            ClaudeVersion::for_model("claude-sonnet-5-20260629"),
            Ok(ClaudeVersion::V4_8)
        );
        assert!(ClaudeVersion::for_model("claude-opus-4-20260904").is_err());
    }

    #[test]
    fn counts_both_supported_frames() {
        assert_eq!(
            ClaudeTokenizer::new(ClaudeVersion::V4_7).token_count("hello, world"),
            15
        );
        assert_eq!(
            ClaudeTokenizer::new(ClaudeVersion::V5).token_count("hello, world"),
            10
        );
    }

    #[test]
    fn counts_uppercase_characters_without_lowercase_mappings() {
        let ordinary = "ϒϓϔℂℇℋℌℍℐℑℒℕℙℚℛℜℝℤℨℬℭℰℱℳℾℿ";
        for character in ordinary.chars() {
            let text = character.to_string();
            assert_eq!(
                ClaudeTokenizer::new(ClaudeVersion::V4_7).token_count(&text),
                15
            );
            assert_eq!(
                ClaudeTokenizer::new(ClaudeVersion::V5).token_count(&text),
                10
            );
        }

        assert_eq!(
            ClaudeTokenizer::new(ClaudeVersion::V4_7).token_count("ⅅ"),
            16
        );
        assert_eq!(ClaudeTokenizer::new(ClaudeVersion::V5).token_count("ⅅ"), 11);
    }

    #[test]
    fn counts_confirmed_vocabulary_and_classification_updates() {
        let tokenizer = ClaudeTokenizer::new(ClaudeVersion::V4_7);
        for (text, expected) in [
            (".ヲււヲ.", 20),
            ("1././1", 15),
            ("a␀a", 15),
            ("ϴ", 15),
            ("漢〱漢", 18),
            ("5 〄 5", 18),
        ] {
            assert_eq!(tokenizer.token_count(text), expected, "{text:?}");
        }
    }
}
