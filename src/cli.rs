//! File decoding and output formatting for the CLI.

use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Write as _};
use std::str::Utf8Error;

use anyhow::Result;

use tokwc::{Counts, Metric, Tokenizer};

const TEXT_DECODE_MESSAGE: &str = "not valid UTF-8; word, character, and token counts require text \
                                   (pass --lossy to replace invalid bytes)";

#[derive(Debug)]
struct TextDecodeError {
    label: String,
    source: Utf8Error,
}

impl fmt::Display for TextDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.label, TEXT_DECODE_MESSAGE)
    }
}

impl Error for TextDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Return the input label and explanation when an error was caused by invalid UTF-8.
#[must_use]
pub(super) fn text_decode_error(error: &anyhow::Error) -> Option<(&str, &'static str)> {
    error
        .downcast_ref::<TextDecodeError>()
        .map(|error| (error.label.as_str(), TEXT_DECODE_MESSAGE))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CountRow {
    pub name: String,
    pub counts: Counts,
    pub is_total: bool,
}

// Decode only for text metrics, retaining the original byte count in lossy mode.
pub(super) fn count_document(
    bytes: &[u8],
    metrics: &[Metric],
    lossy: bool,
    label: &str,
    tokenizer: &Tokenizer,
) -> Result<Counts> {
    let mut counts = if metrics
        .iter()
        .any(|metric| matches!(metric, Metric::Words | Metric::Chars | Metric::Tokens))
    {
        let text = decode_text(bytes, lossy, label)?;
        tokenizer.count_metrics(&text, metrics)
    } else {
        Counts {
            lines: if metrics.contains(&Metric::Lines) {
                memchr::memchr_iter(b'\n', bytes).count()
            } else {
                0
            },
            ..Counts::default()
        }
    };
    // Lossy UTF-8 replacement changes the decoded length, but -c counts the original bytes.
    if metrics.contains(&Metric::Bytes) {
        counts.bytes = bytes.len();
    }

    Ok(counts)
}

/// Format a labeled table containing the selected metrics for each row.
#[must_use]
pub(super) fn format_human(rows: &[CountRow], metrics: &[Metric]) -> String {
    let input_width = rows
        .iter()
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(0)
        .max("input".len());

    let metric_widths = metrics
        .iter()
        .map(|metric| {
            rows.iter()
                .map(|row| decimal_width(row.counts.get(*metric)))
                .max()
                .unwrap_or(1)
                .max(metric.name().len())
        })
        .collect::<Vec<_>>();

    let mut output = String::new();

    let _ = write!(output, "{:<input_width$}", "input");
    for (metric, width) in metrics.iter().zip(&metric_widths) {
        let _ = write!(output, "  {:>width$}", metric.name());
    }
    output.push('\n');

    write_separator(&mut output, input_width, &metric_widths);

    for (index, row) in rows.iter().enumerate() {
        if index > 0 && row.is_total {
            write_separator(&mut output, input_width, &metric_widths);
        }

        let name = &row.name;
        let _ = write!(output, "{name:<input_width$}");
        for (metric, width) in metrics.iter().zip(&metric_widths) {
            let _ = write!(output, "  {:>width$}", row.counts.get(*metric));
        }
        output.push('\n');
    }

    output
}

fn write_separator(output: &mut String, input_width: usize, metric_widths: &[usize]) {
    output.push_str(&"-".repeat(input_width));
    for width in metric_widths {
        output.push_str("  ");
        output.push_str(&"-".repeat(*width));
    }
    output.push('\n');
}

fn decode_text<'a>(bytes: &'a [u8], lossy: bool, label: &str) -> Result<Cow<'a, str>> {
    if lossy {
        Ok(String::from_utf8_lossy(bytes))
    } else {
        std::str::from_utf8(bytes)
            .map(Cow::Borrowed)
            .map_err(|source| {
                TextDecodeError {
                    label: label.to_string(),
                    source,
                }
                .into()
            })
    }
}

fn decimal_width(value: usize) -> usize {
    value.checked_ilog10().unwrap_or(0) as usize + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_labeled_table() {
        let output = format_human(
            &[
                CountRow {
                    name: "a.txt".to_string(),
                    counts: Counts {
                        lines: 1,
                        words: 2,
                        bytes: 3,
                        chars: 0,
                        tokens: 5,
                    },
                    is_total: false,
                },
                CountRow {
                    name: "total".to_string(),
                    counts: Counts {
                        lines: 1,
                        words: 2,
                        bytes: 3,
                        chars: 0,
                        tokens: 5,
                    },
                    is_total: true,
                },
            ],
            &[Metric::Lines, Metric::Words, Metric::Bytes, Metric::Tokens],
        );

        assert_eq!(
            output,
            "input  lines  words  bytes  tokens\n-----  -----  -----  -----  ------\na.txt      1      2      3       5\n-----  -----  -----  -----  ------\ntotal      1      2      3       5\n"
        );
    }

    #[test]
    fn total_name_is_not_magic() {
        let output = format_human(
            &[CountRow {
                name: "total".to_string(),
                counts: Counts {
                    tokens: 7,
                    ..Counts::default()
                },
                is_total: false,
            }],
            &[Metric::Tokens],
        );

        assert_eq!(output, "input  tokens\n-----  ------\ntotal       7\n");
    }
}
