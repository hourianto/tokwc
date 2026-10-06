# Development

Requires Rust 1.88 or newer. From a checkout, install with:

```sh
cargo install --locked --path .
```

Before submitting a change, run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked --no-default-features
```

Both crates share additional Clippy checks in `Cargo.toml`. The test suite
includes the full Unicode regression and recorded API fixtures; no separate
release-only test run is needed. Development builds use light optimization
while retaining debug symbols, assertions, and overflow checks.

Rust 1.88 is the minimum: the code uses let-chains and `as_chunks`, and the
current `ignore` dependency requires it. CI checks this version and stable.

The default `cli` feature builds the command-line program. Library-only builds
must work without it. Public API examples in `docs/library.md` and Rustdoc run as
doctests. CI also checks documentation and library-only Clippy warnings.

See [the table generator](tools/gen-tables/README.md) for source attribution,
regeneration commands, validation, and binary formats. Its inputs are bundled
in the repository. Verify the checked-in tables with:

```sh
cargo run --locked --release -p gen-tables -- --check
```
