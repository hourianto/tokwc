use crate::claude::ClaudeTokenizer;
use crate::{Encoding, Provider, TokenizerError};

/// A reusable token counter for one encoding or Claude message profile.
///
/// Constructors validate configuration without loading vocabulary tables. Tables
/// are loaded on the first count and shared across instances and threads.
/// The default is [`Encoding::O200kBase`] with special-token recognition disabled.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tokenizer {
    encoding: Encoding,
    special_tokens: bool,
}

impl Tokenizer {
    /// Select an encoding or profile directly, treating special-token spellings as text.
    #[must_use]
    pub const fn new(encoding: Encoding) -> Self {
        Self {
            encoding,
            special_tokens: false,
        }
    }

    /// Select counting rules by model name.
    ///
    /// # Errors
    /// Returns [`TokenizerError`] for an unknown model or unsupported encoding.
    pub fn for_model(model: &str) -> Result<Self, TokenizerError> {
        Encoding::for_model(model).map(Self::new)
    }

    /// Select an encoding by its canonical name, or the aliases `o200k` and `cl100k`.
    ///
    /// ```
    /// use tokwc::Tokenizer;
    /// let tokenizer = Tokenizer::for_encoding("cl100k_base")?;
    /// assert_eq!(tokenizer.count("hello world"), 2);
    /// # Ok::<(), tokwc::TokenizerError>(())
    /// ```
    ///
    /// # Errors
    /// Returns [`TokenizerError::UnknownEncoding`] for an unsupported name.
    pub fn for_encoding(name: &str) -> Result<Self, TokenizerError> {
        name.parse().map(Self::new)
    }

    /// Enable recognition of every special token defined by the selected OpenAI encoding.
    ///
    /// Unrecognized spellings remain ordinary text. Calling this again has no effect.
    ///
    /// ```
    /// use tokwc::Tokenizer;
    /// let tokenizer = Tokenizer::for_encoding("cl100k_base")?.with_special_tokens()?;
    /// assert_eq!(tokenizer.count("<|endoftext|>"), 1);
    /// # Ok::<(), tokwc::TokenizerError>(())
    /// ```
    ///
    /// # Errors
    /// Returns [`TokenizerError::UnsupportedSpecialTokens`] for a Claude profile.
    pub fn with_special_tokens(mut self) -> Result<Self, TokenizerError> {
        if self.provider() != Provider::OpenAi {
            return Err(TokenizerError::UnsupportedSpecialTokens);
        }
        self.special_tokens = true;
        Ok(self)
    }

    /// The selected encoding or Claude counting profile.
    #[must_use]
    pub const fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// The provider whose counting rules are used.
    #[must_use]
    pub const fn provider(&self) -> Provider {
        self.encoding.provider()
    }

    /// Whether recognized OpenAI special-token spellings count as single tokens.
    #[must_use]
    pub const fn special_tokens(&self) -> bool {
        self.special_tokens
    }

    /// Count one UTF-8 text input.
    ///
    /// OpenAI counts text tokens without chat framing. Claude counts a single
    /// plain-text user message including its frame, so empty input can have a
    /// nonzero count. Each call is independent; no conversation state is retained.
    #[must_use]
    pub fn count(&self, text: &str) -> usize {
        if let Encoding::Claude(version) = self.encoding {
            ClaudeTokenizer::new(version).token_count(text)
        } else {
            let bpe = tiktoken::get_encoding(self.encoding.name()).expect("enabled encoding");
            if self.special_tokens {
                bpe.count_with_special_tokens(text)
            } else {
                bpe.count(text)
            }
        }
    }
}
