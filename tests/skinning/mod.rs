//! The `runtime` oracle's skinned draws (`skin.tsv`) and the local bone transforms their bone matrices give, shared by
//! the Skeleton and Animation tests.
// test support, not a test: unwraps are the tests' failures, as in tests themselves
#![allow(dead_code, clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::Path;

use anvil_formats::data::read_entry;
use anvil_formats::forge::Forge;
use anvil_formats::objects::read_objects;
use anvil_formats::shaunwhite::animation::{ANIMATION, Animation};
use anvil_formats::shaunwhite::mesh::{Palette, StoredMesh};
use anvil_formats::shaunwhite::skeleton::{SKELETON, Skeleton};

/// A draw's key in the oracle: forge, entry, mesh id, frame, draw range.
type DrawKey = (String, usize, u32, u32, usize);

/// A 4×4 matrix, rows of column-vector form.
pub type M4 = [[f64; 4]; 4];

pub fn mul4(a: &M4, b: &M4) -> M4 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..4).map(|k| a[i][k] * b[k][j]).sum()))
}

/// The inverse by Gauss-Jordan elimination with partial pivoting.
pub fn inv4(m: &M4) -> M4 {
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

/// The unit quaternion `[x, y, z, w]` of a matrix's rotation part.
pub fn rotation(m: &M4) -> [f64; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        return [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            s / 4.0,
        ];
    }
    let i = (0..3)
        .max_by(|&a, &b| m[a][a].total_cmp(&m[b][b]))
        .expect("a diagonal");
    let (j, k) = ((i + 1) % 3, (i + 2) % 3);
    let s = (1.0 + m[i][i] - m[j][j] - m[k][k]).sqrt() * 2.0;
    let mut q = [0.0; 4];
    q[i] = s / 4.0;
    q[j] = (m[j][i] + m[i][j]) / s;
    q[k] = (m[k][i] + m[i][k]) / s;
    q[3] = (m[k][j] - m[j][k]) / s;
    q
}

/// One traced skinned draw: the mesh it drew, the frame, the draw range and each palette entry's MeshBone name CRC
/// and bone matrix (the three `g_Bones` registers, as a column-form 4×4).
pub struct Draw {
    pub forge: String,
    pub entry: usize,
    pub mesh: u32,
    pub frame: u32,
    pub range: usize,
    pub bones: Vec<(usize, u32, M4)>,
}

/// The draws of `skin.tsv`, in its order of (forge, entry, mesh, frame, range).
pub fn draws(table: &Path) -> Vec<Draw> {
    let mut by: BTreeMap<DrawKey, Vec<(usize, u32, M4)>> = BTreeMap::new();
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
        by.entry(key).or_default().push((f[5].parse().unwrap(), crc, s));
    }
    by.into_iter()
        .map(|((forge, entry, mesh, frame, range), bones)| Draw {
            forge,
            entry,
            mesh,
            frame,
            range,
            bones,
        })
        .collect()
}

/// What the install holds for the draws: every Skeleton, the drawn meshes with their palettes and, if asked, every
/// Animation (one per id).
pub struct Install {
    pub skeletons: Vec<Skeleton>,
    pub meshes: BTreeMap<(String, usize, u32), (StoredMesh, Vec<Palette>)>,
    pub animations: Vec<Animation>,
}

pub fn install(folder: &Path, draws: &[Draw], animations: bool) -> Install {
    let mut forges: Vec<_> = std::fs::read_dir(folder)
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
    let wanted: BTreeSet<(String, usize, u32)> =
        draws.iter().map(|d| (d.forge.clone(), d.entry, d.mesh)).collect();
    let (mut skeletons, mut meshes, mut clips, mut seen) =
        (Vec::new(), BTreeMap::new(), Vec::new(), BTreeSet::new());
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
                } else if animations && o.class == ANIMATION && seen.insert(o.id) {
                    clips.push(Animation::parse(&e.resource.data[o.body.start..o.trailer.end]).unwrap());
                } else if wanted.contains(&(fname.clone(), k, o.id)) {
                    let m = StoredMesh::parse(body).unwrap();
                    let palettes = m.compiled.palettes(body).unwrap();
                    meshes.insert((fname.clone(), k, o.id), (m, palettes));
                }
            }
        }
    }
    assert_eq!(meshes.len(), wanted.len());
    Install {
        skeletons,
        meshes,
        animations: clips,
    }
}

/// A traced child bone: its name CRC, its local transform from the draw (`IB_p · S_p⁻¹ · S_c · IB_c⁻¹`, `S` its
/// registers, `IB` its MeshBone matrix in column form) and its stored local position and rotation in the Skeleton.
pub struct Local {
    pub bone: u32,
    pub traced: M4,
    pub position: [f32; 4],
    pub rotation: [f32; 4],
}

/// The child bones of a draw whose parent is in the same draw, in the Skeleton that holds every bone of the draw and
/// places most of them at their stored local position; `None` where no Skeleton holds them all.
pub fn locals(
    d: &Draw,
    mesh: &StoredMesh,
    palettes: &[Palette],
    skeletons: &[Skeleton],
) -> Option<Vec<Local>> {
    let bind = |e: usize| -> M4 {
        let m = mesh.bones[usize::from(palettes[d.range].bones[e])]
            .matrix
            .map(f64::from);
        std::array::from_fn(|i| std::array::from_fn(|j| m[4 * j + i]))
    };
    let of = |s: &Skeleton| -> Vec<Local> {
        let mut out = Vec::new();
        for (e, crc, sc) in &d.bones {
            let c = s.bones.iter().find(|b| b.name == *crc).unwrap();
            let Some(p) = c.parent.and_then(|p| s.bones.iter().find(|b| b.id == p)) else {
                continue;
            };
            let Some((pe, _, sp)) = d.bones.iter().find(|b| b.1 == p.name) else {
                continue;
            };
            out.push(Local {
                bone: *crc,
                traced: mul4(&mul4(&mul4(&bind(*pe), &inv4(sp)), sc), &inv4(&bind(*e))),
                position: c.local_position,
                rotation: c.local_rotation,
            });
        }
        out
    };
    let placed = |l: &[Local]| l.iter().filter(|l| at_rest_position(l)).count();
    skeletons
        .iter()
        .filter(|s| {
            d.bones
                .iter()
                .all(|(_, c, _)| s.bones.iter().any(|b| b.name == *c))
        })
        .map(of)
        .max_by_key(|l| (placed(l), l.len()))
}

/// Whether the traced local translation is the stored local position, within 1e-3.
pub fn at_rest_position(l: &Local) -> bool {
    (0..3).all(|i| (l.traced[i][3] - f64::from(l.position[i])).abs() < 1e-3)
}
