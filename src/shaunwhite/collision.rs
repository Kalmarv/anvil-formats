//! PC collision objects in their authored local frame (Z up). MeshShape stores explicit float4 vertices, u16
//! indices and one material-table index per triangle; its Havok MOPP is acceleration data, retained undecoded.
//! No Havok packfile/tagfile is embedded in this layout. The stored MOPP code version is 5; the SDK version is
//! not established. Field order is AnvilToolkit 1.3.7's AC2 schema, with this game's vertices widened to float4.
//!
//! Surfaces are object ids, not destination surface enums. The referenced SWCollisionMaterial objects have their
//! authored names in the entry directory (ConcreteVert, ConcreteNonRidable, WoodDeck, ...). Their game-specific
//! physics parameters are not decoded here. Preserve the ids/names when building the consumer's surface table.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use primase_foundations::bytes::{Cursor, Endian, Field, FieldError};

const LE: Endian = Endian::Little;
/// The class id of a MeshShape.
pub const MESH_SHAPE: u32 = 0xB22B_3E61;
/// The class id of a CapsuleShape.
pub const CAPSULE_SHAPE: u32 = 0xB859_9052;
/// The class id of a ListShape, a shape holding child shapes.
pub const LIST_SHAPE: u32 = 0x86EB_FD8D;
/// The class id of a BarrelShape, read as a transformed convex hull ([`Shape::Convex`]).
pub const BARREL_SHAPE: u32 = 0x97CD_8890;
/// The class id of a BoxShape.
pub const BOX_SHAPE: u32 = 0x4EC6_8E98;
/// The class id of a SphereShape.
pub const SPHERE_SHAPE: u32 = 0xFA3F_7A18;
/// The class id of a MultiMeshShape.
pub const MULTI_MESH_SHAPE: u32 = 0x9A50_3477;
/// The class id of a MultiInertComponent.
pub const MULTI_INERT_COMPONENT: u32 = 0x8BBE_EE57;
/// The class id of an SWCollisionMaterial, the object a shape's `surface_id` names.
pub const SURFACE_MATERIAL: u32 = 0xEF0C_7E58;
/// The class id of an InertComponent.
pub const INERT_COMPONENT: u32 = 0x0E5A_450A;

/// Why a collision object can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The object ends before a field is complete.
    #[error(transparent)]
    Field(#[from] FieldError),
    /// A field has a value the format does not allow (a wrong class id, a non-finite float, an index past its
    /// list, a nesting deeper than 64, or trailing bytes); `field` names it.
    #[error("collision {field}: {why}")]
    Invalid {
        /// The field that was refused.
        field: &'static str,
        /// What is wrong with it.
        why: String,
    },
}

fn invalid(field: &'static str, why: impl Into<String>) -> Error {
    Error::Invalid {
        field,
        why: why.into(),
    }
}
fn word(c: &mut Cursor<'_>, field: &'static str) -> Result<u32, Error> {
    Ok(c.u32(LE).field(field)?)
}
fn number(c: &mut Cursor<'_>, field: &'static str) -> Result<f32, Error> {
    let value = c.f32(LE).field(field)?;
    if !value.is_finite() {
        return Err(invalid(field, "non-finite float"));
    }
    Ok(value)
}
fn floats<const N: usize>(c: &mut Cursor<'_>, field: &'static str) -> Result<[f32; N], Error> {
    let mut values = [0.; N];
    for value in &mut values {
        *value = number(c, field)?;
    }
    Ok(values)
}
fn expected(field: &'static str, value: u32, want: u32) -> Result<(), Error> {
    if value != want {
        return Err(invalid(field, format!("{value:#x}, expected {want:#x}")));
    }
    Ok(())
}
fn finish(c: &Cursor<'_>) -> Result<(), Error> {
    if c.remaining() != 0 {
        return Err(invalid("object end", format!("{} trailing bytes", c.remaining())));
    }
    Ok(())
}
fn blob<'a>(c: &mut Cursor<'a>, field: &'static str) -> Result<&'a [u8], Error> {
    let n = c.count(LE, 1).field(field)?;
    Ok(c.take(n).field(field)?)
}

/// The stored collision filter; layer and part are kept as source enum values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    /// The Filter object's id.
    pub id: u32,
    /// The collision layer, as the source enum value.
    pub layer: u32,
    /// The collision part, as the source enum value.
    pub part: u32,
}
impl Filter {
    fn read(c: &mut Cursor<'_>) -> Result<Self, Error> {
        let id = word(c, "filter id")?;
        expected("filter class", word(c, "filter class")?, 0x43EF_99C2)?;
        Ok(Self {
            id,
            layer: word(c, "filter layer")?,
            part: word(c, "filter part")?,
        })
    }
}

/// One explicit collision triangle with its source material object's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triangle {
    /// The three vertex indices into [`MeshShape::vertices`].
    pub indices: [u32; 3],
    /// The object id of the triangle's SWCollisionMaterial.
    pub surface_id: u32,
}

/// A mesh collision shape, in local source coordinates. Vertices remain float4 (w=0); AABBs retain their stored w.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshShape<'a> {
    /// The shape object's id.
    pub id: u32,
    /// The vertices (x, y, z, w) in the shape's local frame; w is 0.
    pub vertices: Vec<[f32; 4]>,
    /// The triangles, in stored order.
    pub triangles: Vec<Triangle>,
    /// The object ids of the SWCollisionMaterials the triangles' material indices select, in stored order.
    pub materials: Vec<u32>,
    /// One byte per triangle: its position in `materials`, as stored.
    pub material_indices: &'a [u8],
    /// The Havok MOPP acceleration bytes, undecoded.
    pub mopp: &'a [u8],
    /// The four f32 stored after the MOPP bytes: its offset and scale.
    pub mopp_offset_and_scale: [f32; 4],
    /// The shape's collision filter.
    pub filter: Filter,
    /// The stored MOPP code version; 5 is the only value read.
    pub mopp_version: u32,
    /// The shape's local bounding box: minimum then maximum, each four f32; the fourth lane is as stored.
    pub bounds: [[f32; 4]; 2],
}

impl<'a> MeshShape<'a> {
    /// A complete managed MeshShape body (without its directory envelope), checked to its exact end.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        match Shape::parse(bytes)? {
            Shape::Mesh(mesh) => Ok(mesh),
            _ => Err(invalid("shape class", "not a MeshShape")),
        }
    }
    fn read(c: &mut Cursor<'a>, id: u32) -> Result<Self, Error> {
        let count = c.count(LE, 16).field("vertex count")?;
        let mut vertices = Vec::with_capacity(count);
        for _ in 0..count {
            let v: [f32; 4] = floats(c, "vertex")?;
            if v.get(3) != Some(&0.) {
                return Err(invalid("vertex w", "expected zero"));
            }
            vertices.push(v);
        }
        let mopp = blob(c, "MOPP bytes")?;
        let mopp_offset_and_scale = floats(c, "MOPP offset and scale")?;
        let count = c.count(LE, 2).field("triangle index count")?;
        if !count.is_multiple_of(3) {
            return Err(invalid("triangle index count", "not a multiple of three"));
        }
        let mut indices = Vec::with_capacity(count);
        for _ in 0..count {
            let i = c.u16(LE).field("triangle index")?;
            if usize::from(i) >= vertices.len() {
                return Err(invalid(
                    "triangle index",
                    format!("{i} past {} vertices", vertices.len()),
                ));
            }
            indices.push(u32::from(i));
        }
        let material_indices = blob(c, "triangle material indices")?;
        if material_indices.len() != count.checked_div(3).unwrap_or(0) {
            return Err(invalid("triangle material indices", "need one per triangle"));
        }
        let count = c.count(LE, 4).field("material count")?;
        let mut materials = Vec::with_capacity(count);
        for _ in 0..count {
            materials.push(word(c, "material object id")?);
        }
        let triangles = indices
            .as_chunks::<3>()
            .0
            .iter()
            .zip(material_indices)
            .map(|(indices, &m)| {
                let surface_id = materials.get(usize::from(m)).copied().ok_or_else(|| {
                    invalid(
                        "triangle material index",
                        format!("{m} past {} materials", materials.len()),
                    )
                })?;
                Ok(Triangle {
                    indices: *indices,
                    surface_id,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let filter = Filter::read(c)?;
        let mopp_version = word(c, "MOPP version")?;
        expected("MOPP version", mopp_version, 5)?;
        let bounds = [floats(c, "local AABB min")?, floats(c, "local AABB max")?];
        for (&lo, &hi) in bounds
            .first()
            .into_iter()
            .flatten()
            .take(3)
            .zip(bounds.get(1).into_iter().flatten().take(3))
        {
            if lo > hi {
                return Err(invalid("local AABB", "min exceeds max"));
            }
        }
        Ok(Self {
            id,
            vertices,
            triangles,
            materials,
            material_indices,
            mopp,
            mopp_offset_and_scale,
            filter,
            mopp_version,
            bounds,
        })
    }
}

/// A list's child pointer, either inline, unresolved by id, or explicitly null. External/link/local-link tags are
/// retained (1, 2, 5); a consumer resolves the referenced object in its chosen archive order.
#[derive(Clone, Debug, PartialEq)]
pub enum Child<'a> {
    /// A shape stored in place.
    Inline(Box<Shape<'a>>),
    /// A shape named by object id; the consumer resolves it. `tag` (1, 2 or 5) is the pointer tag as stored.
    Reference {
        /// The pointer tag as stored: 1, 2 or 5.
        tag: u8,
        /// The referenced shape's object id.
        id: u32,
    },
    /// No child (pointer tag 3).
    Null,
}

/// Authored shape data; no tessellation, hull construction or choice of destination surfaces happens here.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape<'a> {
    /// A MeshShape.
    Mesh(MeshShape<'a>),
    /// The contained entity/shape arrays and MOPP as authored. Pairing is retained by position; the consumer
    /// resolves the contained entities' own transforms, not the containing MultiInert entity's transform.
    MultiMesh {
        /// The shape object's id.
        id: u32,
        /// The ids of the contained entities, in stored order.
        entity_ids: Vec<u32>,
        /// The ids of the contained shapes, one per entity and in the same order.
        shape_ids: Vec<u32>,
        /// The Havok MOPP acceleration bytes, undecoded.
        mopp: &'a [u8],
        /// The four f32 stored after the MOPP bytes.
        offset_and_scale: [f32; 4],
    },
    /// A CapsuleShape: the segment from `bottom` to `top` swept by `radius`.
    Capsule {
        /// The shape object's id.
        id: u32,
        /// One end of the capsule's segment (x, y, z, w), as stored.
        bottom: [f32; 4],
        /// The other end of the segment (x, y, z, w), as stored.
        top: [f32; 4],
        /// The radius, above zero.
        radius: f32,
        /// The object id of the SWCollisionMaterial.
        surface_id: u32,
    },
    /// A ListShape: child shapes, inline or by reference.
    List {
        /// The shape object's id.
        id: u32,
        /// The children, in stored order.
        children: Vec<Child<'a>>,
        /// One trailing byte in this game's list objects, absent from the AC2 schema's field list.
        flag: u8,
    },
    /// A BoxShape.
    Box {
        /// The shape object's id.
        id: u32,
        /// The half extents along the box's own axes (x, y, z, w), as stored.
        half_extents: [f32; 4],
        /// The box's transform, 16 f32 as stored.
        matrix: [f32; 16],
        /// The object id of the SWCollisionMaterial.
        surface_id: u32,
    },
    /// A SphereShape.
    Sphere {
        /// The shape object's id.
        id: u32,
        /// The sphere's centre (x, y, z, w), as stored.
        center: [f32; 4],
        /// The radius, above zero.
        radius: f32,
        /// The object id of the SWCollisionMaterial.
        surface_id: u32,
    },
    /// BarrelShape is authored as a transformed convex hull, not an inferred mathematical cylinder.
    Convex {
        /// The shape object's id.
        id: u32,
        /// The hull's transform, 16 f32 as stored.
        matrix: [f32; 16],
        /// The hull's planes, four f32 each, as stored.
        planes: Vec<[f32; 4]>,
        /// The hull's vertices (x, y, z, w), as stored.
        vertices: Vec<[f32; 4]>,
        /// The object id of the SWCollisionMaterial.
        surface_id: u32,
    },
}

impl<'a> Shape<'a> {
    /// Read one complete managed collision object. Unsupported classes, pointer tags, non-finite coordinates,
    /// invalid indices/counts, nested depth over 64 and bytes left over are named errors.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut c = Cursor::new(bytes);
        let shape = Self::read(&mut c, 0)?;
        finish(&c)?;
        Ok(shape)
    }
    fn read(c: &mut Cursor<'a>, depth: usize) -> Result<Self, Error> {
        if depth > 64 {
            return Err(invalid("shape depth", "exceeds 64"));
        }
        let managed = c.u8().field("managed flag")?;
        if managed > 1 {
            return Err(invalid("managed flag", managed.to_string()));
        }
        let id = word(c, "shape id")?;
        let class = word(c, "shape class")?;
        match class {
            MESH_SHAPE => Ok(Self::Mesh(MeshShape::read(c, id)?)),
            MULTI_MESH_SHAPE => {
                let count = c.count(LE, 5).field("contained entity count")?;
                let mut entity_ids = Vec::with_capacity(count);
                for _ in 0..count {
                    expected(
                        "entity handle tag",
                        u32::from(c.u8().field("entity handle tag")?),
                        0,
                    )?;
                    entity_ids.push(word(c, "contained entity")?);
                }
                let count = c.count(LE, 4).field("contained shape count")?;
                if count != entity_ids.len() {
                    return Err(invalid("contained shape count", "differs from entity count"));
                }
                let mut shape_ids = Vec::with_capacity(count);
                for _ in 0..count {
                    shape_ids.push(word(c, "contained shape")?);
                }
                Ok(Self::MultiMesh {
                    id,
                    entity_ids,
                    shape_ids,
                    mopp: blob(c, "multi MOPP bytes")?,
                    offset_and_scale: floats(c, "multi MOPP offset and scale")?,
                })
            }
            CAPSULE_SHAPE => {
                let bottom = floats(c, "capsule bottom")?;
                let top = floats(c, "capsule top")?;
                let radius = number(c, "capsule radius")?;
                if radius <= 0. {
                    return Err(invalid("capsule radius", "not positive"));
                }
                Ok(Self::Capsule {
                    id,
                    bottom,
                    top,
                    radius,
                    surface_id: word(c, "capsule material")?,
                })
            }
            LIST_SHAPE => {
                let count = c.count(LE, 1).field("list child count")?;
                let mut children = Vec::with_capacity(count);
                for _ in 0..count {
                    let tag = c.u8().field("child pointer")?;
                    children.push(match tag {
                        0 | 4 => Child::Inline(Box::new(Self::read(c, depth.saturating_add(1))?)),
                        1 | 2 | 5 => Child::Reference {
                            tag,
                            id: word(c, "child reference")?,
                        },
                        3 => Child::Null,
                        _ => return Err(invalid("child pointer", format!("unsupported tag {tag}"))),
                    });
                }
                Ok(Self::List {
                    id,
                    children,
                    flag: c.u8().field("list flag")?,
                })
            }
            BOX_SHAPE => Ok(Self::Box {
                id,
                half_extents: floats(c, "box half extents")?,
                matrix: floats(c, "box matrix")?,
                surface_id: word(c, "box material")?,
            }),
            SPHERE_SHAPE => {
                let center = floats(c, "sphere center")?;
                let radius = number(c, "sphere radius")?;
                if radius <= 0. {
                    return Err(invalid("sphere radius", "not positive"));
                }
                Ok(Self::Sphere {
                    id,
                    center,
                    radius,
                    surface_id: word(c, "sphere material")?,
                })
            }
            BARREL_SHAPE => {
                let matrix = floats(c, "convex matrix")?;
                let count = c.count(LE, 16).field("plane count")?;
                let mut planes = Vec::with_capacity(count);
                for _ in 0..count {
                    planes.push(floats(c, "convex plane")?);
                }
                let count = c.count(LE, 16).field("hull vertex count")?;
                let mut vertices = Vec::with_capacity(count);
                for _ in 0..count {
                    vertices.push(floats(c, "convex vertex")?);
                }
                Ok(Self::Convex {
                    id,
                    matrix,
                    planes,
                    vertices,
                    surface_id: word(c, "convex material")?,
                })
            }
            _ => Err(invalid("shape class", format!("unsupported {class:#010x}"))),
        }
    }
}

/// A MultiInertComponent's own shape container. Its entity handles and bare shape ids stay unresolved.
#[derive(Clone, Debug, PartialEq)]
pub struct MultiInertComponent<'a> {
    /// The leading managed flag byte: 0 or 1.
    pub managed: u8,
    /// The component object's id.
    pub id: u32,
    /// The id of the embedded resource reference.
    pub resource_id: u32,
    /// The resource reference's kind byte, as stored.
    pub resource_kind: u8,
    /// The resource reference's target id.
    pub resource_target: u32,
    /// The component's shape, which must be a MultiMeshShape.
    pub shape: Shape<'a>,
}
impl<'a> MultiInertComponent<'a> {
    /// Reads a whole MultiInertComponent; refuses a wrong class id, a shape that is not a MultiMeshShape and
    /// trailing bytes.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let (value, used) = Self::parse_prefix(bytes)?;
        if used != bytes.len() {
            return Err(invalid("object end", "trailing bytes after MultiInertComponent"));
        }
        Ok(value)
    }
    /// One inline component at the start of an Entity's remaining bytes, and its consumed length. The component's
    /// counted arrays delimit it; subsequent Entity components are not part of this object.
    pub fn parse_prefix(bytes: &'a [u8]) -> Result<(Self, usize), Error> {
        let mut c = Cursor::new(bytes);
        let managed = c.u8().field("managed flag")?;
        if managed > 1 {
            return Err(invalid("managed flag", managed.to_string()));
        }
        let id = word(&mut c, "multi inert id")?;
        expected(
            "component class",
            word(&mut c, "component class")?,
            MULTI_INERT_COMPONENT,
        )?;
        let resource_id = word(&mut c, "resource reference id")?;
        expected(
            "resource reference class",
            word(&mut c, "resource reference class")?,
            0x3C6C_77C8,
        )?;
        let resource_kind = c.u8().field("resource kind")?;
        let resource_target = word(&mut c, "resource target")?;
        let shape = Shape::read(&mut c, 0)?;
        if !matches!(shape, Shape::MultiMesh { .. }) {
            return Err(invalid("multi inert shape", "not a MultiMeshShape"));
        }
        let used = bytes.len().saturating_sub(c.remaining());
        Ok((
            Self {
                managed,
                id,
                resource_id,
                resource_kind,
                resource_target,
                shape,
            },
            used,
        ))
    }
}

/// The fixed 150-byte InertComponent payload, including its rigid body's shape reference and source physics fields.
/// The entity's transform places the shape; the stored inertia tensor is not a placement matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct InertComponent {
    /// The managed byte is 0 for the measured inline inert; standalone objects commonly store 1.
    pub managed: u8,
    /// The component object's id.
    pub id: u32,
    /// The id of the embedded resource reference.
    pub resource_id: u32,
    /// The resource reference's kind byte, as stored.
    pub resource_kind: u8,
    /// The resource reference's target id.
    pub resource_target: u32,
    /// The `damage` word, as stored.
    pub damage: u32,
    /// The id of the embedded rigid body object.
    pub rigid_body_id: u32,
    /// The rigid body's collision filter.
    pub filter: Filter,
    /// The id of the shape the rigid body uses.
    pub shape_id: u32,
    /// The mass, finite.
    pub mass: f32,
    /// The linear then angular damping, each finite.
    pub damping: [f32; 2],
    /// The four rigid body flag bytes, as stored.
    pub flags: [u8; 4],
    /// The inertia tensor, 16 f32 as stored.
    pub inertia: [f32; 16],
    /// The `walkability` word, as stored.
    pub walkability: u32,
    /// The remaining 12 authored bytes, whose meaning is not established.
    pub tail: [u8; 12],
}
impl InertComponent {
    /// Reads a whole InertComponent; refuses a wrong class id, a non-finite float and trailing bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut c = Cursor::new(bytes);
        let managed = c.u8().field("managed flag")?;
        if managed > 1 {
            return Err(invalid("managed flag", managed.to_string()));
        }
        let id = word(&mut c, "inert id")?;
        expected(
            "component class",
            word(&mut c, "component class")?,
            INERT_COMPONENT,
        )?;
        let resource_id = word(&mut c, "resource reference id")?;
        expected(
            "resource reference class",
            word(&mut c, "resource reference class")?,
            0x3C6C_77C8,
        )?;
        let resource_kind = c.u8().field("resource kind")?;
        let resource_target = word(&mut c, "resource target")?;
        let damage = word(&mut c, "damage")?;
        let rigid_body_id = word(&mut c, "rigid body id")?;
        expected("rigid body class", word(&mut c, "rigid body class")?, 0x228F_402A)?;
        let filter = Filter::read(&mut c)?;
        let shape_id = word(&mut c, "rigid body shape")?;
        let mass = number(&mut c, "mass")?;
        let damping = [
            number(&mut c, "linear damping")?,
            number(&mut c, "angular damping")?,
        ];
        let flags = c.array().field("rigid body flags")?;
        let inertia = floats(&mut c, "inertia tensor")?;
        let walkability = word(&mut c, "walkability")?;
        let tail = c.array().field("inert tail")?;
        finish(&c)?;
        Ok(Self {
            managed,
            id,
            resource_id,
            resource_kind,
            resource_target,
            damage,
            rigid_body_id,
            filter,
            shape_id,
            mass,
            damping,
            flags,
            inertia,
            walkability,
            tail,
        })
    }
}
