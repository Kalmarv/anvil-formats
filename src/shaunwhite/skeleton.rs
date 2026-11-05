//! Shaun White Skateboarding (PC) Skeleton objects: the bone tree of a character, prop or rig with its bind pose, its
//! orientation constraints, poses, IK and centre-of-mass data and its mirror pairs, read as stored.
//!
//! The layout follows the engine's class order as the later [`crate::skeleton`] has it, with 32-bit object ids. The
//! body (little-endian):
//! - the managed byte (1), the object's id, its class ([`SKELETON`]), a u32 kept as stored (0 or 2) and the bone count;
//! - the bones ([`Bone`]), each a pointer tag 0 (inline), a managed byte 1, an id, the class [`BONE`], the CRC-32 of
//!   the bone's name, its parent (a reference: tag 3 none, or 2 and the id of a bone before it), its
//!   BoneInitialTransforms (tag 0, an id, the class [`BONE_INITIAL_TRANSFORMS`] and sixteen f32: the global position
//!   and rotation, then the local ones), its orientation constraints ([`OrientationConstraint`], a u32 count), its
//!   index in the bone list and the number of bones below it (a byte each);
//! - the root (a reference to the first bone), a u32 kept as stored, the pose groups ([`PoseGroup`], a u32 count),
//!   the IK data (a pointer: tag 3 none, or 0 and an inline [`IK_DATA`] object holding a u32 count of bone name
//!   CRCs), the centre-of-mass data (tag 3 none, or 0 and an inline [`COM_DATA`] object of two f32 x 4), a u32 kept as
//!   stored, and the mirror data: an id, the class [`SKELETON_MIRROR_DATA`] and a u32 count of [`BoneMirror`]s. The body
//!   ends there.
//!
//! A position is x, y, z and 0; a rotation is a unit quaternion x, y, z, w. Every one of the Skeletons of the 33 data
//! forges of the PC patched build (forges of 2010-10-27) reads so, to its exact end, and on every non-root bone the
//! global transform is the parent's global transform composed with the bone's local one.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use primase_foundations::bytes::{Cursor, Field, FieldError};

use crate::fields::LE;

pub use crate::skeleton::SKELETON;
/// The class id of a Bone (the CRC-32 of `Bone`).
pub const BONE: u32 = 0x9574_1049;
/// The class id of a bone's bind pose (the CRC-32 of `BoneInitialTransforms`).
pub const BONE_INITIAL_TRANSFORMS: u32 = 0x6350_E5A6;
/// The class id of an orientation constraint (the CRC-32 of `OrientationConstraintBoneModifier`).
pub const ORIENTATION_CONSTRAINT: u32 = 0x7511_6750;
/// The class id of a constraint's target (the CRC-32 of `ConstraintTarget`).
pub const CONSTRAINT_TARGET: u32 = 0xDF63_8110;
/// The class id of a pose group (the CRC-32 of `SkeletonPoseGroup`).
pub const SKELETON_POSE_GROUP: u32 = 0xC8F8_ABFF;
/// The class id of a pose (the CRC-32 of `SkeletonPose`).
pub const SKELETON_POSE: u32 = 0x209C_F072;
/// The class id of one bone of a pose (the CRC-32 of `SkeletonPoseBone`).
pub const SKELETON_POSE_BONE: u32 = 0x071C_EE50;
/// The class id of the IK data (the CRC-32 of `IKData`).
pub const IK_DATA: u32 = 0x9A33_101E;
/// The class id of the centre-of-mass data (the CRC-32 of `COMData`).
pub const COM_DATA: u32 = 0x9A67_179A;
/// The class id of the mirror data (the CRC-32 of `SkeletonMirrorData`).
pub const SKELETON_MIRROR_DATA: u32 = 0xE96B_19A5;
/// The class id of one mirror pair (the CRC-32 of `BoneMirrorData`).
pub const BONE_MIRROR_DATA: u32 = 0xE03C_A1FD;

/// Why a Skeleton body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body ends before a field is complete.
    #[error("Skeleton body: {0}")]
    Short(#[from] FieldError),
    /// A fixed value (a managed byte, a pointer tag, a class id) or a relation (a parent read before, a bone's index
    /// or count of bones below, the root, a constraint's bone) is not what the layout gives.
    #[error("Skeleton {what}: {found}, expected {expected}")]
    Value {
        /// The field refused.
        what: &'static str,
        /// The value found.
        found: u64,
        /// The value this reader accepts there.
        expected: u64,
    },
    /// Bytes follow the mirror data.
    #[error("Skeleton: {0} bytes after the mirror data")]
    Trailing(usize),
}

/// A Skeleton body as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    /// The Skeleton object's id.
    pub id: u32,
    /// The u32 after the class, kept as stored (0 or 2).
    pub kind: u32,
    /// The bones, parents before their children.
    pub bones: Vec<Bone>,
    /// The root bone's id (the first bone on every stored skeleton).
    pub root: Option<u32>,
    /// The u32 after the root, kept as stored.
    pub unknown_after_root: u32,
    /// The pose groups.
    pub pose_groups: Vec<PoseGroup>,
    /// The IK data: its object id and the CRC-32s of the bone names it lists.
    pub ik: Option<(u32, Vec<u32>)>,
    /// The centre-of-mass data: its object id and two f32 x 4 kept as stored (their fourth components are 0).
    pub com: Option<(u32, [[f32; 4]; 2])>,
    /// The u32 before the mirror data, kept as stored.
    pub unknown_before_mirrors: u32,
    /// The mirror data's object id.
    pub mirror_id: u32,
    /// The mirror pairs.
    pub mirrors: Vec<BoneMirror>,
}

/// One bone and its bind pose.
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// The Bone object's id: what a parent, the root and a constraint name.
    pub id: u32,
    /// The CRC-32 of the bone's name.
    pub name: u32,
    /// The parent's id (none for the root).
    pub parent: Option<u32>,
    /// The BoneInitialTransforms object's id.
    pub transforms_id: u32,
    /// The bind pose in the skeleton's space: x, y, z and 0.
    pub global_position: [f32; 4],
    /// The bind pose's rotation in the skeleton's space: a unit quaternion x, y, z, w.
    pub global_rotation: [f32; 4],
    /// The bind pose in the parent's space: x, y, z and 0.
    pub local_position: [f32; 4],
    /// The bind pose's rotation in the parent's space: a unit quaternion x, y, z, w.
    pub local_rotation: [f32; 4],
    /// The bone's orientation constraints (none on most bones).
    pub constraints: Vec<OrientationConstraint>,
    /// The bone's index in [`Skeleton::bones`].
    pub index: u8,
    /// How many bones lie below this one (its descendants, which follow it in the list).
    pub below: u8,
}

/// An orientation constraint of a bone.
#[derive(Clone, Debug, PartialEq)]
pub struct OrientationConstraint {
    /// The object's id.
    pub id: u32,
    /// The constrained bone's id (the bone that holds it, on every stored constraint).
    pub bone: Option<u32>,
    /// The byte after it, kept as stored (0 on every stored constraint).
    pub unknown: u8,
    /// The targets: a bone of the skeleton each, with an f32 kept as stored (50 on every stored target).
    pub targets: Vec<(u32, Option<u32>, f32)>,
    /// The byte before the rotation, kept as stored (0 or 1).
    pub flag: u8,
    /// A unit quaternion x, y, z, w, kept as stored.
    pub rotation: [f32; 4],
}

/// A group of poses.
#[derive(Clone, Debug, PartialEq)]
pub struct PoseGroup {
    /// The object's id.
    pub id: u32,
    /// The poses.
    pub poses: Vec<Pose>,
}

/// One pose: some bones' transforms.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    /// The object's id.
    pub id: u32,
    /// The u32 after the class, kept as stored.
    pub unknown: u32,
    /// The bones it moves: the object id, the CRC-32 of the bone's name, a position (x, y, z, 0) and a unit
    /// quaternion (x, y, z, w).
    pub bones: Vec<(u32, u32, [f32; 4], [f32; 4])>,
}

/// One mirror pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoneMirror {
    /// The object's id.
    pub id: u32,
    /// The first bone's name CRC.
    pub first: u32,
    /// The second bone's name CRC.
    pub second: u32,
    /// Two bytes kept as stored.
    pub flags: [u8; 2],
}

fn word(c: &mut Cursor<'_>, name: &'static str) -> Result<u32, Error> {
    Ok(c.u32(LE).field(name)?)
}
fn expect(what: &'static str, found: impl Into<u64>, expected: impl Into<u64>) -> Result<(), Error> {
    let (found, expected) = (found.into(), expected.into());
    if found == expected {
        Ok(())
    } else {
        Err(Error::Value {
            what,
            found,
            expected,
        })
    }
}
fn n64(v: usize) -> u64 {
    u64::try_from(v).unwrap_or(u64::MAX)
}
fn floats<const N: usize>(c: &mut Cursor<'_>, name: &'static str) -> Result<[f32; N], Error> {
    let mut v = [0f32; N];
    for x in &mut v {
        *x = c.f32(LE).field(name)?;
    }
    Ok(v)
}
/// An id and a class that must be `class`.
fn object(c: &mut Cursor<'_>, class: u32, name: &'static str) -> Result<u32, Error> {
    let id = word(c, name)?;
    expect(name, word(c, name)?, class)?;
    Ok(id)
}
/// A reference: tag 3 none, or 2 and an id.
fn reference(c: &mut Cursor<'_>, name: &'static str) -> Result<Option<u32>, Error> {
    match c.u8().field(name)? {
        3 => Ok(None),
        2 => Ok(Some(word(c, name)?)),
        found => Err(Error::Value {
            what: name,
            found: u64::from(found),
            expected: 2,
        }),
    }
}
/// A pointer tag: false for 3 (none), true for 0 (an inline object).
fn present(c: &mut Cursor<'_>, name: &'static str) -> Result<bool, Error> {
    match c.u8().field(name)? {
        3 => Ok(false),
        0 => Ok(true),
        found => Err(Error::Value {
            what: name,
            found: u64::from(found),
            expected: 0,
        }),
    }
}

fn constraint(c: &mut Cursor<'_>) -> Result<OrientationConstraint, Error> {
    expect("constraint tag", c.u8().field("constraint tag")?, 0u8)?;
    let id = object(c, ORIENTATION_CONSTRAINT, "orientation constraint")?;
    let bone = reference(c, "constraint bone")?;
    let unknown = c.u8().field("constraint byte")?;
    let count = c.count(LE, 18).field("constraint target count")?;
    let targets = (0..count)
        .map(|_| {
            expect(
                "constraint target tag",
                c.u8().field("constraint target tag")?,
                0u8,
            )?;
            let id = object(c, CONSTRAINT_TARGET, "constraint target")?;
            let bone = reference(c, "constraint target bone")?;
            Ok((id, bone, c.f32(LE).field("constraint target f32")?))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(OrientationConstraint {
        id,
        bone,
        unknown,
        targets,
        flag: c.u8().field("constraint flag")?,
        rotation: floats(c, "constraint rotation")?,
    })
}

fn bone(c: &mut Cursor<'_>, before: &[Bone]) -> Result<Bone, Error> {
    expect("bone tag", c.u8().field("bone tag")?, 0u8)?;
    expect("bone managed byte", c.u8().field("bone managed byte")?, 1u8)?;
    let id = object(c, BONE, "bone")?;
    let name = word(c, "bone name")?;
    let parent = reference(c, "bone parent")?;
    if let Some(p) = parent
        && !before.iter().any(|b| b.id == p)
    {
        return Err(Error::Value {
            what: "bone parent, a bone read before",
            found: u64::from(p),
            expected: 0,
        });
    }
    expect("bind pose tag", c.u8().field("bind pose tag")?, 0u8)?;
    let transforms_id = object(c, BONE_INITIAL_TRANSFORMS, "bind pose")?;
    let global_position = floats(c, "global position")?;
    let global_rotation = floats(c, "global rotation")?;
    let local_position = floats(c, "local position")?;
    let local_rotation = floats(c, "local rotation")?;
    let count = c.count(LE, 1).field("orientation constraint count")?;
    let constraints = (0..count)
        .map(|_| constraint(c))
        .collect::<Result<Vec<_>, Error>>()?;
    let index = c.u8().field("bone index")?;
    expect("bone index", index, n64(before.len()))?;
    Ok(Bone {
        id,
        name,
        parent,
        transforms_id,
        global_position,
        global_rotation,
        local_position,
        local_rotation,
        constraints,
        index,
        below: c.u8().field("bones below")?,
    })
}

fn pose_group(c: &mut Cursor<'_>) -> Result<PoseGroup, Error> {
    let id = object(c, SKELETON_POSE_GROUP, "pose group")?;
    let count = c.count(LE, 16).field("pose count")?;
    let poses = (0..count)
        .map(|_| {
            let id = object(c, SKELETON_POSE, "pose")?;
            let unknown = word(c, "pose u32")?;
            let n = c.count(LE, 44).field("pose bone count")?;
            let bones = (0..n)
                .map(|_| {
                    let id = object(c, SKELETON_POSE_BONE, "pose bone")?;
                    Ok((
                        id,
                        word(c, "pose bone name")?,
                        floats(c, "pose bone position")?,
                        floats(c, "pose bone rotation")?,
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok(Pose { id, unknown, bones })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(PoseGroup { id, poses })
}

impl Skeleton {
    /// Reads a Skeleton object's body (the module documentation), refusing a fixed value that differs, a parent not
    /// read before its child, a bone index or count of bones below that disagrees with the list, a root that is not a
    /// bone, a constraint naming no bone of the skeleton, or bytes after the mirror data.
    pub fn parse(body: &[u8]) -> Result<Skeleton, Error> {
        let mut c = Cursor::new(body);
        expect("managed byte", c.u8().field("managed byte")?, 1u8)?;
        let id = object(&mut c, SKELETON, "skeleton")?;
        let kind = word(&mut c, "skeleton u32")?;
        let count = c.count(LE, 87).field("bone count")?;
        let mut bones: Vec<Bone> = Vec::with_capacity(count);
        for _ in 0..count {
            let b = bone(&mut c, &bones)?;
            bones.push(b);
        }
        for (i, b) in bones.iter().enumerate() {
            let below = bones
                .iter()
                .filter(|other| {
                    let mut p = other.parent;
                    while let Some(id) = p {
                        if id == b.id {
                            return true;
                        }
                        p = bones.iter().find(|x| x.id == id).and_then(|x| x.parent);
                    }
                    false
                })
                .count();
            expect("bones below", b.below, n64(below))?;
            let known = |r: Option<u32>| r.is_none_or(|id| bones.iter().any(|x| x.id == id));
            for k in &b.constraints {
                if !known(k.bone) || !k.targets.iter().all(|t| known(t.1)) {
                    return Err(Error::Value {
                        what: "constraint bone, a bone of the skeleton",
                        found: n64(i),
                        expected: 0,
                    });
                }
            }
        }
        let root = reference(&mut c, "root")?;
        if root.is_some_and(|r| !bones.iter().any(|b| b.id == r)) {
            return Err(Error::Value {
                what: "root, a bone of the skeleton",
                found: u64::from(root.unwrap_or(0)),
                expected: 0,
            });
        }
        let unknown_after_root = word(&mut c, "u32 after the root")?;
        let groups = c.count(LE, 12).field("pose group count")?;
        let pose_groups = (0..groups)
            .map(|_| pose_group(&mut c))
            .collect::<Result<Vec<_>, Error>>()?;
        let ik = if present(&mut c, "IK data pointer tag")? {
            let id = object(&mut c, IK_DATA, "IK data")?;
            let n = c.count(LE, 4).field("IK bone count")?;
            let names = (0..n)
                .map(|_| word(&mut c, "IK bone name"))
                .collect::<Result<Vec<_>, Error>>()?;
            Some((id, names))
        } else {
            None
        };
        let com = if present(&mut c, "COM data pointer tag")? {
            let id = object(&mut c, COM_DATA, "COM data")?;
            Some((id, [floats(&mut c, "COM data")?, floats(&mut c, "COM data")?]))
        } else {
            None
        };
        let unknown_before_mirrors = word(&mut c, "u32 before the mirror data")?;
        let mirror_id = object(&mut c, SKELETON_MIRROR_DATA, "mirror data")?;
        let n = c.count(LE, 18).field("bone mirror count")?;
        let mirrors = (0..n)
            .map(|_| {
                Ok(BoneMirror {
                    id: object(&mut c, BONE_MIRROR_DATA, "bone mirror")?,
                    first: word(&mut c, "bone mirror first")?,
                    second: word(&mut c, "bone mirror second")?,
                    flags: c.array::<2>().field("bone mirror bytes")?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if c.remaining() != 0 {
            return Err(Error::Trailing(c.remaining()));
        }
        Ok(Skeleton {
            id,
            kind,
            bones,
            root,
            unknown_after_root,
            pose_groups,
            ik,
            com,
            unknown_before_mirrors,
            mirror_id,
            mirrors,
        })
    }
}
