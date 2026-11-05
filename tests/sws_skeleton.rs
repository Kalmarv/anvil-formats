//! Shaun White Skateboarding Skeletons: a body laid out here by hand and its refusals, and every Skeleton of every
//! data forge of `installs.shaunwhite` read to its exact end, its bind pose consistent bone to bone.
mod inputs;

use std::collections::BTreeMap;
use std::fs::File;

use anvil_formats::data::read_entry;
use anvil_formats::forge::Forge;
use anvil_formats::objects::read_objects;
use anvil_formats::shaunwhite::skeleton::{
    BONE, BONE_INITIAL_TRANSFORMS, BONE_MIRROR_DATA, Error, ORIENTATION_CONSTRAINT, SKELETON,
    SKELETON_MIRROR_DATA, Skeleton,
};
use inputs::{Game, install};

fn words(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// A bone as the game lays it: `parent` 0 for none; a translation of 1 along x from its parent; `constrained` adds an
/// orientation constraint on itself with no targets.
fn bone(id: u32, parent: u32, x: f32, index: u8, below: u8, constrained: bool) -> Vec<u8> {
    let mut b = vec![0, 1];
    b.extend(words(&[id, BONE, id + 1000]));
    if parent == 0 {
        b.push(3);
    } else {
        b.push(2);
        b.extend(words(&[parent]));
    }
    b.push(0);
    b.extend(words(&[id + 2000, BONE_INITIAL_TRANSFORMS]));
    b.extend(floats(&[
        x, 0., 0., 0., 0., 0., 0., 1., 1., 0., 0., 0., 0., 0., 0., 1.,
    ]));
    if constrained {
        b.extend(words(&[1]));
        b.push(0);
        b.extend(words(&[id + 3000, ORIENTATION_CONSTRAINT]));
        b.push(2);
        b.extend(words(&[id]));
        b.push(0);
        b.extend(words(&[0]));
        b.push(1);
        b.extend(floats(&[0., 0., 0., 1.]));
    } else {
        b.extend(words(&[0]));
    }
    b.extend([index, below]);
    b
}

/// A three-bone chain (root, child, grandchild) with one mirror pair and no pose groups, IK or COM data.
fn skeleton(bones: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(words(&[7, SKELETON, 2, bones.len() as u32]));
    for x in bones {
        b.extend(x);
    }
    b.push(2);
    b.extend(words(&[10, 0, 0]));
    b.extend([3, 3]);
    b.extend(words(&[
        0x55,
        8,
        SKELETON_MIRROR_DATA,
        1,
        9,
        BONE_MIRROR_DATA,
        1010,
        1020,
    ]));
    b.extend([5, 0x55]);
    b
}

fn chain() -> Vec<Vec<u8>> {
    vec![
        bone(10, 0, 0., 0, 2, false),
        bone(20, 10, 1., 1, 1, true),
        bone(30, 20, 2., 2, 0, false),
    ]
}

#[test]
fn hand_built_skeleton_reads_and_refuses_what_does_not_fit() {
    let s = Skeleton::parse(&skeleton(&chain())).unwrap();
    assert_eq!((s.id, s.kind, s.bones.len(), s.root), (7, 2, 3, Some(10)));
    let b = &s.bones[2];
    assert_eq!(
        (b.id, b.name, b.parent, b.index, b.below),
        (30, 1030, Some(20), 2, 0)
    );
    assert_eq!(b.global_position, [2., 0., 0., 0.]);
    assert_eq!(b.local_position, [1., 0., 0., 0.]);
    assert_eq!(s.bones[1].constraints[0].bone, Some(20));
    assert_eq!((s.mirrors[0].first, s.mirrors[0].flags), (1010, [5, 0x55]));
    assert!(s.ik.is_none() && s.com.is_none() && s.pose_groups.is_empty());

    // a parent after its child, a wrong count of bones below, a wrong index, a byte after the end
    let mut late = chain();
    late.swap(0, 1);
    assert!(matches!(
        Skeleton::parse(&skeleton(&late)),
        Err(Error::Value { .. })
    ));
    let mut below = chain();
    below[0] = bone(10, 0, 0., 0, 1, false);
    assert!(matches!(
        Skeleton::parse(&skeleton(&below)),
        Err(Error::Value {
            what: "bones below",
            ..
        })
    ));
    let mut index = chain();
    index[2] = bone(30, 20, 2., 5, 0, false);
    assert!(matches!(
        Skeleton::parse(&skeleton(&index)),
        Err(Error::Value {
            what: "bone index",
            ..
        })
    ));
    let body = skeleton(&chain());
    assert!(matches!(
        Skeleton::parse(&[body.clone(), vec![0]].concat()),
        Err(Error::Trailing(1))
    ));
    assert!(matches!(
        Skeleton::parse(&body[..body.len() - 1]),
        Err(Error::Short(_))
    ));
}

type Quat = [f32; 4];
fn mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}
fn rotate(q: Quat, v: [f32; 4]) -> [f32; 4] {
    let p = mul(mul(q, [v[0], v[1], v[2], 0.]), [-q[0], -q[1], -q[2], q[3]]);
    [p[0], p[1], p[2], 0.]
}

/// Every Skeleton object of the 33 data forges reads to its exact end, and every non-root bone's global bind pose is
/// its parent's global pose composed with its local one (within 1e-3; rotations equal up to sign).
#[test]
fn every_real_skeleton_reads_and_its_bind_pose_composes() {
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
    let (mut skeletons, mut bones, mut composed) = (0usize, 0usize, 0usize);
    let mut parts: BTreeMap<&str, usize> = BTreeMap::new();
    for path in &forges {
        let f = Forge::open(path).unwrap();
        let mut r = File::open(path).unwrap();
        for (k, n) in f.names.iter().enumerate() {
            if n.name == "GlobalMetaFile" {
                continue;
            }
            let e = read_entry(&f.payload(&mut r, k).unwrap(), 1 << 30).unwrap();
            for o in read_objects(&e.metadata.data, &e.resource.data).unwrap() {
                if o.class != SKELETON {
                    continue;
                }
                let s = Skeleton::parse(&e.resource.data[o.body.clone()])
                    .unwrap_or_else(|err| panic!("{}: {}: {}: {err}", path.display(), n.name, o.name));
                assert_eq!(s.id, o.id);
                assert_eq!(s.root, s.bones.first().map(|b| b.id));
                skeletons += 1;
                for b in &s.bones {
                    bones += 1;
                    let Some(p) = b.parent else {
                        assert_eq!(
                            (b.global_position, b.global_rotation),
                            (b.local_position, b.local_rotation)
                        );
                        continue;
                    };
                    let p = s.bones.iter().find(|x| x.id == p).unwrap();
                    let q = mul(p.global_rotation, b.local_rotation);
                    let t = rotate(p.global_rotation, b.local_position);
                    let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
                    let at: [f32; 4] = std::array::from_fn(|i| p.global_position[i] + t[i]);
                    assert!(close(at, b.global_position), "{}: bone {}", o.name, b.index);
                    assert!(
                        close(q, b.global_rotation) || close(q.map(|x| -x), b.global_rotation),
                        "{}: bone {}",
                        o.name,
                        b.index
                    );
                    composed += 1;
                }
                *parts.entry("constraints").or_default() +=
                    s.bones.iter().map(|b| b.constraints.len()).sum::<usize>();
                *parts.entry("pose groups").or_default() += s.pose_groups.len();
                *parts.entry("with IK").or_default() += usize::from(s.ik.is_some());
                *parts.entry("with COM").or_default() += usize::from(s.com.is_some());
                *parts.entry("mirror pairs").or_default() += s.mirrors.len();
            }
        }
    }
    println!("{skeletons} Skeletons, {bones} bones, {composed} composed from their parents; {parts:?}");
    assert!(skeletons > 0 && composed > 0);
}

/// A 4×4 matrix, rows of column-vector form.
type M4 = [[f64; 4]; 4];

fn mul4(a: &M4, b: &M4) -> M4 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..4).map(|k| a[i][k] * b[k][j]).sum()))
}

/// The inverse by Gauss-Jordan elimination with partial pivoting.
fn inv4(m: &M4) -> M4 {
    let mut a: [[f64; 8]; 4] = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            if j < 4 {
                m[i][j]
            } else {
                f64::from(u8::from(j - 4 == i))
            }
        })
    });
    for c in 0..4 {
        let p = (c..4)
            .max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))
            .expect("a pivot row");
        a.swap(c, p);
        let d = a[c][c];
        a[c] = a[c].map(|x| x / d);
        for r in 0..4 {
            if r != c {
                let f = a[r][c];
                let row = a[c];
                for (x, y) in a[r].iter_mut().zip(row) {
                    *x -= f * y;
                }
            }
        }
    }
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j + 4]))
}

/// The game skins with each bone's MeshBone matrix as the inverse bind (the `runtime` oracle's `g_Bones` registers
/// are the bone's pose times the stored matrix, transposed to column form): for every bone of a traced skinned draw
/// whose parent is in the same draw, the local translation the two bones' registers and MeshBone matrices give,
/// `IB_p · S_p⁻¹ · S_c · IB_c⁻¹`, is the bone's stored local position in its Skeleton. Every pair holds in the first
/// frame each mesh is drawn; later frames move a few bones by the clip's animation. Each palette entry also names the
/// MeshBone the oracle's row does.
#[test]
fn skinned_draws_place_child_bones_at_their_stored_local_positions() {
    let (Some(folder), Some(table)) = (
        install(Game::ShaunWhite),
        inputs::oracle(inputs::OracleSet::Runtime, Game::ShaunWhite, "skin.tsv"),
    ) else {
        return;
    };
    type Draw = (String, usize, u32, u32, usize);
    let mut draws: BTreeMap<Draw, Vec<(usize, u32, M4)>> = BTreeMap::new();
    for line in std::fs::read_to_string(table).unwrap().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let v: Vec<f64> = f[7]
            .split(' ')
            .map(|w| f64::from(f32::from_bits(u32::from_str_radix(w, 16).unwrap())))
            .collect();
        let s: M4 = std::array::from_fn(|i| {
            if i < 3 {
                std::array::from_fn(|j| v[4 * i + j])
            } else {
                [0.0, 0.0, 0.0, 1.0]
            }
        });
        let key = (
            f[0].to_string(),
            f[1].parse().unwrap(),
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
            f[4].parse().unwrap(),
        );
        let crc = u32::from_str_radix(f[6].trim_start_matches("0x"), 16).unwrap();
        draws
            .entry(key)
            .or_default()
            .push((f[5].parse().unwrap(), crc, s));
    }
    // every Skeleton of the install, and the meshes the oracle names
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
    let wanted: std::collections::BTreeSet<(String, usize, u32)> =
        draws.keys().map(|(f, e, m, _, _)| (f.clone(), *e, *m)).collect();
    let mut skeletons = Vec::new();
    let mut meshes = BTreeMap::new();
    for path in &forges {
        let fname = path.file_name().unwrap().to_string_lossy().to_string();
        let f = Forge::open(path).unwrap();
        let mut r = File::open(path).unwrap();
        for (k, n) in f.names.iter().enumerate() {
            if n.name == "GlobalMetaFile" {
                continue;
            }
            let e = read_entry(&f.payload(&mut r, k).unwrap(), 1 << 30).unwrap();
            for o in read_objects(&e.metadata.data, &e.resource.data).unwrap() {
                let body = &e.resource.data[o.body.clone()];
                if o.class == SKELETON {
                    skeletons.push(Skeleton::parse(body).unwrap());
                } else if wanted.contains(&(fname.clone(), k, o.id)) {
                    let m = anvil_formats::shaunwhite::mesh::StoredMesh::parse(body).unwrap();
                    let palettes = m.compiled.palettes(body).unwrap();
                    meshes.insert((fname.clone(), k, o.id), (m, palettes));
                }
            }
        }
    }
    assert_eq!(meshes.len(), wanted.len());
    let first_frame: BTreeMap<(String, usize, u32), u32> =
        draws.keys().fold(BTreeMap::new(), |mut m, (f, e, id, frame, _)| {
            let x = m.entry((f.clone(), *e, *id)).or_insert(*frame);
            *x = (*x).min(*frame);
            m
        });
    let (mut pairs, mut placed, mut first_pairs, mut entries) = (0usize, 0usize, 0usize, 0usize);
    for ((forge, entry, id, frame, range), bones) in &draws {
        let (mesh, palettes) = &meshes[&(forge.clone(), *entry, *id)];
        let bind = |e: usize| -> M4 {
            let m = mesh.bones[usize::from(palettes[*range].bones[e])]
                .matrix
                .map(f64::from);
            std::array::from_fn(|i| std::array::from_fn(|j| m[4 * j + i]))
        };
        for (e, crc, _) in bones {
            assert_eq!(mesh.bones[usize::from(palettes[*range].bones[*e])].name, *crc);
            entries += 1;
        }
        let names: std::collections::BTreeSet<u32> = bones.iter().map(|b| b.1).collect();
        // the Skeleton whose bones place most of the draw's children
        let score = |s: &Skeleton| -> (usize, usize) {
            let (mut n, mut ok) = (0, 0);
            for (e, crc, sc) in bones {
                let c = s.bones.iter().find(|b| b.name == *crc).unwrap();
                let Some(p) = c.parent.and_then(|p| s.bones.iter().find(|b| b.id == p)) else {
                    continue;
                };
                let Some((pe, _, sp)) = bones.iter().find(|b| b.1 == p.name) else {
                    continue;
                };
                let l = mul4(&mul4(&mul4(&bind(*pe), &inv4(sp)), sc), &inv4(&bind(*e)));
                n += 1;
                ok += usize::from((0..3).all(|i| (l[i][3] - f64::from(c.local_position[i])).abs() < 1e-3));
            }
            (n, ok)
        };
        let (n, ok) = skeletons
            .iter()
            .filter(|s| names.iter().all(|c| s.bones.iter().any(|b| b.name == *c)))
            .map(score)
            .max_by_key(|&(n, ok)| (ok, n))
            .unwrap_or_else(|| panic!("{forge}: mesh {id}: no Skeleton holds the draw's bones"));
        pairs += n;
        placed += ok;
        if first_frame[&(forge.clone(), *entry, *id)] == *frame {
            assert_eq!(ok, n, "{forge}: mesh {id} frame {frame} range {range}");
            first_pairs += n;
        }
    }
    println!(
        "{} skinned draws, {entries} palette entries; {placed} of {pairs} child bones at their stored local position, all {first_pairs} in each mesh's first frame",
        draws.len()
    );
    assert!(first_pairs > 0);
}
