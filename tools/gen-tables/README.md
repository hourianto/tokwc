# Regenerating Claude tables

From the repository root:

```sh
cargo run --locked --release -p gen-tables
```

This regenerates all five files in `src/claude/data`. To verify the checked-in
files without changing them:

```sh
cargo run --locked --release -p gen-tables -- --check
```

An optional output directory generates the tables elsewhere, including into an
empty directory. The generator does not read any existing table files.

```sh
cargo run --locked --release -p gen-tables -- target/generated-tables
```

The inputs are bundled here and shared with the runtime. No ctok checkout,
Python installation, API access, or vocabulary download is needed. Cargo builds
the bundled Zstandard compressor, which requires a C compiler for regeneration;
no installed `zstd` command or library is needed. The CLI uses a pure Rust decoder.
Add `--offline` if Cargo's dependencies are already cached.

## Source and attribution

`data/pieces_v4_7.json` contains the vocabulary keys from ctok's
[`ctok/data/pieces_v4_7.json` at revision ad78ea15a1febf983b379475b20f5a2b0d2ebe76](https://github.com/sanderland/ctok/blob/ad78ea15a1febf983b379475b20f5a2b0d2ebe76/ctok/data/pieces_v4_7.json).
Witness metadata was omitted; section names and all piece keys are retained.
ctok is Copyright (c) 2026 Sander Land, licensed under MIT. Its complete license
is in [data/LICENSE.ctok](data/LICENSE.ctok), and remains beside the runtime data
in [LICENSE.claudetok](../../src/claude/data/LICENSE.claudetok).

The generator expands the source's word/case markers into bytes, includes the
end-of-word variants of contractions, and builds a trie. Determinization tracks
the minimum segmentation cost; state minimization and block deduplication
produce the counting table. An independent dynamic-programming implementation
checks every vocabulary piece, surrounding boundaries, and deterministic random
byte sequences against the DFA. It also computes the short-word table.

Unicode properties come from the runtime's `properties.rs` and `constants.rs`.
The Unicode crates share pins in the workspace manifest. Rust's built-in character
properties also matter: CI checks generation on Rust 1.88 and stable. When
updating Unicode rules, toolchains, or vocabulary, regenerate and run the full
release tests. The generator checks the vocabulary assumptions behind the
digit, whitespace, word-boundary, and supplementary-scalar shortcuts.

## Binary formats

Each `.bin.zst` file is a single Zstandard frame compressed at level 19. The
generator verifies every frame with the runtime decoder. `Cargo.lock` pins the
compressor used for byte-for-byte regeneration; a compressor update may require
regenerating the files even if their decoded contents are unchanged.

The runtime decompresses each table once, on first use, and shares it across
threads and Claude profiles. It expands the DFA's deduplicated blocks for fast
lookups and discards the intermediate decoded buffer. The other tables remain
in memory in their decoded form.

The formats below describe the **decompressed** bytes. All integer fields are
little-endian. Bitmap bit `n` is bit `n % 8` of byte `n / 8`.

| File | Format |
| --- | --- |
| `count_dfa_v4_7.bin.zst` | `TOKDFA01`, u32 state count, u32 block count; 16 u16 block indices per state; then blocks of 16 u32 transitions. A transition's upper 24 bits are the destination state and its low byte is a signed i8 count delta. |
| `floor_v4_7.bin.zst` | Two 8,192-byte bitmaps: one-token BMP scalars, then two-byte UTF-8 prefixes indexed by their big-endian byte pair. Four supported three-byte prefixes are checked by the generator and handled directly by the runtime. |
| `multi_scalar.bin.zst` | An 8,192-byte bitmap of BMP scalars appearing in multi-scalar vocabulary pieces. |
| `character.bin.zst` | 65,536 u16 entries. Bits 0–2 encode the character class; bits 3–8 encode stray mark, punctuation boundary, digit boundary, decimal number, other number, and normalization fast-path eligibility. Surrogates have zero entries. |
| `ascii_words.bin.zst` | 16,512 u8 costs including case and word boundaries. One-letter words use index `a`; two-letter words use `128 + 128*a + b`. Non-letter entries are zero. |
