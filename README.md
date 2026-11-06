# anvil-formats

[![crates.io: anvil-formats](https://img.shields.io/crates/v/anvil-formats?label=anvil-formats)](https://crates.io/crates/anvil-formats) [![docs.rs](https://img.shields.io/docsrs/anvil-formats)](https://docs.rs/anvil-formats) [![CI](https://img.shields.io/github/actions/workflow/status/Kalmarv/anvil-formats/ci.yml?branch=main&label=CI)](https://github.com/Kalmarv/anvil-formats/actions/workflows/ci.yml)

A Rust library that reads the data files of Ubisoft's Anvil engine. It reads `.forge` archives and the resources stored in them for two games, Riders Republic and Shaun White Skateboarding.

## Features

- Reads `.forge` archives of version 26 (Shaun White Skateboarding) and version 27 (Riders Republic)
- Lists the entries of one forge, or of several forges searched as one archive, from the index and name tables alone
- Decompresses an entry's LZO2A or zstd data blocks and verifies their Adler-32 checksums
- Reads Riders Republic meshes and decodes their vertex attributes as the engine's vertex declarations state them
- Reads Riders Republic materials, texture sets, texture maps and mip levels
- Reads Riders Republic material templates, their shader permutations and the DXBC containers of `ShaderDataDX11.bin`
- Reads Riders Republic entities, entity groups, LOD selectors and collision shapes
- Reads Riders Republic skeletons, rigs, animation clips and the rider customization tables
- Reads Riders Republic terrain node layers and decodes their height and material tiles
- Reads Riders Republic floor heights, world grids, grass, water, decals, impostors, lights and sky settings
- Reads Shaun White Skateboarding scene entities, collision, spawn markers, grind guidance, lights and materials
- Reads Shaun White Skateboarding texture maps, meshes, skeletons, animation clips and animation sets, and the SDB2 shader databases beside its forges
- Keeps every byte it does not interpret as stored, so a record can be compared with its file
- Returns texture pixels as stored bytes without decoding them
- Refuses short or malformed data with a typed error that names the field and its offset instead of panicking

## Getting Started

### Requirements

Rust 1.99 or newer. To read anything real you need your own copy of the game; the crate ships no game data.

### Installation

```sh
cargo add anvil-formats
```

The release on crates.io is 0.1.2. The Shaun White Skateboarding texture map, mesh, skeleton and animation readers (`shaunwhite::texture`, `shaunwhite::mesh`, `shaunwhite::skeleton`, `shaunwhite::animation`) are not released yet; to use them, depend on the repository instead:

```sh
cargo add anvil-formats --git https://github.com/Kalmarv/anvil-formats
```

### Usage

Count a forge's entries by kind, then read its first Mesh ([`examples/forge_listing.rs`](examples/forge_listing.rs)):

```rust
//! Counts a forge's entries by kind, then reads its first Mesh.
//!
//! Run: `cargo run --example forge_listing -- path/to/DataPC.forge`
use std::collections::BTreeMap;

use anvil_formats::archive::{Archive, At};
use anvil_formats::listing::Kind;
use anvil_formats::mesh::StoredMesh;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: forge_listing <file.forge>")?;
    // Reads only the forge's index and name tables; payloads stay on disk.
    let mut archive = Archive::open(&[path])?;

    let mut by_kind: BTreeMap<Option<Kind>, usize> = BTreeMap::new();
    let mut first_mesh = None;
    for (forge, entry) in archive.listing() {
        *by_kind.entry(entry.kind).or_default() += 1;
        if entry.kind == Some(Kind::Mesh) && first_mesh.is_none() {
            first_mesh = Some(At {
                forge,
                entry: entry.entry,
            });
        }
    }
    for (kind, count) in &by_kind {
        match kind {
            Some(kind) => println!("{count:>8}  {}", kind.name()),
            None => println!("{count:>8}  (kinds this crate has no reader for)"),
        }
    }

    if let Some(at) = first_mesh {
        // Decompresses and checks the entry, then reads the Mesh body as stored.
        let resource = archive.resource(at)?;
        let mesh = StoredMesh::parse(&resource.body)?;
        println!("{}: {} vertices", resource.name, mesh.positions()?.len());
    }
    Ok(())
}
```

```sh
cargo run --example forge_listing -- path/to/DataPC.forge
```

Shaun White Skateboarding's name records carry no type id, so its entries list with `kind` set to `None`. Read them with `forge::Forge::payload`, `data::read_entry` and `objects::read_objects`, then pass each object's body to a reader in [`shaunwhite`](src/shaunwhite/mod.rs).

Run the tests with `cargo test --workspace`. The tests that read game files skip unless they are configured; [CONTRIBUTING.md](CONTRIBUTING.md) describes the setup and how to report a file the crate cannot read. The formats are documented in the [Engine wiki](https://kalmarv.github.io/engine-wiki/engines/anvil/) and the API on [docs.rs](https://docs.rs/anvil-formats). [primase](https://github.com/Kalmarv/primase) builds its Riders Republic and Shaun White Skateboarding readers on this crate. Licensed under GPL-3.0-or-later.
