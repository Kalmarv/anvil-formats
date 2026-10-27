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
