# Rust library

Disable the default CLI feature when adding a published release as a dependency:

```toml
[dependencies]
tokwc = { version = "0.1", default-features = false }
```

Both providers use the same API:

```rust
use tokwc::{Encoding, Tokenizer};

fn main() -> Result<(), tokwc::TokenizerError> {
    let openai = Tokenizer::for_model("gpt-6.1-sol")?;
    assert_eq!(openai.encoding(), Encoding::O200kBase);
    assert_eq!(openai.count("hello world\n"), 3);

    let claude = Tokenizer::for_model("claude-opus-5-5")?;
    assert_eq!(claude.count("hello, world"), 12);
    Ok(())
}
```

Use `Tokenizer::new(Encoding::Cl100kBase)` or
`Tokenizer::for_encoding("cl100k_base")` to select an encoding directly.
`Encoding::ALL` lists supported encodings and Claude profiles. Unknown model
names and unsupported encodings return a typed `TokenizerError`.

## Text metrics

`count_all` includes lines, words, UTF-8 bytes, Unicode scalar values, and tokens.
`count_metrics` computes only selected fields and leaves the others at zero:

```rust
use tokwc::{Metric, Tokenizer};

let tokenizer = Tokenizer::default(); // o200k_base
let all = tokenizer.count_all("hello world\n");
assert_eq!((all.lines, all.words, all.chars, all.tokens), (1, 2, 12, 3));

let selected = tokenizer.count_metrics("é\n", &[Metric::Bytes, Metric::Chars]);
assert_eq!((selected.bytes, selected.chars, selected.tokens), (3, 2, 0));
```

Inputs are UTF-8 strings. File reading, invalid-byte handling, and output
formatting belong to the caller. Each count is independent; summing token counts
for separate inputs can differ from counting their concatenation.

## Counting rules

OpenAI counts text tokens without chat framing. Claude counts one plain-text
user message per input, including model-specific overhead. An empty Claude
message can therefore have a nonzero count. Full API requests, system prompts,
tools, and attachments are outside this API's scope. Claude support is unofficial
and counts may differ from the live API.

Special-token spellings are ordinary text by default. For OpenAI, opt in with
`tokenizer.with_special_tokens()?`; recognized special tokens then count as one.
Claude profiles reject that option with `TokenizerError::UnsupportedSpecialTokens`.

## Reuse and features

`Tokenizer` is cheap to copy and is `Send + Sync`. Vocabulary tables initialize
once on first use and are shared across instances and threads. Input text is not
retained after counting, and no network requests are made. Constructing a
tokenizer or requesting only non-token metrics does not load its vocabulary.

The default `cli` feature builds the command-line program. Disabling default
features omits argument parsing, file traversal, parallel file scheduling, and
CLI output dependencies. Both providers and all counting methods remain available.

Generate API documentation locally with `cargo doc --no-default-features --open`.
