use std::ops::AddAssign;

use crate::Tokenizer;

/// A text metric to compute with [`Tokenizer::count_metrics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    /// Newline (`\n`) count, as in `wc -l`.
    Lines,
    /// Words separated by Unicode whitespace.
    Words,
    /// UTF-8 byte count.
    Bytes,
    /// Unicode scalar count, not grapheme clusters or display columns.
    Chars,
    /// Token count under the selected tokenizer's rules.
    Tokens,
}

impl Metric {
    /// All five metrics, in CLI output order.
    pub const ALL: [Self; 5] = [
        Self::Lines,
        Self::Words,
        Self::Bytes,
        Self::Chars,
        Self::Tokens,
    ];

    /// Lowercase metric name, also used in CLI JSON output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lines => "lines",
            Self::Words => "words",
            Self::Bytes => "bytes",
            Self::Chars => "chars",
            Self::Tokens => "tokens",
        }
    }
}

/// Counts for one input. Unrequested metrics are zero.
///
/// Adding counts sums independent inputs. Token counts may differ from counting
/// their concatenation; Claude includes a separate message frame for each input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Number of `\n` characters.
    pub lines: usize,
    /// Number of words separated by Unicode whitespace.
    pub words: usize,
    /// Number of UTF-8 bytes.
    pub bytes: usize,
    /// Number of Unicode scalar values.
    pub chars: usize,
    /// Token count, including message overhead for Claude profiles.
    pub tokens: usize,
}

impl Counts {
    /// Read the count for a particular metric.
    #[must_use]
    pub const fn get(self, metric: Metric) -> usize {
        match metric {
            Metric::Lines => self.lines,
            Metric::Words => self.words,
            Metric::Bytes => self.bytes,
            Metric::Chars => self.chars,
            Metric::Tokens => self.tokens,
        }
    }
}

impl AddAssign for Counts {
    fn add_assign(&mut self, rhs: Self) {
        self.lines += rhs.lines;
        self.words += rhs.words;
        self.bytes += rhs.bytes;
        self.chars += rhs.chars;
        self.tokens += rhs.tokens;
    }
}

impl Tokenizer {
    /// Count lines, words, bytes, characters, and tokens for one input.
    ///
    /// ```
    /// use tokwc::Tokenizer;
    /// let counts = Tokenizer::default().count_all("hello world\n");
    /// assert_eq!((counts.lines, counts.words, counts.bytes), (1, 2, 12));
    /// assert_eq!((counts.chars, counts.tokens), (12, 3));
    /// ```
    #[must_use]
    pub fn count_all(&self, text: &str) -> Counts {
        self.count_metrics(text, &Metric::ALL)
    }

    /// Compute only the requested metrics. Unrequested fields are zero.
    ///
    /// Repeating a metric has no effect. An empty selection returns all zeros.
    /// Vocabulary tables are not loaded unless [`Metric::Tokens`] is requested.
    ///
    /// ```
    /// use tokwc::{Metric, Tokenizer};
    /// let counts = Tokenizer::default().count_metrics("é\n", &[Metric::Bytes, Metric::Chars]);
    /// assert_eq!((counts.bytes, counts.chars, counts.tokens), (3, 2, 0));
    /// ```
    #[must_use]
    pub fn count_metrics(&self, text: &str, metrics: &[Metric]) -> Counts {
        let mut counts = Counts::default();
        if metrics.contains(&Metric::Lines) {
            counts.lines = memchr::memchr_iter(b'\n', text.as_bytes()).count();
        }
        if metrics.contains(&Metric::Words) {
            counts.words = text.split_whitespace().count();
        }
        if metrics.contains(&Metric::Bytes) {
            counts.bytes = text.len();
        }
        if metrics.contains(&Metric::Chars) {
            counts.chars = text.chars().count();
        }
        if metrics.contains(&Metric::Tokens) {
            counts.tokens = self.count(text);
        }
        counts
    }
}
