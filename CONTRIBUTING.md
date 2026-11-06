# Contributing

## Build and test

Rust 1.99 or newer (`rust-toolchain.toml` pins 1.99.0).

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

Tests that read a game's own files skip when the file is not configured, printing one line that names the missing
key, so these pass without any game installed. To run them, point `GAME_DATA` at a TOML file naming your installs:

```toml
[installs]
riders     = 'D:/games/RidersRepublic'   # the folder holding the .forge files
shaunwhite = 'D:/games/ShaunWhite'       # the folder holding the version-26 .forge files
[oracles]
root = 'D:/oracles'                      # optional: tables of what a game's own renderer loaded
[scratch]
root = 'D:/scratch'                      # optional: where tests write output
[run]
require = 'gate'                         # optional: fail on a missing required input instead of skipping
```

```sh
GAME_DATA=path/to/game-data.toml cargo test --workspace
```

The file format is documented in [primase-testdata](https://github.com/Kalmarv/primase-testdata), a git dependency of
the tests. The inputs every test may ask for are listed in [tests/inputs/README.md](tests/inputs/README.md). The real
forges are large (Riders Republic's `DataPC_MAD_USNAT.forge` is 15.6 GB; its tests read about 163 MB of it). A test
that reads a new real file declares it in `tests/inputs/mod.rs` first; `tests/real_data_inputs.rs` checks the
declarations and the generated table. `tests/readme.rs` checks that the README's example is `examples/forge_listing.rs`.

## Conventions

- Readers take bytes and return records. Only `forge`, `archive` and `shader_source` open files.
- Every read is bounds-checked and returns an error naming the field and offset; no `unwrap`, `expect` or `panic!` in
  library code.
- Keep bytes you do not understand as stored, and say in the docs what a layout was established on (build, number of
  instances, tool it agrees with) and what is only observed.
- A new reader gets a test built from hand-made bytes, so it runs without game files; add a real-file test too when you
  have the data.

## Releasing

A release is a commit that changes a version. Once every CI job has passed on main, CI's release job
(`.github/release.py`) publishes each crate whose version crates.io does not have yet, in dependency order (trusted
publishing: no token is stored anywhere), tags the commit `v<version>` (`<package>-v<version>` for a package with a
version of its own) and creates the GitHub Release with that version's CHANGELOG section as its notes.
A commit that changes no version only runs CI.

1. Bump the version when a change is worth releasing: a patch (0.x.Y) for fixes and data or manifest additions, a minor
   (0.X.0) for new API or any breaking change before 1.0. Run `cargo check` so `Cargo.lock` follows.
2. In the same commit, move the change's CHANGELOG lines under a `## <version> (<date>)` heading.
3. Push to main. Never publish by hand; a new crate's first release is the one exception (the token holder makes it,
   then the crate gets its trusted-publishing config).

## Reporting a file this library cannot read

Open an issue with the "Unreadable file" template. Please include:

- the game, build or version, and the file's name (for example `DataPC_MAD_USNAT.forge`);
- the entry's name, type id or kind, and its index in the forge, if you know them;
- the error message, complete (it names the field and the offset);
- the output of `cargo run --example forge_listing -- <forge>` if the forge itself does not open.

Please do **not** attach game files, or extracts of them that are large enough to rebuild the game's content. They are
copyrighted, and we cannot accept them. If a few bytes are needed, a hex dump of the failing record (the offset from the
error message, and up to a few hundred bytes around it) is enough; we may ask for one.
