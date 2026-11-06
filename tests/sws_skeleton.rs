//! Shaun White Skateboarding Skeletons: a body laid out here by hand and its refusals, and every Skeleton of every
//! data forge of `installs.shaunwhite` read to its exact end, its bind pose consistent bone to bone.
mod inputs;
mod skinning;

use std::collections::{BTreeMap, BTreeSet};
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

/// The game skins with each bone's MeshBone matrix as the inverse bind (the `runtime` oracle's `g_Bones` registers
/// are the bone's pose times the stored matrix, transposed to column form): for a bone of a traced skinned draw whose
/// parent is in the same draw, the local translation the two bones' registers and MeshBone matrices give,
/// `IB_p · S_p⁻¹ · S_c · IB_c⁻¹`, is the bone's stored local position in its Skeleton, unless something moves it.
/// Each palette entry also names the MeshBone the oracle's row does. Of the trace's 6,170 such pairs, 2,876 are at
/// their stored position, and the rest can't be:
/// - in the first frame the trace meets them, every pair of the train, the five pigeons and the walking worker's legs
///   is at its stored position, and every pigeon pair stays there in every frame; the test asserts these;
/// - 704 pairs are bones a stored clip translates: the train's and the walking worker's, whose poses
///   `traced_skinned_frames_are_decoded_clip_poses` checks against their clips' translations;
/// - 2,550 pairs are on two walking characters' four meshes and two heads, up to 2.4 m from their stored positions
///   with no clip translating those bones: no stored clip poses those meshes (the same test), and what moves them is
///   not known; 40 are the one train bone its clip does not drive.
#[test]
fn skinned_draws_place_child_bones_at_their_stored_local_positions() {
    let (Some(folder), Some(table)) = (
        install(Game::ShaunWhite),
        inputs::oracle(inputs::OracleSet::Runtime, Game::ShaunWhite, "skin.tsv"),
    ) else {
        return;
    };
    let draws = skinning::draws(&table);
    let data = skinning::install(&folder, &draws, false);
    let mut first_frame: BTreeMap<(&str, usize, u32), u32> = BTreeMap::new();
    for d in &draws {
        let f = first_frame.entry((&d.forge, d.entry, d.mesh)).or_insert(d.frame);
        *f = (*f).min(d.frame);
    }
    let (mut pairs, mut placed, mut entries) = (0usize, 0usize, 0usize);
    // each mesh's pairs in the first frame the trace meets it: (at their stored position, all)
    let mut first: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let mut pigeons_off = 0usize;
    for d in &draws {
        let (mesh, palettes) = &data.meshes[&(d.forge.clone(), d.entry, d.mesh)];
        for (e, crc, _) in &d.bones {
            assert_eq!(mesh.bones[usize::from(palettes[d.range].bones[*e])].name, *crc);
            entries += 1;
        }
        let locals = skinning::locals(d, mesh, palettes, &data.skeletons)
            .unwrap_or_else(|| panic!("{}: mesh {}: no Skeleton holds the draw's bones", d.forge, d.mesh));
        let ok = locals.iter().filter(|l| skinning::at_rest_position(l)).count();
        pairs += locals.len();
        placed += ok;
        if first_frame[&(d.forge.as_str(), d.entry, d.mesh)] == d.frame {
            let f = first.entry(d.mesh).or_default();
            f.0 += ok;
            f.1 += locals.len();
        }
        if PIGEONS.contains(&d.mesh) {
            pigeons_off += locals.len() - ok;
        }
    }
    let at_rest: BTreeSet<u32> = first
        .iter()
        .filter(|(_, (ok, all))| *all > 0 && ok == all)
        .map(|(m, _)| *m)
        .collect();
    println!(
        "{} skinned draws, {entries} palette entries; {placed} of {pairs} child bones at their stored local position; \
         by mesh, its first frame's pairs at their stored position (of all) {first:?}",
        draws.len()
    );
    let mut expected = BTreeSet::from(PIGEONS);
    expected.extend([TRAIN, WORKER_LEGS]);
    assert_eq!(at_rest, expected);
    assert_eq!(pigeons_off, 0);
}

/// The traced train's skinned mesh.
const TRAIN: u32 = 2_445_827_479;
/// The traced walking worker's legs.
const WORKER_LEGS: u32 = 2_881_073_162;
/// The five traced pigeons' skinned meshes.
const PIGEONS: [u32; 5] = [
    1_814_156_304,
    1_814_156_345,
    1_814_156_367,
    1_950_109_412,
    1_950_109_447,
];
