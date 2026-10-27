# Contributing

## Build and test

Rust 1.99 or newer (`rust-toolchain.toml` pins 1.99.0).

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

Tests that read a game's own files skip when the file is not configured, so these pass without any game installed.
To run them, point `GAME_DATA` at a TOML file naming your installs (see the README, "Building and testing"); the inputs
every test may ask for are listed in [tests/inputs/README.md](tests/inputs/README.md). A test that reads a new real file
declares it in `tests/inputs/mod.rs` first; `tests/real_data_inputs.rs` checks the declarations and the generated table.

## Conventions

- Readers take bytes and return records. Only `forge`, `archive` and `shader_source` open files.
- Every read is bounds-checked and returns an error naming the field and offset; no `unwrap`, `expect` or `panic!` in
  library code.
- Keep bytes you do not understand as stored, and say in the docs what a layout was established on (build, number of
  instances, tool it agrees with) and what is only observed.
- A new reader gets a test built from hand-made bytes, so it runs without game files; add a real-file test too when you
  have the data.

## Reporting a file this library cannot read

Open an issue with the "Unreadable file" template. Please include:

- the game, build or version, and the file's name (for example `DataPC_MAD_USNAT.forge`);
- the entry's name, type id or kind, and its index in the forge, if you know them;
- the error message, complete (it names the field and the offset);
- the output of `cargo run --example forge_listing -- <forge>` if the forge itself does not open.

Please do **not** attach game files, or extracts of them that are large enough to rebuild the game's content. They are
copyrighted, and we cannot accept them. If a few bytes are needed, a hex dump of the failing record (the offset from the
error message, and up to a few hundred bytes around it) is enough; we may ask for one.
