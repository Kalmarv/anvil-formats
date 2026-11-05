//! Shaun White Skateboarding (PC) Mesh objects: the mesh's raw SubMeshes, its bones, its inline CompiledMesh (vertex
//! streams, 16-bit indices and draw ranges) and its material list, read as stored.
//!
//! The body (little-endian, offsets in the body):
//! - 0: the managed byte (1); 1: the object's id; 5: its class ([`MESH`]); 9: the category (u32); 13: the generated
//!   flag (a byte); 14: the SubMesh count `s` (u32); 18: `s` SubMeshes ([`SubMesh`]), 0 on most meshes; then the bone
//!   count `n` (u32);
//! - `n` MeshBones of 76 bytes: an id, the class [`MESH_BONE`], a u32 kept as stored (the bone's name hash in the
//!   meshes examined) and a 4 x 4 f32 matrix;
//! - eleven bytes kept as stored; then the class [`COMPILED_MESH`] and the compiled data's length `L` (u32 each), and
//!   the compiled data ([`CompiledMesh`]);
//! - after it: the platform version (16) and SDK version (32), the material count `m` and `m` material object ids
//!   (u32 each). The body ends there.
//!
//! Every one of the 16,149 Mesh objects of the 33 data forges of the PC patched build (forges of 2010-10-27) reads
//! so, to its exact end; 221 of them keep SubMeshes, and every mesh with morph streams is among those 221.
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
/// The class id of a SubMesh (the CRC-32 of `SubMesh`).
pub const SUB_MESH: u32 = 0x5755_DE7F;
/// The class id of a UVChannel (the CRC-32 of `UVChannel`).
pub const UV_CHANNEL: u32 = 0xBDAD_8273;
/// The class id of a MorphChannel (the CRC-32 of `MorphChannel`).
pub const MORPH_CHANNEL: u32 = 0xE70C_5754;

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
    /// A SubMesh's copy holds a copy in turn, deeper than [`MAX_SUB_MESH_DEPTH`].
    #[error("Mesh: SubMesh copies nested deeper than {MAX_SUB_MESH_DEPTH}")]
    Depth,
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

/// How deep [`SubMesh::copy`] may nest: one level on every stored mesh (a SubMesh's copy has none).
pub const MAX_SUB_MESH_DEPTH: usize = 4;

/// One raw SubMesh: one part of the mesh as authored (one material's triangles), before it is compiled. Every range
/// is of the mesh body; `n` is the vertex count (the positions' count) and `t` the triangle count.
///
/// Each SubMesh is one range of [`CompiledMesh::ranges`] (the one using [`SubMesh::material`]): the same vertex
/// positions bit for bit and the same triangles, in another vertex and triangle order.
#[derive(Clone, Debug, PartialEq)]
pub struct SubMesh {
    /// The SubMesh object's id.
    pub id: u32,
    /// The u32 after the class (0 on every stored SubMesh).
    pub unknown_head: u32,
    /// The triangles: `3t` u16 vertex indices, a triangle list.
    pub indices: Range<usize>,
    /// `n` u32, or none: for each vertex, the index of the authored vertex it was split from (vertices with equal
    /// values have equal positions). The copy ([`SubMesh::copy`]) has none.
    pub source_vertices: Range<usize>,
    /// The u32 after them (0 on every stored SubMesh).
    pub unknown_faces: u32,
    /// `t` u32, one per triangle, kept as stored (bit masks such as 1, 2, 4, 8).
    pub face_words: Range<usize>,
    /// `t` bytes, one per triangle, kept as stored (3 on most).
    pub face_bytes: Range<usize>,
    /// `n` positions, three f32 each.
    pub positions: Range<usize>,
    /// `n` normals, three f32 each.
    pub normals: Range<usize>,
    /// `n` tangents, three f32 each (unit vectors perpendicular to the normals), or none: present on every copy
    /// ([`SubMesh::copy`]) and on no original.
    pub tangents: Range<usize>,
    /// `n` binormals, three f32 each (unit vectors perpendicular to the normals), or none, as the tangents.
    pub binormals: Range<usize>,
    /// `n` vertex colours (Direct3D colours: B, G, R, A), or none: the compiled vertices' colour where the mesh has one.
    pub colours: Range<usize>,
    /// Three further runs of `n` u32 each, or none, kept as stored: the second and third are rarely present, the last
    /// is on every SubMesh.
    pub vertex_words: [Range<usize>; 3],
    /// The UV channels, in order; the first is the compiled vertices' first UV.
    pub uv_channels: Vec<UvChannel>,
    /// The u32 before the morph channels (0 on every stored SubMesh).
    pub unknown_morph: u32,
    /// The morph channels: one per morph stream of the compiled data that this SubMesh moves.
    pub morphs: Vec<MorphChannel>,
    /// The byte after the morph channels: 1 when there are any, else 0.
    pub has_morphs: u8,
    /// The material object id of this SubMesh: the id [`StoredMesh::materials`] gives for its range.
    pub material: u32,
    /// The two bytes after the material (1 and 1 on every stored SubMesh).
    pub unknown_flags: [u8; 2],
    /// A second SubMesh owned by this one: on every stored mesh, a copy with the same indices, positions and material,
    /// with tangents and binormals, no source vertices and no morph channels; it holds none in turn.
    pub copy: Option<Box<SubMesh>>,
    /// The u32 at the end, kept as stored (0 on most).
    pub unknown_tail: u32,
}

/// One UV channel of a [`SubMesh`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UvChannel {
    /// The u32 before the class (0 on every stored channel).
    pub id: u32,
    /// The u32 after the class (0 on every stored channel).
    pub kind: u32,
    /// The byte after it, kept as stored.
    pub unknown: u8,
    /// `n` UVs, two f32 each.
    pub uvs: Range<usize>,
    /// `n` u32, or none, kept as stored (present where the SubMesh has source vertices).
    pub source: Range<usize>,
}

/// One morph channel of a [`SubMesh`]: the whole SubMesh's vertices in one morph target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphChannel {
    /// The MorphChannel object's id.
    pub id: u32,
    /// The u32 after the class, kept as stored.
    pub name: u32,
    /// `n` positions, three f32 each, absolute (not offsets from [`SubMesh::positions`]).
    pub positions: Range<usize>,
    /// `n` normals, three f32 each.
    pub normals: Range<usize>,
    /// `n` UVs, two f32 each.
    pub uvs: Range<usize>,
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
    /// The raw SubMeshes (none on most meshes).
    pub sub_meshes: Vec<SubMesh>,
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
fn n64(v: usize) -> u64 {
    u64::try_from(v).unwrap_or(u64::MAX)
}
fn usize_of(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// A counted run of `each`-byte elements: the count (u32), then the elements.
fn run(c: &mut Cursor<'_>, each: usize, name: &'static str) -> Result<(usize, Range<usize>), Error> {
    let n = c.count(LE, each).field(name)?;
    Ok((n, range(c, n.saturating_mul(each), name)?))
}
/// A run that holds `n` elements, or none where `optional`.
fn run_of(
    c: &mut Cursor<'_>,
    each: usize,
    n: usize,
    optional: bool,
    name: &'static str,
) -> Result<Range<usize>, Error> {
    let (found, r) = run(c, each, name)?;
    if found == n || (optional && found == 0) {
        Ok(r)
    } else {
        Err(Error::Value {
            what: name,
            found: n64(found),
            expected: n64(n),
        })
    }
}
/// A count that is 0 on every stored SubMesh, whose elements' size is therefore not known.
fn empty(c: &mut Cursor<'_>, name: &'static str) -> Result<(), Error> {
    expect(name, word(c, name)?, 0u32)
}

fn sub_mesh(c: &mut Cursor<'_>, depth: usize) -> Result<SubMesh, Error> {
    if depth > MAX_SUB_MESH_DEPTH {
        return Err(Error::Depth);
    }
    expect("SubMesh managed byte", c.u8().field("SubMesh managed byte")?, 1u8)?;
    let id = word(c, "SubMesh id")?;
    expect("SubMesh class", word(c, "SubMesh class")?, SUB_MESH)?;
    let unknown_head = word(c, "SubMesh u32 after the class")?;
    let (index_count, indices) = run(c, 2, "SubMesh indices")?;
    if !index_count.is_multiple_of(3) {
        return Err(Error::Value {
            what: "SubMesh index count, a multiple of 3",
            found: n64(index_count),
            expected: n64(index_count.div_euclid(3).saturating_mul(3)),
        });
    }
    let triangles = index_count.div_euclid(3);
    let (sources, source_vertices) = run(c, 4, "SubMesh source vertices")?;
    let unknown_faces = word(c, "SubMesh u32 before the faces")?;
    let face_words = run_of(c, 4, triangles, false, "SubMesh face words")?;
    let face_bytes = run_of(c, 1, triangles, false, "SubMesh face bytes")?;
    let (n, positions) = run(c, 12, "SubMesh positions")?;
    if sources != 0 && sources != n {
        return Err(Error::Value {
            what: "SubMesh source vertices",
            found: n64(sources),
            expected: n64(n),
        });
    }
    let normals = run_of(c, 12, n, false, "SubMesh normals")?;
    let tangents = run_of(c, 12, n, true, "SubMesh tangents")?;
    let binormals = run_of(c, 12, n, true, "SubMesh binormals")?;
    let colours = run_of(c, 4, n, true, "SubMesh colours")?;
    let second = run_of(c, 4, n, true, "SubMesh second vertex words")?;
    let third = run_of(c, 4, n, true, "SubMesh third vertex words")?;
    empty(c, "SubMesh first empty run")?;
    let last = run_of(c, 4, n, true, "SubMesh last vertex words")?;
    empty(c, "SubMesh second empty run")?;
    empty(c, "SubMesh third empty run")?;
    let channels = c.count(LE, 21).field("UV channel count")?;
    let uv_channels = (0..channels)
        .map(|_| {
            let id = word(c, "UV channel id")?;
            expect("UV channel class", word(c, "UV channel class")?, UV_CHANNEL)?;
            let kind = word(c, "UV channel u32 after the class")?;
            let unknown = c.u8().field("UV channel byte")?;
            let uvs = run_of(c, 8, n, false, "UV channel UVs")?;
            let source = run_of(c, 4, n, true, "UV channel source words")?;
            Ok(UvChannel {
                id,
                kind,
                unknown,
                uvs,
                source,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let unknown_morph = word(c, "SubMesh u32 before the morph channels")?;
    let count = c.count(LE, 24).field("morph channel count")?;
    let morphs = (0..count)
        .map(|_| {
            let id = word(c, "morph channel id")?;
            expect(
                "morph channel class",
                word(c, "morph channel class")?,
                MORPH_CHANNEL,
            )?;
            let name = word(c, "morph channel u32 after the class")?;
            Ok(MorphChannel {
                id,
                name,
                positions: run_of(c, 12, n, false, "morph channel positions")?,
                normals: run_of(c, 12, n, false, "morph channel normals")?,
                uvs: run_of(c, 8, n, false, "morph channel UVs")?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let has_morphs = c.u8().field("SubMesh morph flag")?;
    let material = word(c, "SubMesh material")?;
    let unknown_flags = c.array::<2>().field("SubMesh bytes after the material")?;
    let copy = match c.u8().field("SubMesh copy pointer tag")? {
        0 => Some(Box::new(sub_mesh(c, depth.saturating_add(1))?)),
        3 => None,
        found => {
            return Err(Error::Value {
                what: "SubMesh copy pointer tag (0 inline, 3 none)",
                found: u64::from(found),
                expected: 3,
            });
        }
    };
    let unknown_tail = word(c, "SubMesh last u32")?;
    Ok(SubMesh {
        id,
        unknown_head,
        indices,
        source_vertices,
        unknown_faces,
        face_words,
        face_bytes,
        positions,
        normals,
        tangents,
        binormals,
        colours,
        vertex_words: [second, third, last],
        uv_channels,
        unknown_morph,
        morphs,
        has_morphs,
        material,
        unknown_flags,
        copy,
        unknown_tail,
    })
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
        let count = c.count(LE, 1).field("SubMesh count")?;
        let sub_meshes = (0..count)
            .map(|_| {
                expect(
                    "SubMesh pointer tag (0 inline)",
                    c.u8().field("SubMesh pointer tag")?,
                    0u8,
                )?;
                sub_mesh(&mut c, 0)
            })
            .collect::<Result<Vec<_>, Error>>()?;
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
            sub_meshes,
            bones,
            unknown,
            compiled,
            materials,
        })
    }
}
