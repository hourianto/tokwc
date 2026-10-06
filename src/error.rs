use std::error::Error;
use std::fmt;

use crate::claude::ClaudeVersionError;

/// Failure to select a tokenizer or configure its counting rules.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenizerError {
    /// No encoding is known for this model name.
    UnknownModel(String),
    /// The requested encoding name is not supported.
    UnknownEncoding(String),
    /// The Claude model or snapshot suffix is unsupported.
    UnsupportedClaudeModel(ClaudeVersionError),
    /// A known OpenAI model uses an encoding that is not bundled.
    UnsupportedModelEncoding {
        /// Requested model name.
        model: String,
        /// Encoding required by that model.
        encoding: String,
    },
    /// Special-token recognition was requested for a Claude profile.
    UnsupportedSpecialTokens,
}

impl fmt::Display for TokenizerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownModel(model) => write!(formatter, "unknown model {model:?}"),
            Self::UnknownEncoding(name) => write!(formatter, "unknown encoding {name:?}"),
            Self::UnsupportedClaudeModel(error) => error.fmt(formatter),
            Self::UnsupportedModelEncoding { model, encoding } => write!(
                formatter,
                "model {model:?} uses unsupported encoding {encoding:?}"
            ),
            Self::UnsupportedSpecialTokens => {
                formatter.write_str("special tokens are only supported for OpenAI encodings")
            }
        }
    }
}

impl Error for TokenizerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnsupportedClaudeModel(error) => error.source(),
            _ => None,
        }
    }
}
