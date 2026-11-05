//! Shaun White Skateboarding Meshes: every Mesh of every data forge of `installs.shaunwhite` read to its exact end.
mod inputs;

use std::collections::BTreeMap;
use std::fs::File;

use anvil_formats::data::read_entry;
use anvil_formats::forge::Forge;
use anvil_formats::objects::read_objects;
use anvil_formats::shaunwhite::mesh::{
    Error, MAX_SUB_MESH_DEPTH, MESH, MORPH_CHANNEL, SUB_MESH, StoredMesh, UV_CHANNEL,
};
use inputs::{Game, install};

fn words(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// A SubMesh as the game lays it (without the pointer tag before it): two vertices, one triangle (0, 1, 1), its
/// colours and last vertex words, one UV channel, `morphs` morph channels, and a copy `depth` levels deep.
fn sub_mesh(id: u32, morphs: u32, depth: u32) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(words(&[id, SUB_MESH, 0, 3]));
    b.extend([0, 0, 1, 0, 1, 0]);
    b.extend(words(&[if depth == 0 { 0 } else { 2 }]));
    if depth != 0 {
        b.extend(words(&[7, 7]));
    }
    b.extend(words(&[0, 1, 0x11]));
    b.extend(words(&[1]));
    b.push(3);
    b.extend(words(&[2]));
    b.extend(floats(&[0., 1., 2., 3., 4., 5.]));
    b.extend(words(&[2]));
    b.extend(floats(&[0., 0., 1., 0., 0., 1.]));
    if depth == 0 {
        // the copy's tangents and binormals
        b.extend(words(&[2]));
        b.extend(floats(&[1., 0., 0., 1., 0., 0.]));
        b.extend(words(&[2]));
        b.extend(floats(&[0., 1., 0., 0., 1., 0.]));
    } else {
        b.extend(words(&[0, 0]));
    }
    b.extend(words(&[2, 0xFF00_0000, 0xFF00_00FF, 0, 0, 0, 2, 9, 9, 0, 0]));
    b.extend(words(&[1, 0, UV_CHANNEL, 0]));
    b.push(0x22);
    b.extend(words(&[2]));
    b.extend(floats(&[0., 0., 1., 1.]));
    b.extend(words(&[0, 0, morphs]));
    for m in 0..morphs {
        b.extend(words(&[40 + m, MORPH_CHANNEL, 0xBEEF, 2]));
        b.extend(floats(&[0., 1., 9., 3., 4., 9.]));
        b.extend(words(&[2]));
        b.extend(floats(&[0., 0., 1., 0., 0., 1.]));
        b.extend(words(&[2]));
        b.extend(floats(&[0., 0., 1., 1.]));
    }
    b.push(u8::from(morphs != 0));
    b.extend(words(&[0x1234]));
    b.extend([1, 1]);
    if depth == 0 {
        b.push(3);
    } else {
        b.push(0);
        b.extend(sub_mesh(id + 1, 0, depth - 1));
    }
    b.extend(words(&[0]));
    b
}

/// A mesh with `subs` (each a SubMesh with its pointer tag), one bone, two vertices of 12 bytes, three indices and
/// one range (with its palette), as the game lays it.
fn mesh_body_with(subs: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(words(&[5, MESH, 2]));
    b.push(0);
    b.extend(words(&[subs.len() as u32]));
    for sub in subs {
        b.push(0);
        b.extend(sub);
    }
    b.extend(words(&[1, 9, 0x9EF0_E7A1, 0xABCD]));
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

fn mesh_body() -> Vec<u8> {
    mesh_body_with(&[])
}

#[test]
fn hand_built_mesh_reads_and_refuses_what_does_not_fit() {
    let b = mesh_body();
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

    assert!(m.sub_meshes.is_empty());
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
fn hand_built_sub_meshes_read_and_refuse_what_does_not_fit() {
    let b = mesh_body_with(&[sub_mesh(20, 1, 1)]);
    let m = StoredMesh::parse(&b).unwrap();
    let s = &m.sub_meshes[0];
    assert_eq!(
        (s.id, s.material, s.has_morphs, s.unknown_flags),
        (20, 0x1234, 1, [1, 1])
    );
    assert_eq!(&b[s.indices.clone()], &[0, 0, 1, 0, 1, 0]);
    assert_eq!(&b[s.positions.clone()], &floats(&[0., 1., 2., 3., 4., 5.])[..]);
    assert_eq!(&b[s.colours.clone()], &words(&[0xFF00_0000, 0xFF00_00FF])[..]);
    assert_eq!(&b[s.vertex_words[2].clone()], &words(&[9, 9])[..]);
    assert_eq!(&b[s.source_vertices.clone()], &words(&[7, 7])[..]);
    assert!(s.vertex_words[0].is_empty());
    assert_eq!((s.face_words.len(), s.face_bytes.len()), (4, 1));
    let uv = &s.uv_channels[0];
    assert_eq!((uv.unknown, uv.uvs.len(), uv.source.len()), (0x22, 16, 0));
    let morph = &s.morphs[0];
    assert_eq!((morph.id, morph.name), (40, 0xBEEF));
    assert_eq!(
        &b[morph.positions.clone()],
        &floats(&[0., 1., 9., 3., 4., 9.])[..]
    );
    let copy = s.copy.as_ref().unwrap();
    assert_eq!((copy.id, copy.morphs.len(), copy.has_morphs), (21, 0, 0));
    assert!(copy.source_vertices.is_empty() && copy.copy.is_none() && s.tangents.is_empty());
    assert_eq!(&b[copy.binormals.clone()], &floats(&[0., 1., 0., 0., 1., 0.])[..]);
    // the rest of the body reads as without SubMeshes
    assert_eq!(m.compiled, {
        let plain = StoredMesh::parse(&mesh_body()).unwrap();
        let shift = b.len() - mesh_body().len();
        let moved = |r: &std::ops::Range<usize>| r.start + shift..r.end + shift;
        let mut c = plain.compiled;
        c.streams = c.streams.iter().map(moved).collect();
        c.indices = moved(&c.indices);
        c.tail = moved(&c.tail);
        c
    });

    // copies nested past the limit, a pointer tag that is neither inline nor none, a wrong class, an index count
    // that is not whole triangles, and a run of the wrong length
    assert!(matches!(
        StoredMesh::parse(&mesh_body_with(&[sub_mesh(20, 0, MAX_SUB_MESH_DEPTH as u32 + 1)])),
        Err(Error::Depth)
    ));
    let mut tag = sub_mesh(20, 0, 0);
    let at = tag.len() - 5;
    tag[at] = 5;
    assert!(matches!(
        StoredMesh::parse(&mesh_body_with(&[tag])),
        Err(Error::Value { what, .. }) if what.starts_with("SubMesh copy pointer tag")
    ));
    let mut class = sub_mesh(20, 0, 0);
    class[5] ^= 1;
    assert!(matches!(
        StoredMesh::parse(&mesh_body_with(&[class])),
        Err(Error::Value {
            what: "SubMesh class",
            ..
        })
    ));
    let mut odd = sub_mesh(20, 0, 0);
    odd[13] = 2;
    assert!(matches!(
        StoredMesh::parse(&mesh_body_with(&[odd])),
        Err(Error::Value {
            what: "SubMesh index count, a multiple of 3",
            ..
        })
    ));
    let mut normals = sub_mesh(20, 0, 0);
    // the normals' count: after the head and the index count (17), the indices (6), the source vertices (4), the u32
    // and the face words (4 + 4 + 4), the face bytes (4 + 1) and the positions (4 + 24)
    let at = 17 + 6 + 4 + 12 + 5 + 28;
    normals[at] = 1;
    assert!(matches!(
        StoredMesh::parse(&mesh_body_with(&[normals])),
        Err(Error::Value {
            what: "SubMesh normals",
            ..
        })
    ));
    let mut tangents = sub_mesh(20, 0, 0);
    tangents[at + 28] = 1;
    assert!(matches!(
        StoredMesh::parse(&mesh_body_with(&[tangents])),
        Err(Error::Value {
            what: "SubMesh tangents",
            ..
        })
    ));
}

/// The position bit patterns of `n` vertices of three f32 at `stride` bytes, sorted.
fn sorted_positions(bytes: &[u8], stride: usize, n: usize) -> Vec<[u8; 12]> {
    let mut v: Vec<[u8; 12]> = (0..n)
        .map(|i| {
            bytes[i * stride..i * stride + 12]
                .try_into()
                .expect("a slice of 12 bytes")
        })
        .collect();
    v.sort();
    v
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
    let mut sub_stats: BTreeMap<&'static str, usize> = BTreeMap::new();
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
                let m = StoredMesh::parse(body)
                    .unwrap_or_else(|err| panic!("{}: {}: {}: {err}", path.display(), n.name, o.name));
                let c = &m.compiled;
                if !m.sub_meshes.is_empty() {
                    raw += 1;
                    check_sub_meshes(&m, body, &mut sub_stats);
                } else {
                    assert_eq!(c.morph_count, 0, "{}", o.name);
                }
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
    println!("SubMeshes: {sub_stats:?}");
    assert_eq!((meshes, raw), (16_149, 221));
    assert_eq!(
        (
            sub_stats["SubMeshes"],
            sub_stats["copies"],
            sub_stats["with morph channels"]
        ),
        (349, 349, 323)
    );
}

/// The SubMeshes of a mesh against its compiled data: one range each, of their material, holding the same vertex
/// positions bit for bit and as many triangles; the vertex count is theirs summed; the morph streams are as many as the
/// most morph channels of a SubMesh; each copy has its SubMesh's indices, positions and material.
fn check_sub_meshes(m: &StoredMesh, body: &[u8], stats: &mut BTreeMap<&'static str, usize>) {
    let c = &m.compiled;
    assert_eq!(m.sub_meshes.len(), c.ranges.len());
    let stride = c.stride as usize;
    let mut vertices = 0;
    for s in &m.sub_meshes {
        let n = s.positions.len() / 12;
        vertices += n;
        let candidates: Vec<_> = c
            .ranges
            .iter()
            .zip(&m.materials)
            .filter(|(r, mat)| **mat == s.material && r.vertex_count as usize == n)
            .map(|(r, _)| r)
            .filter(|r| {
                let at = c.streams[0].start + r.first_vertex as usize * stride;
                sorted_positions(&body[at..], stride, n)
                    == sorted_positions(&body[s.positions.clone()], 12, n)
            })
            .collect();
        assert_eq!(candidates.len(), 1, "SubMesh {}", s.id);
        assert_eq!(candidates[0].triangles as usize * 3, s.indices.len() / 2);
        assert_eq!(s.has_morphs, u8::from(!s.morphs.is_empty()));
        assert!(s.morphs.len() <= c.morph_count as usize);
        let copy = s.copy.as_ref().expect("every stored SubMesh owns a copy");
        assert_eq!(&body[copy.indices.clone()], &body[s.indices.clone()]);
        assert_eq!(&body[copy.positions.clone()], &body[s.positions.clone()]);
        assert_eq!((copy.material, copy.morphs.len()), (s.material, 0));
        assert!(copy.copy.is_none() && copy.source_vertices.is_empty());
        assert!(s.tangents.is_empty() && s.binormals.is_empty());
        assert_eq!((copy.tangents.len(), copy.binormals.len()), (n * 12, n * 12));
        *stats.entry("SubMeshes").or_default() += 1;
        *stats.entry("copies").or_default() += 1;
        if !s.morphs.is_empty() {
            *stats.entry("with morph channels").or_default() += 1;
        }
        if !s.source_vertices.is_empty() {
            *stats.entry("with source vertices").or_default() += 1;
        }
        if !s.colours.is_empty() {
            *stats.entry("with colours").or_default() += 1;
        }
    }
    assert_eq!(vertices, (c.vertex_bytes / c.stride) as usize);
    assert_eq!(
        m.sub_meshes.iter().map(|s| s.morphs.len()).max().unwrap_or(0),
        c.morph_count as usize
    );
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
    let (mut buffers, mut declarations, mut raw) = (0usize, 0usize, std::collections::BTreeSet::new());
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
                let m = StoredMesh::parse(body).unwrap();
                if !m.sub_meshes.is_empty() {
                    raw.insert((forge.to_string(), k, id, !row[7].is_empty()));
                }
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
        "{buffers} uploaded buffers equal their stored bytes; {declarations} draw declarations equal the layout, over (flags, stride) {layouts:?}; {} meshes in the table keep SubMeshes, {} of them drawn",
        raw.iter()
            .map(|r| (&r.0, r.1, r.2))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        raw.iter()
            .filter(|r| r.3)
            .map(|r| (&r.0, r.1, r.2))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    assert!(buffers > 0 && declarations > 0);
}
