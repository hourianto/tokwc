use std::fs;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result, bail};
use clap::Parser;
use clap::builder::{PossibleValue, PossibleValuesParser, TypedValueParser};
use ignore::WalkBuilder;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use serde::Serialize;
use tokwc::claude::ClaudeVersion;
use tokwc::{Counts, Encoding, Metric, Tokenizer};

mod cli;

use cli::{CountRow, count_document, format_human, text_decode_error};

#[derive(Debug, Parser)]
#[command(
    name = "tokwc",
    version,
    about = "Count OpenAI or Claude tokens locally in files, directories, or stdin.",
    after_help = "Use --model for a model name, or --encoding for a direct encoding/profile.\nOpenAI counts text tokens; Claude counts one plain-text user message including its frame.\nSee --list-encodings for model/profile mappings.\n\nExamples:\n  tokwc prompt.txt\n  tokwc --model claude-opus-5-5 prompt.txt\n  tokwc -a src README.md\n  cat prompt.txt | tokwc --model gpt-6.1-sol -\n  tokwc --json --all --encoding cl100k_base notes.md"
)]
struct Cli {
    #[arg(
        short = 't',
        long,
        help = "Print token counts (default when no count flags are selected)"
    )]
    tokens: bool,

    #[arg(short = 'l', long, help = "Print newline counts")]
    lines: bool,

    #[arg(short = 'w', long, help = "Print word counts")]
    words: bool,

    #[arg(short = 'c', long = "bytes", help = "Print byte counts")]
    bytes: bool,

    #[arg(
        short = 'm',
        long = "chars",
        help = "Print Unicode scalar value counts"
    )]
    chars: bool,

    #[arg(
        short = 'a',
        long,
        help = "Print lines, words, bytes, chars, and tokens"
    )]
    all: bool,

    #[arg(
        short = 'e',
        long,
        value_parser = encoding_parser(),
        conflicts_with = "model",
        help = "OpenAI encoding or Claude counting profile (default: o200k_base)"
    )]
    encoding: Option<Encoding>,

    #[arg(
        short = 'M',
        long,
        help = "Select token counting rules by model name (recommended)"
    )]
    model: Option<String>,

    #[arg(long, help = "Treat recognized OpenAI special tokens as single tokens")]
    special: bool,

    #[arg(
        long,
        help = "Replace invalid UTF-8 bytes instead of rejecting text counts"
    )]
    lossy: bool,

    #[arg(long, help = "Emit structured JSON instead of wc-style columns")]
    json: bool,

    #[arg(long, help = "Suppress the total row when reading more than one input")]
    no_total: bool,

    #[arg(
        long,
        help = "Include hidden and ignored files when walking directories"
    )]
    no_ignore: bool,

    #[arg(long, help = "Follow symbolic links when walking directories")]
    follow_links: bool,

    #[arg(
        long,
        value_name = "DEPTH",
        help = "Limit recursion depth when walking directories"
    )]
    max_depth: Option<usize>,

    #[arg(
        short = 'j',
        long,
        value_name = "N",
        default_value = "4",
        help = "Maximum parallel file workers; 1 counts sequentially"
    )]
    threads: NonZeroUsize,

    #[arg(long, help = "List OpenAI encodings and Claude counting profiles")]
    list_encodings: bool,

    #[arg(
        value_name = "PATH",
        help = "Files, directories, or readable streams; use - for stdin"
    )]
    paths: Vec<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
        {
            return;
        }
        eprintln!("tokwc: {error:#}");
        process::exit(1);
    }
}

fn encoding_parser() -> impl TypedValueParser<Value = Encoding> {
    PossibleValuesParser::new(Encoding::ALL.map(|encoding| {
        let value = PossibleValue::new(encoding.name());
        match encoding {
            Encoding::O200kBase => value.alias("o200k"),
            Encoding::Cl100kBase => value.alias("cl100k"),
            _ => value,
        }
    }))
    .map(|name| name.parse().expect("validated encoding name"))
}

fn print_encodings() -> io::Result<()> {
    let width = Encoding::ALL
        .iter()
        .map(|encoding| encoding.name().len())
        .max()
        .unwrap_or(0);
    let mut output = io::stdout().lock();
    for encoding in Encoding::ALL {
        let description = match encoding {
            Encoding::O200kBase => {
                "default OpenAI text encoding; gpt-6.1-sol, gpt-6-astra, gpt-6-sol, gpt-6-luna, gpt-5, gpt-4o, o-series"
            }
            Encoding::O200kHarmony => "OpenAI text encoding; gpt-oss-20b, gpt-oss-120b",
            Encoding::Cl100kBase => {
                "OpenAI text encoding; gpt-4, gpt-3.5-turbo, text-embedding-3-small"
            }
            Encoding::Claude(ClaudeVersion::V4_7) => "Claude message profile; claude-opus-4-7",
            Encoding::Claude(ClaudeVersion::V4_8) => {
                "Claude message profile; claude-opus-4-8, claude-sonnet-5, claude-fable-5"
            }
            Encoding::Claude(ClaudeVersion::V5) => "Claude message profile; claude-opus-5",
            Encoding::Claude(ClaudeVersion::FableV5_1) => {
                "Claude message profile; claude-fable-5-1"
            }
            Encoding::Claude(ClaudeVersion::V5_5) => {
                "Claude message profile; claude-opus-5-5, claude-sonnet-5-5"
            }
        };
        writeln!(output, "{:<width$}  {description}", encoding.name())?;
    }
    Ok(())
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_encodings {
        print_encodings()?;
        return Ok(());
    }

    let metrics = selected_metrics(&cli);

    let mut tokenizer = match cli.model.as_deref() {
        Some(model) => Tokenizer::for_model(model)?,
        None => Tokenizer::new(cli.encoding.unwrap_or_default()),
    };
    if cli.special {
        tokenizer = tokenizer
            .with_special_tokens()
            .context("--special is only supported by the OpenAI tokenizer")?;
    }
    let inputs = inputs_from_cli(&cli)?;

    let mut rows = Vec::with_capacity(inputs.len() + 1);
    let mut total = Counts::default();
    let mut skipped = 0;
    let count = |input: &Input| {
        let label = input.label();
        let result = input
            .read()
            .and_then(|bytes| count_document(&bytes, &metrics, cli.lossy, &label, &tokenizer));
        (label, input.is_walked_file(), result)
    };
    let counted =
        if cli.threads.get() > 1 && inputs.len() > 1 && inputs.iter().all(Input::is_regular_file) {
            let pool = ThreadPoolBuilder::new()
                .num_threads(inputs.len().min(cli.threads.get()))
                .build()
                .context("failed to start file-counting workers")?;
            pool.install(|| inputs.par_iter().map(count).collect::<Vec<_>>())
        } else {
            inputs.iter().map(count).collect::<Vec<_>>()
        };

    for (label, walked, result) in counted {
        match result {
            Ok(counts) => {
                total += counts;
                rows.push(CountRow {
                    name: label,
                    counts,
                    is_total: false,
                });
            }
            Err(error) => {
                if walked && let Some((label, message)) = text_decode_error(&error) {
                    skipped += 1;
                    eprintln!("tokwc: warning: skipped {label}: {message}");
                    continue;
                }
                return Err(error);
            }
        }
    }

    if rows.is_empty() && skipped > 0 {
        bail!("no files counted ({skipped} skipped)");
    }

    if rows.len() > 1 && !cli.no_total {
        rows.push(CountRow {
            name: "total".to_string(),
            counts: total,
            is_total: true,
        });
    }

    if cli.json {
        print_json(&rows, &metrics, &tokenizer, &cli)?;
    } else {
        io::stdout()
            .lock()
            .write_all(format_human(&rows, &metrics).as_bytes())?;
    }

    Ok(())
}

fn selected_metrics(cli: &Cli) -> Vec<Metric> {
    if cli.all {
        return Metric::ALL.to_vec();
    }

    let mut metrics = Vec::new();
    if cli.lines {
        metrics.push(Metric::Lines);
    }
    if cli.words {
        metrics.push(Metric::Words);
    }
    if cli.bytes {
        metrics.push(Metric::Bytes);
    }
    if cli.chars {
        metrics.push(Metric::Chars);
    }
    if cli.tokens {
        metrics.push(Metric::Tokens);
    }

    if metrics.is_empty() {
        metrics.push(Metric::Tokens);
    }

    metrics
}

#[derive(Debug)]
enum Input {
    Stdin {
        explicit: bool,
    },
    File {
        path: PathBuf,
        origin: FileOrigin,
        regular: bool,
    },
}

#[derive(Debug, Clone, Copy)]
enum FileOrigin {
    Explicit,
    Walked,
}

impl Input {
    fn label(&self) -> String {
        match self {
            Self::Stdin { explicit } => if *explicit { "-" } else { "stdin" }.to_string(),
            Self::File { path, .. } => display_path(path),
        }
    }

    const fn is_walked_file(&self) -> bool {
        matches!(
            self,
            Self::File {
                origin: FileOrigin::Walked,
                ..
            }
        )
    }

    const fn is_regular_file(&self) -> bool {
        matches!(self, Self::File { regular: true, .. })
    }

    fn read(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        match self {
            Self::Stdin { .. } => {
                io::stdin()
                    .read_to_end(&mut bytes)
                    .context("failed to read stdin")?;
            }
            Self::File { path, .. } => {
                bytes =
                    fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
            }
        }
        Ok(bytes)
    }
}

fn inputs_from_cli(cli: &Cli) -> Result<Vec<Input>> {
    if cli.paths.is_empty() {
        return Ok(vec![Input::Stdin { explicit: false }]);
    }

    let mut inputs = Vec::new();
    for path in &cli.paths {
        if path.as_os_str() == "-" {
            inputs.push(Input::Stdin { explicit: true });
            continue;
        }

        let metadata =
            fs::metadata(path).with_context(|| format!("failed to inspect {}", path.display()))?;
        if metadata.is_dir() {
            inputs.extend(walk_directory(path, cli)?);
        } else {
            inputs.push(Input::File {
                path: path.clone(),
                origin: FileOrigin::Explicit,
                regular: metadata.is_file(),
            });
        }
    }

    Ok(inputs)
}

fn walk_directory(root: &Path, cli: &Cli) -> Result<Vec<Input>> {
    let mut builder = WalkBuilder::new(root);
    builder.sort_by_file_path(Ord::cmp);
    builder.follow_links(cli.follow_links);
    builder.max_depth(cli.max_depth);

    if cli.no_ignore {
        builder.standard_filters(false);
        builder.hidden(false);
    }

    let mut inputs = Vec::new();
    for entry in builder.build() {
        let entry =
            entry.with_context(|| format!("failed to walk directory {}", root.display()))?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            inputs.push(Input::File {
                path: entry.into_path(),
                origin: FileOrigin::Walked,
                regular: true,
            });
        }
    }

    if inputs.is_empty() {
        bail!("{} contains no readable files", root.display());
    }

    Ok(inputs)
}

fn display_path(path: &Path) -> String {
    let path = path.display().to_string();
    path.strip_prefix("./").unwrap_or(&path).to_string()
}

#[derive(Debug, Serialize)]
struct JsonOutput {
    provider: &'static str,
    encoding: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    special_tokens: bool,
    lossy: bool,
    metrics: Vec<&'static str>,
    inputs: Vec<JsonRow>,
}

#[derive(Debug, Serialize)]
struct JsonRow {
    name: String,
    is_total: bool,
    counts: JsonCounts,
}

#[derive(Debug, Default, Serialize)]
struct JsonCounts {
    #[serde(skip_serializing_if = "Option::is_none")]
    lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    words: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tokens: Option<usize>,
}

fn print_json(
    rows: &[CountRow],
    metrics: &[Metric],
    tokenizer: &Tokenizer,
    cli: &Cli,
) -> Result<()> {
    let output = JsonOutput {
        provider: tokenizer.provider().name(),
        encoding: tokenizer.encoding().name(),
        model: cli.model.clone(),
        special_tokens: tokenizer.special_tokens(),
        lossy: cli.lossy,
        metrics: metrics.iter().map(|metric| metric.name()).collect(),
        inputs: rows
            .iter()
            .map(|row| JsonRow {
                name: row.name.clone(),
                is_total: row.is_total,
                counts: json_counts(row.counts, metrics),
            })
            .collect(),
    };

    let mut bytes = serde_json::to_vec_pretty(&output)?;
    bytes.push(b'\n');
    io::stdout().lock().write_all(&bytes)?;
    Ok(())
}

fn json_counts(counts: Counts, metrics: &[Metric]) -> JsonCounts {
    let mut json = JsonCounts::default();
    for metric in metrics {
        match metric {
            Metric::Lines => json.lines = Some(counts.lines),
            Metric::Words => json.words = Some(counts.words),
            Metric::Bytes => json.bytes = Some(counts.bytes),
            Metric::Chars => json.chars = Some(counts.chars),
            Metric::Tokens => json.tokens = Some(counts.tokens),
        }
    }
    json
}
