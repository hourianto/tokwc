use std::fmt;
use std::str::FromStr;

use crate::TokenizerError;
use crate::claude::ClaudeVersion;

/// Provider whose counting rules an encoding uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    /// OpenAI text-token counts, without chat-message framing.
    OpenAi,
    /// Anthropic counts for one plain-text user message, including its frame.
    Anthropic,
}

impl Provider {
    /// Lowercase provider name, also used in CLI JSON output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

/// An OpenAI encoding or Claude message-counting profile.
///
/// Claude profiles share a vocabulary but apply different message overhead and
/// whitespace rules. The default is [`Encoding::O200kBase`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// OpenAI's cl100k text encoding, used by GPT-4 and GPT-3.5 Turbo.
    Cl100kBase,
    /// OpenAI's o200k text encoding, used by GPT-6.1 Sol and GPT-4o.
    #[default]
    O200kBase,
    /// The o200k encoding with Harmony special tokens, used by gpt-oss.
    O200kHarmony,
    /// A Claude message profile, reusing the engine's version selection.
    Claude(ClaudeVersion),
}

impl Encoding {
    /// All supported encodings and profiles, in CLI listing order.
    pub const ALL: [Self; 8] = [
        Self::O200kBase,
        Self::O200kHarmony,
        Self::Cl100kBase,
        Self::Claude(ClaudeVersion::V4_7),
        Self::Claude(ClaudeVersion::V4_8),
        Self::Claude(ClaudeVersion::V5),
        Self::Claude(ClaudeVersion::FableV5_1),
        Self::Claude(ClaudeVersion::V5_5),
    ];

    /// Canonical name accepted by [`crate::Tokenizer::for_encoding`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::O200kHarmony => "o200k_harmony",
            Self::Cl100kBase => "cl100k_base",
            Self::Claude(version) => version.encoding_name(),
        }
    }

    /// Provider whose rules this encoding implements.
    #[must_use]
    pub const fn provider(self) -> Provider {
        match self {
            Self::Claude(_) => Provider::Anthropic,
            Self::Cl100kBase | Self::O200kBase | Self::O200kHarmony => Provider::OpenAi,
        }
    }

    /// Resolve a supported model name without loading any vocabulary tables.
    ///
    /// # Errors
    /// Returns [`TokenizerError`] for an unknown model or an unsupported encoding.
    pub fn for_model(model: &str) -> Result<Self, TokenizerError> {
        if model.starts_with("claude-") {
            return ClaudeVersion::for_model(model)
                .map(Self::Claude)
                .map_err(TokenizerError::UnsupportedClaudeModel);
        }
        // These model IDs are not yet in tiktoken 4.1.2's lookup table.
        let name = match model {
            "gpt-6.1-sol" | "gpt-6-astra" | "gpt-6-sol" | "gpt-6-luna" => Some("o200k_base"),
            _ => tiktoken::model_to_encoding(model),
        }
        .ok_or_else(|| TokenizerError::UnknownModel(model.to_owned()))?;
        name.parse().map_err(
            |_unknown_encoding| TokenizerError::UnsupportedModelEncoding {
                model: model.to_owned(),
                encoding: name.to_owned(),
            },
        )
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Encoding {
    type Err = TokenizerError;

    /// Parse a canonical name, or the aliases `o200k` and `cl100k`.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let name = match name {
            "o200k" => "o200k_base",
            "cl100k" => "cl100k_base",
            _ => name,
        };
        Self::ALL
            .into_iter()
            .find(|encoding| encoding.name() == name)
            .ok_or_else(|| TokenizerError::UnknownEncoding(name.to_owned()))
    }
}
