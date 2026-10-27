# anvil-formats

A Rust library that reads the data files of Ubisoft's Anvil engine: `.forge` archives and the meshes, textures, materials, entities and terrain stored in them.

It was written against two games' own files: **Riders Republic** (Steam build 25394987; `.forge` version 27, zstd-compressed blocks) and **Shaun White Skateboarding** (PC, 2010; `.forge` version 26, LZO2A blocks). It reads the archive index and name tables, an entry's compressed data blocks (checksum verified), and the resources inside as plain records that keep every byte the reader does not interpret as it was stored:

- **Riders Republic:** Entities and EntityGroups, meshes and vertex layouts, materials, texture sets, texture maps and mips, material templates with their shader permutations and DXBC shader containers, LOD selectors, collision shapes, skeletons, rigs and animation, build tables (character customization), terrain node layers and their compressed code, floor heights, world grids, grass, water, decals, impostors, lights, sky, ambiance and forecast presets.
- **Shaun White Skateboarding:** the objects of a version-26 entry, scene entities, collision, spawn markers, grind guidance, lights, materials, and the SDB2 shader databases that ship beside the forges.

The formats are documented in the [Engine wiki](https://kalmarv.github.io/engine-wiki/engines/anvil/); each module's docs say which games and builds its layout was established on. Used by [primase](https://github.com/Kalmarv/primase), a game-asset conversion library.

## Example

Count a forge's entries by kind, then read its first Mesh. This is [`examples/forge_listing.rs`](examples/forge_listing.rs); run it with `cargo run --example forge_listing -- path/to/DataPC.forge`.

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

`Archive::open` reads only the index and name tables, so listing a 15 GB forge takes the time of its index. A Riders Republic forge's entries carry a type id the listing names as a `Kind` where this crate has a reader for it; a Shaun White Skateboarding forge's name records carry no type id, so a listing gives its entries no type id (`Listed::type_id` is `None`) and no kind; read its entries with `forge::Forge::payload`, `data::read_entry` and `objects::read_objects` (see [`shaunwhite`](src/shaunwhite/mod.rs)).

## What it guarantees

- **Checked reads.** Every field is read through a bounds-checked cursor. A short or malformed body is refused with an error that names the field and the offset (or the record, count and expected value), and the library code does not `unwrap`, `expect` or `panic!`. Slice indexing is denied by lint in most modules. This is a design rule and is tested against real and hand-built bytes; it has not been fuzzed.
- **Strict where the format allows it.** Counts, sizes and offsets are validated against the file or block they sit in; a forge whose tables disagree, a data block whose Adler-32 checksum does not match, or a record that does not end exactly where its length says is refused rather than guessed at.
- **Bytes kept as stored.** Fields whose meaning is not established are kept (`unknown_*` fields, `unnamed` byte runs, trailers), so a record can be compared with the file it came from.
- **No file access outside forge opening.** Only `forge`, `archive` and `shader_source` touch the file system; every other module takes `&[u8]`.
- **Documented evidence.** Docs state what a layout was established on (which build, how many instances, which third-party reader it agrees with) and say where something is an observation rather than a stored rule.

## What it does not do

- It does not write, repack or patch forges; it only reads.
- It does not convert: textures are not decoded to pixels (BCn blocks are returned as stored), meshes are not exported to glTF, shaders are not disassembled.
- It does not read other Anvil games or other forge versions. Versions 26 and 27 with a single FileSet are read; anything else is refused with an error naming what it found. Other Anvil titles may share parts of the layout, but nothing here was checked against them.
- It does not ship a class registry. Where an Entity component's length depends on the game's component classes, you pass a predicate saying which type ids are components ([`entity::StoredEntity::parse`](src/entity.rs)).
- It does not ship, download or extract game data. You need your own copy of the games to read anything real.
- Meaning is mostly not interpreted. Fields are named where the engine's own names are known; values such as light units or colour encodings are returned as stored.

## Building and testing

Rust 1.99 or newer (`rust-toolchain.toml` pins 1.99.0). The `primase-foundations` (byte cursor, hashing and the zstd, LZO2A and LZ4 decoders), `dxcontainer` (DXBC containers) and `primase-testdata` (dev-only) dependencies are git dependencies pinned to a revision in `Cargo.toml`.

```sh
cargo test --workspace
```

runs the unit tests and the tests that build their input by hand. The tests that read real game files **skip, printing one line naming the missing key**, unless you point `GAME_DATA` at a TOML file naming your installs:

```toml
[installs]
riders     = 'D:/games/RidersRepublic'   # the folder holding the .forge files
shaunwhite = 'D:/games/ShaunWhite'       # the folder holding the version-26 .forge files
[scratch]
root = 'D:/scratch'                      # optional: where tests write output
[run]
require = 'gate'                         # optional: fail on a missing required input instead of skipping
```

```sh
GAME_DATA=path/to/game-data.toml cargo test --workspace
```

Every key and file the tests read is listed in [`tests/inputs/README.md`](tests/inputs/README.md) (what it is, where it comes from, which tests use it); `tests/real_data_inputs.rs` fails if a test asks for an input that table does not declare. The file format is documented in [primase-testdata](https://github.com/Kalmarv/primase-testdata). The real forges are large (Riders Republic's `DataPC_MAD_USNAT.forge` is 15.6 GB; its tests read about 163 MB of it).

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to report a file this library cannot read.

## License

GPL-3.0-or-later; see [LICENSE](LICENSE).
