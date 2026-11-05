//! Shaun White Skateboarding Meshes: every Mesh of every data forge of `installs.shaunwhite` read to its exact end.
mod inputs;

use std::collections::BTreeMap;
use std::fs::File;

use anvil_formats::data::read_entry;
use anvil_formats::forge::Forge;
use anvil_formats::objects::read_objects;
use anvil_formats::shaunwhite::mesh::{Error, MESH, StoredMesh};
use inputs::{Game, install};

fn words(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// A mesh with one bone, two vertices of 12 bytes, three indices and one range (with its palette), as the game lays it.
fn mesh_body(submeshes: u32) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(words(&[5, MESH, 2]));
    b.push(0);
    b.extend(words(&[submeshes, 1, 9, 0x9EF0_E7A1, 0xABCD]));
    b.extend((0..16).flat_map(|i| (i as f32).to_le_bytes()));
    b.extend([0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut data = words(&[16, 0, 12, 24, 12, 24, 6, 0, 64, 1, 0, 24 + 6 + 24]);
    data.extend(0u8..24);
    data.extend([0, 0, 1, 0, 1, 0]);
    data.extend(words(&[4, 0, 2, 0, 1, 77]));
    let mut tail = vec![3, 1, 1, 0, 0, 2, 0, 0];
    tail.resize(40, 0xEE);
    data.extend(tail);
    b.extend(words(&[0xFC9E_1595, data.len() as u32]));
    b.extend(data);
    b.extend(words(&[16, 32, 1, 0x1234]));
    b
}

#[test]
fn hand_built_mesh_reads_and_refuses_what_does_not_fit() {
    let b = mesh_body(0);
    let m = StoredMesh::parse(&b).unwrap();
    assert_eq!(
        (
            m.id,
            m.category,
            m.bones.len(),
            m.bones[0].name,
            m.materials.as_slice()
        ),
        (5, 2, 1, 0xABCD, &[0x1234][..])
    );
    let c = &m.compiled;
    assert_eq!(
        (c.stride, c.vertex_bytes, c.index_bytes, c.flags, c.streams.len()),
        (12, 24, 6, 64, 1)
    );
    assert_eq!(&b[c.streams[0].clone()], &(0u8..24).collect::<Vec<_>>()[..]);
    assert_eq!(&b[c.indices.clone()], &[0, 0, 1, 0, 1, 0]);
    assert_eq!(
        (
            c.ranges[0].vertex_count,
            c.ranges[0].triangles,
            c.ranges[0].unknown
        ),
        (2, 1, 77)
    );
    let p = c.palettes(&b).unwrap();
    assert_eq!(
        (p[0].unknown, p[0].vertex_count, p[0].bones.as_slice()),
        (0, 2, &[0][..])
    );

    assert!(matches!(
        StoredMesh::parse(&mesh_body(1)),
        Err(Error::SubMeshes(1))
    ));
    assert!(matches!(
        StoredMesh::parse(&[b.clone(), vec![0]].concat()),
        Err(Error::Trailing(1))
    ));
    assert!(matches!(
        StoredMesh::parse(&b[..b.len() - 1]),
        Err(Error::Short(_))
    ));
    let mut bad = b.clone();
    let total = b.len() - 16 - 40 - 24 - 6 - 24 - 4;
    bad[total] = 0; // the buffer total no longer adds up
    assert!(matches!(
        StoredMesh::parse(&bad),
        Err(Error::Value {
            what: "buffer total",
            ..
        })
    ));
}

#[test]
fn every_real_mesh_reads() {
    let Some(folder) = install(Game::ShaunWhite) else {
        return;
    };
    let mut forges: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "forge"))
        .filter(|p| {
            !p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("DataPC_StreamedSounds")
        })
        .collect();
    forges.sort();
    let (mut meshes, mut raw) = (0usize, 0usize);
    let mut pairs: BTreeMap<bool, usize> = BTreeMap::new();
    let mut partitions: BTreeMap<(bool, bool, bool), usize> = BTreeMap::new();
    let mut skin: BTreeMap<(u32, bool), usize> = BTreeMap::new();
    let mut index_stats: BTreeMap<(bool, bool), usize> = BTreeMap::new();
    let mut unknown: BTreeMap<[u8; 11], usize> = BTreeMap::new();
    let mut layouts: BTreeMap<(u32, u32, u32), usize> = BTreeMap::new();
    let mut tails: BTreeMap<(bool, isize), usize> = BTreeMap::new();
    for path in &forges {
        let f = Forge::open(path).unwrap();
        let mut r = File::open(path).unwrap();
        for (k, n) in f.names.iter().enumerate() {
            if n.name == "GlobalMetaFile" {
                continue;
            }
            let e = read_entry(&f.payload(&mut r, k).unwrap(), 1 << 30).unwrap();
            for o in read_objects(&e.metadata.data, &e.resource.data).unwrap() {
                if o.class != MESH {
                    continue;
                }
                let body = &e.resource.data[o.body.clone()];
                if let Err(Error::SubMeshes(_)) = StoredMesh::parse(body) {
                    raw += 1;
                    continue;
                }
                let m = StoredMesh::parse(body)
                    .unwrap_or_else(|err| panic!("{}: {}: {}: {err}", path.display(), n.name, o.name));
                let c = &m.compiled;
                assert_eq!(m.id, o.id);
                assert_eq!(c.ranges.len(), m.materials.len());
                let indices = c.index_bytes / 2;
                for part in [&c.ranges, &c.second_ranges] {
                    *partitions
                        .entry((
                            part.iter().all(|r| r.primitive == 4),
                            part.iter().map(|r| r.triangles * 3).sum::<u32>() == indices,
                            part.iter()
                                .all(|r| r.first_vertex + r.vertex_count <= c.vertex_bytes / c.stride),
                        ))
                        .or_default() += 1;
                }
                meshes += 1;
                *unknown.entry(m.unknown).or_default() += 1;
                *layouts.entry((c.flags, c.stride, c.morph_count)).or_default() += 1;
                let layout = c.layout().unwrap_or_else(|err| panic!("{}: {err}", o.name));
                let last = layout.last().unwrap();
                assert_eq!(u32::from(last.offset + last.kind.size()), c.stride);
                *skin.entry((c.flags & 3, !m.bones.is_empty())).or_default() += 1;
                let palettes = c
                    .palettes(body)
                    .unwrap_or_else(|err| panic!("{}: {}: {err}", n.name, o.name));
                for (i, p) in palettes.iter().enumerate() {
                    *index_stats
                        .entry((
                            usize::from(p.unknown) == i,
                            u32::from(p.vertex_count) == c.ranges[i].vertex_count,
                        ))
                        .or_default() += 1;
                    assert!(
                        p.bones.iter().all(|&b| usize::from(b) < m.bones.len().max(1)),
                        "{}",
                        o.name
                    );
                    if m.bones.is_empty() {
                        assert!(p.bones.is_empty());
                    }
                }
                *tails
                    .entry((
                        !m.bones.is_empty(),
                        palettes.iter().map(|p| p.bones.len()).max().unwrap_or(0) as isize,
                    ))
                    .or_default() += 1;
                *pairs.entry(c.ranges.len() == c.second_ranges.len()).or_default() += 1;
            }
        }
    }
    println!("{meshes} meshes; (flags, stride, morphs): {layouts:?}");
    println!("(skinned, largest palette): {tails:?}; second partition as long as the first: {pairs:?}");
    println!("(flags & 3, has bones): {skin:?}");
    println!(
        "partitions (all triangle lists, triangles fill the indices, vertices inside the stream): {partitions:?}"
    );
    assert_eq!(partitions.keys().collect::<Vec<_>>(), [&(true, true, true)]);
    println!("(palette u16 == position, vertex count == range's): {index_stats:?}");
    println!("eleven-byte runs: {unknown:?}");
    assert_eq!(unknown.len(), 2);
    assert_eq!((meshes, raw), (15_928, 221));
}

/// The SHA-256 of `bytes`, in lower-case hex.
fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every vertex and index buffer the game's renderer uploaded for a mesh (the `runtime` oracle) is the vertex stream or
/// index bytes this reader slices, and the vertex declaration the game drew it with (its stream 0) is
/// [`CompiledMesh::layout`](anvil_formats::shaunwhite::mesh::CompiledMesh::layout) at the mesh's stride.
#[test]
fn buffers_and_layouts_equal_what_the_game_uploads() {
    let (Some(folder), Some(table)) = (
        install(Game::ShaunWhite),
        inputs::oracle(inputs::OracleSet::Runtime, Game::ShaunWhite, "meshes.tsv"),
    ) else {
        return;
    };
    let text = std::fs::read_to_string(table).unwrap();
    let mut rows: BTreeMap<&str, BTreeMap<usize, Vec<Vec<&str>>>> = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        rows.entry(f[0])
            .or_default()
            .entry(f[1].parse().unwrap())
            .or_default()
            .push(f);
    }
    let (mut buffers, mut declarations, mut refused) = (0usize, 0usize, std::collections::BTreeSet::new());
    let mut layouts = std::collections::BTreeSet::new();
    for (forge, entries) in &rows {
        let path = folder.join(forge);
        let f = Forge::open(&path).unwrap();
        let mut r = File::open(&path).unwrap();
        for (&k, rows) in entries {
            let e = read_entry(&f.payload(&mut r, k).unwrap(), 1 << 30).unwrap();
            let objs = read_objects(&e.metadata.data, &e.resource.data).unwrap();
            for row in rows {
                let id: u32 = row[2].parse().unwrap();
                let o = objs.iter().find(|o| o.id == id && o.class == MESH).unwrap();
                let body = &e.resource.data[o.body.clone()];
                let m = match StoredMesh::parse(body) {
                    Err(Error::SubMeshes(_)) => {
                        refused.insert((forge.to_string(), k, id, !row[7].is_empty()));
                        continue;
                    }
                    other => other.unwrap(),
                };
                let c = &m.compiled;
                let part = if row[5] == "vertices" {
                    c.streams[0].clone()
                } else {
                    c.indices.clone()
                };
                assert_eq!(sha256(&body[part]), row[6], "{forge} {k} {id} {}", row[5]);
                buffers += 1;
                if row[5] != "vertices" || row[7].is_empty() {
                    continue;
                }
                // the declaration's stream-0 elements, without its end marker
                let declared: Vec<(u16, u8, u8, u8)> = row[7]
                    .split(',')
                    .map(|e| {
                        e.split(':')
                            .map(|v| v.parse::<u16>().unwrap())
                            .collect::<Vec<_>>()
                    })
                    .filter(|e| e[0] == 0)
                    .map(|e| (e[1], e[2] as u8, e[4] as u8, e[5] as u8))
                    .collect();
                let ours: Vec<(u16, u8, u8, u8)> = c
                    .layout()
                    .unwrap()
                    .iter()
                    .map(|e| (e.offset, e.kind.d3d(), e.usage.d3d(), e.index))
                    .collect();
                assert_eq!(ours, declared, "{forge} {k} {id}");
                assert_eq!(row[8], c.stride.to_string());
                layouts.insert((c.flags, c.stride));
                declarations += 1;
            }
        }
    }
    println!(
        "{buffers} uploaded buffers equal their stored bytes; {declarations} draw declarations equal the layout, over (flags, stride) {layouts:?}; {} meshes in the table keep raw sub-meshes, {} of them drawn",
        refused
            .iter()
            .map(|r| (&r.0, r.1, r.2))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        refused
            .iter()
            .filter(|r| r.3)
            .map(|r| (&r.0, r.1, r.2))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    assert!(buffers > 0 && declarations > 0);
}
