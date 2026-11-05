# anvil-formats

A Rust library that reads the data files of Ubisoft's Anvil engine. It reads `.forge` archives and the resources stored in them for two games: Riders Republic and Shaun White Skateboarding.

## Features

- Reads `.forge` archives of version 26 (Shaun White Skateboarding, LZO2A blocks) and version 27 (Riders Republic, zstd blocks)
- Lists a forge from its index and name tables without reading the payloads
- Decompresses an entry's data blocks and verifies their Adler-32 checksums
- Reads Riders Republic meshes, vertex layouts, materials, texture sets, texture maps and mips
- Reads Riders Republic material templates, shader permutations and their DXBC containers
- Reads Riders Republic entities, collision shapes, skeletons, rigs, animation and character build tables
- Reads Riders Republic terrain layers, floor heights, world grids, grass, water, decals, impostors, lights and sky
- Reads Shaun White Skateboarding scene entities, collision, spawn markers, grind guidance, lights and materials
- Reads Shaun White Skateboarding TextureMaps and slices their mip levels by face and level
- Reads Shaun White Skateboarding meshes with their bones, vertex layouts, draw ranges and bone palettes
- Reads the SDB2 shader databases that ship beside the Shaun White Skateboarding forges
- Keeps every byte it does not interpret as stored, so a record can be compared with its file
- Refuses short or malformed data with an error that names the field and its offset
- Returns texture blocks and vertex data as stored; it does not decode, convert or write

## Getting Started

### Requirements

Rust 1.99 or newer. To read anything real you need your own copy of the game; the crate ships no game data.

### Installation

```sh
cargo add anvil-formats
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

The formats are documented in the [Engine wiki](https://kalmarv.github.io/engine-wiki/engines/anvil/), and the API on [docs.rs](https://docs.rs/anvil-formats). [CONTRIBUTING.md](CONTRIBUTING.md) covers building, running the tests against your own game files, and reporting a file the crate cannot read.
