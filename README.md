# tokwc

Count tokens for OpenAI and Claude models in files, directories, or stdin.
Also counts lines, words, bytes, and characters, like `wc`.

> [!NOTE]
> AI usage disclosure: tokwc is developed by agentic LLMs.

## Install

Requires Rust 1.88 or newer.

```sh
cargo install --locked tokwc
```

## Use

Token counts use `o200k_base` by default. Choose a model with `-M` (`--model`):

```sh
tokwc prompt.txt
tokwc -M gpt-6.1-sol prompt.txt
tokwc -M claude-opus-5-5 prompt.txt
tokwc --all src/
cat prompt.txt | tokwc --json
```

Directories are scanned recursively, respecting ignore files. Multiple files
get individual counts and a total.

Add `--all` to include lines, words, bytes, and characters:

```sh
echo 'hello world' | tokwc --all
```

```text
input  lines  words  bytes  chars  tokens
-----  -----  -----  -----  -----  ------
stdin      1      2     12     12       3
```

Claude counts include one user message's overhead.
[Scope and accuracy](docs/usage.md#models-and-encodings).

Run `tokwc --help` or see the [usage reference](docs/usage.md) for model choices
and options. For Rust integration, see [Library usage](docs/library.md).
See [Development](CONTRIBUTING.md) to contribute.

## License and credits

[MIT](LICENSE).

OpenAI token counting uses [rust-tiktoken](https://github.com/goliajp/rust-tiktoken),
a fast Rust implementation compatible with OpenAI's tiktoken.

The Claude implementation and vocabulary are adapted from
[ctok](https://github.com/sanderland/ctok), Copyright (c) 2026 Sander Land,
also under [MIT](src/claude/data/LICENSE.claudetok).
