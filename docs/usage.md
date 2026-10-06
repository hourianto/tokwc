# Usage reference

[Quick start](../README.md)

## Counts and output

With no count flags, `tokwc` prints tokens using `o200k_base`. With no paths,
it reads stdin; `-` selects stdin explicitly.

| Flag | Count |
| --- | --- |
| `-t, --tokens` | Tokens; default when no count flags are selected |
| `-l, --lines` | Newline characters |
| `-w, --words` | Words separated by Unicode whitespace |
| `-c, --bytes` | Bytes in the original input |
| `-m, --chars` | Unicode scalar values |
| `-a, --all` | All five counts |

Select multiple metrics by combining flags, such as `-lt`. Multiple inputs
produce a row per file and a final `total` row. Use `--no-total` to omit it.
Counts are per input: concatenating the files can produce a different token
count, even with OpenAI encodings.

`--json` includes the selected model, resolved encoding, and counts. Each entry
in `inputs` has an `is_total` boolean, so a file named `total` is unambiguous.
For example, `echo 'hello world' | tokwc --json --model gpt-6.1-sol` produces:

```json
{
  "provider": "openai",
  "encoding": "o200k_base",
  "model": "gpt-6.1-sol",
  "special_tokens": false,
  "lossy": false,
  "metrics": ["tokens"],
  "inputs": [{"name": "stdin", "is_total": false, "counts": {"tokens": 3}}]
}
```

## Models and encodings

**Use `--model` (or `-M`) when you know the model name.** It selects the appropriate
counting rules. Use `--encoding` (or `-e`) to select those rules directly.
The two options are mutually exclusive. Run `tokwc --list-encodings` to list
all direct choices.

### OpenAI

| Direct encoding | Example model names |
| --- | --- |
| `o200k_base` (default) | `gpt-6.1-sol`, `gpt-6-astra`, `gpt-6-sol`, `gpt-6-luna`, `gpt-5.6-sol`, `gpt-4o` |
| `o200k_harmony` | `gpt-oss-20b`, `gpt-oss-120b` |
| `cl100k_base` | `gpt-4`, `gpt-3.5-turbo`, `text-embedding-3-small` |

OpenAI counts cover the input text only. They exclude chat-message framing,
system prompts, tool definitions, and other API request overhead. `--special`
counts recognized special-token spellings, such as `<|endoftext|>`, as single
tokens; without it, they are ordinary text. Older encodings such as `p50k_base`
are not included.

### Claude

| Supported model names | Direct counting profile |
| --- | --- |
| `claude-opus-4-7` | `claude_v4_7` |
| `claude-opus-4-8`, `claude-sonnet-5`, `claude-fable-5` | `claude_v4_8` |
| `claude-opus-5` | `claude_v5` |
| `claude-fable-5-1` | `claude_fable_v5_1` |
| `claude-opus-5-5`, `claude-sonnet-5-5` | `claude_v5_5` |

An eight-digit snapshot suffix is accepted for these names, for example
`claude-opus-5-5-20260921`. Unknown families and versions are rejected.
Although selected through `--encoding`, the Claude choices are counting
profiles: they share a vocabulary and differ in message overhead
and whitespace handling. `--special` applies only to OpenAI.

Each Claude input represents **one plain-text user message**, including its
message overhead. Totals sum those independent messages. This does not count
an entire conversation, system prompts, tools, images, or documents attached to
an API request. Use Anthropic's `count_tokens` endpoint for those requests.

Claude support is unofficial. In a comparison on 2026-10-06,
`claude-opus-5-5` matched Anthropic's API for all 894 tested files: 874 source
files and 20 multilingual prose files, totaling 5,130,349 tokens. Each original
UTF-8 file was sent as one user message, without newline conversion, a system
prompt, tools, or beta headers (API version `2023-06-01`). That corpus is not
bundled here, and the result does not establish accuracy for arbitrary input or
other models. The repository includes 568 synthetic API regression cases;
counts may still differ from the live API.

## File handling

Explicit paths can name regular files or readable streams, including Unix
FIFOs, `/dev/stdin`, and shell process substitutions. Directory walks recurse
over regular files, respecting `.gitignore` and `.ignore` and skipping hidden
files. Use `--max-depth 1` to skip subdirectories.

Files and stdin are read into memory. Regular files are counted with at most
four workers by default. Use `--threads N` (or `-j N`) to
change the limit, for example `tokwc -j 8 src/`; `-j 1` runs sequentially.
The worker count never exceeds the number of input files. Single-file input and
commands containing streams run sequentially.

When text counts are selected, directory walks skip invalid UTF-8 files with a
warning. Explicitly named inputs fail on invalid UTF-8. Use `--lossy` to replace
invalid bytes, or select only bytes and lines (`-cl`) to count arbitrary bytes.
There is no separate binary-file detector. Read and traversal errors fail the
command; a directory with no countable files also fails.

| Option | Behavior |
| --- | --- |
| `-j, --threads <N>` | Maximum parallel file workers; positive integer, default 4 |
| `--lossy` | Replace invalid UTF-8 bytes for text counts |
| `--no-ignore` | Include hidden and ignored files in directory walks |
| `--follow-links` | Follow symbolic links when walking directories |
| `--max-depth <DEPTH>` | Limit directory recursion |
