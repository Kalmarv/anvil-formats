//! Shaun White Skateboarding (PC) Mesh objects: the mesh's bones, its inline CompiledMesh (vertex streams, 16-bit
//! indices and draw ranges) and its material list, read as stored.
//!
//! The body (little-endian, offsets in the body):
//! - 0: the managed byte (1); 1: the object's id; 5: its class ([`MESH`]); 9: the category (u32); 13: the generated
//!   flag (a byte); 14: the sub-mesh count (u32): 0 on the meshes read here (a mesh that also keeps raw SubMesh
//!   objects, 221 of the 16,149, stores them next and is refused, [`Error::SubMeshes`]); 18: the bone count `n` (u32);
//! - 22: `n` MeshBones of 76 bytes: an id, the class [`MESH_BONE`], a u32 kept as stored (the bone's name hash in the
//!   meshes examined) and a 4 x 4 f32 matrix;
//! - 22 + 76`n`: eleven bytes kept as stored; then the class [`COMPILED_MESH`] and the compiled data's length `L`
//!   (u32 each), and the compiled data ([`CompiledMesh`]);
//! - after it: the platform version (16) and SDK version (32), the material count `m` and `m` material object ids
//!   (u32 each). The body ends there.
//!
//! Every one of the 15,928 Mesh objects without raw sub-meshes of the 33 data forges of the PC patched build (forges of
//! 2010-10-27) reads so, to its exact end.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use std::ops::Range;

use primase_foundations::bytes::{Cursor, Field, FieldError};

use crate::fields::LE;

/// The class id of a Mesh (the CRC-32 of `Mesh`).
pub const MESH: u32 = 0x415D_9568;
/// The class id of a MeshBone (the CRC-32 of `MeshBone`).
pub const MESH_BONE: u32 = 0x9EF0_E7A1;
/// The class id of a CompiledMesh (the CRC-32 of `CompiledMesh`).
pub const COMPILED_MESH: u32 = 0xFC9E_1595;

/// Why a Mesh body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body ends before a field is complete.
    #[error("Mesh body: {0}")]
    Short(#[from] FieldError),
    /// A value that must be fixed or must agree with another is not what the layout gives.
    #[error("Mesh {what}: {found}, expected {expected}")]
    Value {
        /// The field refused.
        what: &'static str,
        /// The value found.
        found: u64,
        /// The value this reader accepts there.
        expected: u64,
    },
    /// The mesh keeps raw SubMesh objects (its sub-mesh count is not 0); their layout is not read.
    #[error("Mesh: {0} raw sub-meshes, whose layout is not read")]
    SubMeshes(u32),
    /// Bytes follow the material list.
    #[error("Mesh: {0} bytes after the material list")]
    Trailing(usize),
}

/// One MeshBone as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshBone {
    /// The MeshBone object's id.
    pub id: u32,
    /// The u32 after the class, as stored.
    pub name: u32,
    /// Sixteen f32, as stored.
    pub matrix: [f32; 16],
}

/// One draw range of the compiled data: six u32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawRange {
    /// The Direct3D primitive type (4, a triangle list, on every stored range).
    pub primitive: u32,
    /// The first vertex the range's indices reach.
    pub first_vertex: u32,
    /// How many vertices from `first_vertex` it reaches.
    pub vertex_count: u32,
    /// The first index, in indices from the start of the index bytes.
    pub first_index: u32,
    /// The number of triangles.
    pub triangles: u32,
    /// The sixth u32, as stored; its meaning is not established.
    pub unknown: u32,
}

/// A range's bone palette, from the compiled data's tail: the mesh bones its vertices' bone indices name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    /// A u16 kept as stored: the range's position in 25,302 of the 34,168 stored palettes, another value in the rest.
    pub unknown: u16,
    /// The range's vertex count, repeated (equal on every stored palette).
    pub vertex_count: u16,
    /// Indices into [`StoredMesh::bones`], one per palette entry (none on a mesh without bones).
    pub bones: Vec<u8>,
}

/// A vertex element's storage, as a Direct3D 9 declaration type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementType {
    /// Two f32 (`D3DDECLTYPE_FLOAT2`).
    Float2,
    /// Three f32 (`D3DDECLTYPE_FLOAT3`).
    Float3,
    /// Four bytes B, G, R, A read as 0..1 (`D3DDECLTYPE_D3DCOLOR`).
    D3dColor,
    /// Four unsigned bytes read as integers 0..255 (`D3DDECLTYPE_UBYTE4`).
    Ubyte4,
    /// Four unsigned bytes read as 0..1 (`D3DDECLTYPE_UBYTE4N`).
    Ubyte4N,
}

impl ElementType {
    /// The `D3DDECLTYPE` value.
    pub fn d3d(self) -> u8 {
        match self {
            ElementType::Float2 => 1,
            ElementType::Float3 => 2,
            ElementType::D3dColor => 4,
            ElementType::Ubyte4 => 5,
            ElementType::Ubyte4N => 8,
        }
    }
    /// Its size in bytes.
    pub fn size(self) -> u16 {
        match self {
            ElementType::Float2 => 8,
            ElementType::Float3 => 12,
            _ => 4,
        }
    }
}

/// What a vertex element is, as a Direct3D 9 declaration usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Usage {
    /// `D3DDECLUSAGE_POSITION`.
    Position,
    /// `D3DDECLUSAGE_NORMAL`.
    Normal,
    /// `D3DDECLUSAGE_TANGENT`.
    Tangent,
    /// `D3DDECLUSAGE_COLOR`.
    Color,
    /// `D3DDECLUSAGE_TEXCOORD`.
    TexCoord,
    /// `D3DDECLUSAGE_BLENDINDICES`.
    BlendIndices,
    /// `D3DDECLUSAGE_BLENDWEIGHT`.
    BlendWeight,
}

impl Usage {
    /// The `D3DDECLUSAGE` value.
    pub fn d3d(self) -> u8 {
        match self {
            Usage::Position => 0,
            Usage::BlendWeight => 1,
            Usage::BlendIndices => 2,
            Usage::Normal => 3,
            Usage::TexCoord => 5,
            Usage::Tangent => 6,
            Usage::Color => 10,
        }
    }
}

/// One element of a mesh's vertex layout: where it lies in the vertex, how it is stored and what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VertexElement {
    /// Its offset in the vertex.
    pub offset: u16,
    /// How it is stored.
    pub kind: ElementType,
    /// What it is.
    pub usage: Usage,
    /// The usage index (`TEXCOORD1` has 1).
    pub index: u8,
}

/// The compiled data of a mesh (all ranges are of the body).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledMesh {
    /// The u32 at 0 (16 on every stored mesh).
    pub version: u32,
    /// The number of vertex streams after the base one: whole vertex arrays of the same layout (morph targets).
    pub morph_count: u32,
    /// Bytes per vertex.
    pub stride: u32,
    /// The bytes of one vertex stream.
    pub vertex_bytes: u32,
    /// The bytes of the 16-bit indices.
    pub index_bytes: u32,
    /// The flags word that selects the vertex elements ([`CompiledMesh::layout`]).
    pub flags: u32,
    /// The base vertex stream, then each morph stream, each `vertex_bytes` long.
    pub streams: Vec<Range<usize>>,
    /// The indices.
    pub indices: Range<usize>,
    /// The first partition of the indices: one range per material, in material order.
    pub ranges: Vec<DrawRange>,
    /// The second partition of the same indices.
    pub second_ranges: Vec<DrawRange>,
    /// The bytes after the ranges to the end of the compiled data: 40 per range of [`CompiledMesh::ranges`],
    /// starting with the ranges' palettes ([`CompiledMesh::palettes`]).
    pub tail: Range<usize>,
}

impl CompiledMesh {
    /// The vertex layout of every stream: the elements, in vertex order, that the game's renderer declares for the
    /// mesh's vertices (stream 0 of its declaration). The flags word and the stride select them:
    /// - position, three f32 at 0; normal, `UBYTE4` at 12 (bytes 0..255, rescaled by the shaders);
    /// - a tangent (`UBYTE4`) when the stride holds it (see below);
    /// - with flag 64, the vertex colour as a Direct3D colour (B, G, R, A in memory), `COLOR0`;
    /// - the first UV, `TEXCOORD0`: two f32, or three where the stride holds four bytes more;
    /// - with flag 4 and no morph streams, a second UV, `TEXCOORD1` (two f32); morph meshes have none;
    /// - with flag 1 or flag 2, four bone indices (`UBYTE4`, `BLENDINDICES`) and four weights (`UBYTE4N`,
    ///   `BLENDWEIGHT`).
    ///
    /// The stride less the sizes of the other elements is 0 (no tangent), 4 (a tangent) or 8 (a tangent, and a
    /// three-f32 first UV); another remainder is refused. A further vertex stream of a draw (the per-instance colour,
    /// `TEXCOORD2`) is not part of the mesh.
    pub fn layout(&self) -> Result<Vec<VertexElement>, Error> {
        let colour = self.flags & 64 != 0;
        let second_uv = self.flags & 4 != 0 && self.morph_count == 0;
        let skin = self.flags & 3 != 0;
        let fixed = [(true, 24u32), (colour, 4), (second_uv, 8), (skin, 8)]
            .iter()
            .filter(|(on, _)| *on)
            .fold(0u32, |n, (_, size)| n.saturating_add(*size));
        let (tangent, uv3) = match self.stride.checked_sub(fixed) {
            Some(0) => (false, false),
            Some(4) => (true, false),
            Some(8) => (true, true),
            _ => {
                return Err(Error::Value {
                    what: "stride for the flags",
                    found: u64::from(self.stride),
                    expected: u64::from(fixed),
                });
            }
        };
        let mut out = Vec::new();
        let mut at = 0u16;
        let mut push = |kind: ElementType, usage: Usage, index: u8| {
            out.push(VertexElement {
                offset: at,
                kind,
                usage,
                index,
            });
            at = at.saturating_add(kind.size());
        };
        push(ElementType::Float3, Usage::Position, 0);
        push(ElementType::Ubyte4, Usage::Normal, 0);
        if tangent {
            push(ElementType::Ubyte4, Usage::Tangent, 0);
        }
        if colour {
            push(ElementType::D3dColor, Usage::Color, 0);
        }
        push(
            if uv3 {
                ElementType::Float3
            } else {
                ElementType::Float2
            },
            Usage::TexCoord,
            0,
        );
        if second_uv {
            push(ElementType::Float2, Usage::TexCoord, 1);
        }
        if skin {
            push(ElementType::Ubyte4, Usage::BlendIndices, 0);
            push(ElementType::Ubyte4N, Usage::BlendWeight, 0);
        }
        Ok(out)
    }
    /// The palettes at the start of the tail, one per range of [`CompiledMesh::ranges`], in order; `body` is the
    /// mesh body this was read from. Each is a byte 3, a byte 1, the entry count `n` (a byte, at most 32 on the stored
    /// meshes), a u16 kept as stored, the range's vertex count (u16) and `n` bone indices (a byte each); a mesh without
    /// bones has `n` 0. The rest of the tail (to 40 bytes per range) is kept as stored.
    pub fn palettes(&self, body: &[u8]) -> Result<Vec<Palette>, Error> {
        let tail = body.get(self.tail.clone()).ok_or(Error::Value {
            what: "tail",
            found: u64::try_from(self.tail.end).unwrap_or(u64::MAX),
            expected: u64::try_from(body.len()).unwrap_or(u64::MAX),
        })?;
        let mut c = Cursor::new(tail);
        let mut out = Vec::with_capacity(self.ranges.len());
        for r in &self.ranges {
            expect("palette marker", c.u8().field("palette marker")?, 3u8)?;
            expect("palette version", c.u8().field("palette version")?, 1u8)?;
            let n = c.u8().field("palette entry count")?;
            let unknown = c.u16(LE).field("palette u16")?;
            let vertex_count = c.u16(LE).field("palette vertex count")?;
            expect("palette vertex count", vertex_count, r.vertex_count)?;
            let bones = c.take(usize::from(n)).field("palette bones")?.to_vec();
            out.push(Palette {
                unknown,
                vertex_count,
                bones,
            });
        }
        Ok(out)
    }
}

/// A Mesh body as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredMesh {
    /// The Mesh object's id.
    pub id: u32,
    /// The category (u32), as stored.
    pub category: u32,
    /// The generated flag, as stored.
    pub generated: u8,
    /// The mesh's bones.
    pub bones: Vec<MeshBone>,
    /// The eleven bytes before the compiled data, as stored.
    pub unknown: [u8; 11],
    /// The compiled data.
    pub compiled: CompiledMesh,
    /// The material object ids, one per range of [`CompiledMesh::ranges`].
    pub materials: Vec<u32>,
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
fn range(c: &mut Cursor<'_>, n: usize, name: &'static str) -> Result<Range<usize>, Error> {
    let start = c.position();
    c.take(n).field(name)?;
    Ok(start..c.position())
}
fn usize_of(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

fn draw_ranges(c: &mut Cursor<'_>, n: u32) -> Result<Vec<DrawRange>, Error> {
    (0..n)
        .map(|_| {
            Ok(DrawRange {
                primitive: word(c, "range primitive")?,
                first_vertex: word(c, "range first vertex")?,
                vertex_count: word(c, "range vertex count")?,
                first_index: word(c, "range first index")?,
                triangles: word(c, "range triangle count")?,
                unknown: word(c, "range sixth word")?,
            })
        })
        .collect()
}

fn compiled(c: &mut Cursor<'_>, end: usize) -> Result<CompiledMesh, Error> {
    let version = word(c, "compiled version")?;
    let morph_count = word(c, "stream count")?;
    let stride = word(c, "stride")?;
    let vertex_bytes = word(c, "vertex bytes")?;
    let streams = u64::from(morph_count).saturating_add(1);
    expect(
        "stride x streams",
        word(c, "stride x streams")?,
        u64::from(stride).saturating_mul(streams),
    )?;
    expect(
        "vertex bytes x streams",
        word(c, "vertex bytes x streams")?,
        u64::from(vertex_bytes).saturating_mul(streams),
    )?;
    let index_bytes = word(c, "index bytes")?;
    expect("u32 at 28", word(c, "u32 at 28")?, 0u32)?;
    let flags = word(c, "flags")?;
    let first = word(c, "range count")?;
    let second = word(c, "second range count")?;
    expect(
        "buffer total",
        word(c, "buffer total")?,
        u64::from(vertex_bytes)
            .saturating_mul(streams)
            .saturating_add(u64::from(index_bytes))
            .saturating_add((u64::from(first).saturating_add(u64::from(second))).saturating_mul(24)),
    )?;
    let streams = (0..=morph_count)
        .map(|_| range(c, usize_of(vertex_bytes), "a vertex stream"))
        .collect::<Result<Vec<_>, _>>()?;
    let indices = range(c, usize_of(index_bytes), "indices")?;
    let ranges = draw_ranges(c, first)?;
    let second_ranges = draw_ranges(c, second)?;
    let at = c.position();
    if at > end {
        return Err(Error::Value {
            what: "compiled data length",
            found: u64::try_from(end).unwrap_or(u64::MAX),
            expected: u64::try_from(at).unwrap_or(u64::MAX),
        });
    }
    c.take_to(end).field("compiled tail")?;
    expect(
        "tail length",
        u64::try_from(end.saturating_sub(at)).unwrap_or(u64::MAX),
        u64::from(first).saturating_mul(40),
    )?;
    Ok(CompiledMesh {
        version,
        morph_count,
        stride,
        vertex_bytes,
        index_bytes,
        flags,
        streams,
        indices,
        ranges,
        second_ranges,
        tail: at..end,
    })
}

impl StoredMesh {
    /// Reads a Mesh object's body (the module documentation), refusing a fixed value that differs, sizes that
    /// disagree, or bytes after the material list.
    pub fn parse(body: &[u8]) -> Result<StoredMesh, Error> {
        let mut c = Cursor::new(body);
        expect("managed byte", c.u8().field("managed byte")?, 1u8)?;
        let id = word(&mut c, "id")?;
        expect("class", word(&mut c, "class")?, MESH)?;
        let category = word(&mut c, "category")?;
        let generated = c.u8().field("generated")?;
        let submesh_count = word(&mut c, "sub-mesh count")?;
        if submesh_count != 0 {
            return Err(Error::SubMeshes(submesh_count));
        }
        let bone_count = c.count(LE, 76).field("bone count")?;
        let mut bones = Vec::with_capacity(bone_count);
        for _ in 0..bone_count {
            let id = word(&mut c, "bone id")?;
            expect("bone class", word(&mut c, "bone class")?, MESH_BONE)?;
            let name = word(&mut c, "bone name")?;
            let mut matrix = [0f32; 16];
            for v in &mut matrix {
                *v = c.f32(LE).field("bone matrix")?;
            }
            bones.push(MeshBone { id, name, matrix });
        }
        let unknown = c.array::<11>().field("bytes before the compiled data")?;
        expect("compiled class", word(&mut c, "compiled class")?, COMPILED_MESH)?;
        let length = c.count(LE, 1).field("compiled data length")?;
        let end = c.position().saturating_add(length);
        let compiled = compiled(&mut c, end)?;
        expect("platform version", word(&mut c, "platform version")?, 16u32)?;
        expect("SDK version", word(&mut c, "SDK version")?, 32u32)?;
        let count = c.count(LE, 4).field("material count")?;
        let materials = (0..count)
            .map(|_| word(&mut c, "material id"))
            .collect::<Result<Vec<_>, _>>()?;
        if c.remaining() != 0 {
            return Err(Error::Trailing(c.remaining()));
        }
        Ok(StoredMesh {
            id,
            category,
            generated,
            bones,
            unknown,
            compiled,
            materials,
        })
    }
}
