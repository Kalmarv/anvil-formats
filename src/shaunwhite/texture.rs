//! Shaun White Skateboarding (PC) TextureMap objects: the stored texture description, its inline CompiledTextureMap and
//! the mip levels it holds, read as stored. Pixels are not decoded.
//!
//! A texture is one TextureMap object holding its whole mip chain inline (there are no external mip resources, unlike
//! the later engine's [`crate::texture`]). The body (little-endian, offsets in the body):
//! - 0: the managed byte (1); 1: the object's id; 5: its class ([`TEXTURE_MAP`]);
//! - 9: width, 13: height, 17: depth, 21: the format code, 25: the dimension (1 flat, 2 cube), 29: gamma (0 linear,
//!   1 sRGB), 33: the mip count (0 is one level), each a u32;
//! - 37: address U, V, W (three u32); 49: magnification, minification and mip filters (three u32); 61: the map type,
//!   65: the category (u32 each); 69: three bytes, kept as stored; 72: the user category (u32);
//! - 76: a byte 0 (the compiled object is inline), 77: the compiled object's id, 81: its class
//!   ([`COMPILED_TEXTURE_MAP`]), 85: the pixel byte length `P`, 89: the pixels;
//! - 89 + `P`: fifteen u32, the compiled copy: platform 2, SDK version 32, then width, height, depth, mip count,
//!   format code, dimension, gamma, address U, V, W and the three filters, equal to the fields above. The body ends
//!   there.
//!
//! The pixels are the levels largest first, each its format's size for its dimensions ([`crate::texture::Storage`]);
//! a cube map holds six complete chains, face by face. Every one of the 40,851 TextureMaps of the 33 data forges of
//! the PC patched build (forges of 2010-10-27) reads so: the compiled copy agrees with the head on every one, and the
//! levels fill the pixels exactly for every format code. The game's renderer, run on the same files, creates each
//! texture with the Direct3D 9 format [`d3d_format`] names and uploads each level's stored bytes unchanged
//! ([`StoredTextureMap::levels`]).
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use std::ops::Range;

use primase_foundations::bytes::{Cursor, Field, FieldError};

use crate::fields::LE;
use crate::texture::Storage;

pub use crate::material::TEXTURE_MAP;
pub use crate::texture::COMPILED_TEXTURE_MAP;

/// Why a TextureMap body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body ends before a field is complete.
    #[error("TextureMap body: {0}")]
    Short(#[from] FieldError),
    /// A value that must be fixed is another (the managed byte, a class id, the inline marker, platform or SDK
    /// version), or the dimension is not 1 or 2.
    #[error("TextureMap {what}: {found}, only {read} is read")]
    Value {
        /// The field refused.
        what: &'static str,
        /// The value found.
        found: u32,
        /// The value this reader accepts there.
        read: u32,
    },
    /// A field of the compiled copy differs from the same field before the pixels.
    #[error("TextureMap compiled copy: {what} {found}, the head's {stored}")]
    Compiled {
        /// The field.
        what: &'static str,
        /// The value in the compiled copy.
        found: u32,
        /// The value in the head.
        stored: u32,
    },
    /// Bytes follow the compiled copy.
    #[error("TextureMap: {0} bytes after the compiled copy")]
    Trailing(usize),
    /// The format code's storage is not established, or the pixels do not fill the levels exactly.
    #[error("TextureMap pixels: {why}")]
    Pixels {
        /// What does not match.
        why: String,
    },
}

/// A Shaun White Skateboarding TextureMap body as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTextureMap {
    /// The TextureMap object's id.
    pub id: u32,
    /// Level 0's width in texels.
    pub width: u32,
    /// Level 0's height in texels.
    pub height: u32,
    /// The depth (1 on every stored texture).
    pub depth: u32,
    /// The engine's format code ([`d3d_format`], [`storage`]).
    pub format: u32,
    /// The dimension: 1 flat, 2 cube.
    pub dimension: u32,
    /// 0 linear, 1 sRGB.
    pub gamma: u32,
    /// The mip count as stored (0 means one level; [`StoredTextureMap::level_count`]).
    pub mip_count: u32,
    /// Address modes U, V, W, as stored.
    pub address: [u32; 3],
    /// Magnification, minification and mip filters, as stored.
    pub filters: [u32; 3],
    /// The map type (u32), as stored.
    pub map_type: u32,
    /// The category (u32), as stored.
    pub category: u32,
    /// Three bytes whose meaning is not established, as stored.
    pub unknown_flags: [u8; 3],
    /// The user category (u32), as stored.
    pub user_category: u32,
    /// The inline CompiledTextureMap's id.
    pub compiled_id: u32,
    /// Where the pixels lie in the body.
    pub pixels: Range<usize>,
}

/// One stored mip level: which face and level it is, its size, and its bytes in the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    /// The cube face (0 to 5, Direct3D's +X, -X, +Y, -Y, +Z, -Z order), 0 on a flat texture.
    pub face: u32,
    /// The mip level, 0 the largest.
    pub mip: u32,
    /// The level's width in texels.
    pub width: u32,
    /// The level's height in texels.
    pub height: u32,
    /// Where its bytes lie in the body.
    pub bytes: Range<usize>,
}

/// How a format code's levels are stored, for the codes the 40,851 TextureMaps use: 2 and 3 in 8-byte blocks, 4, 5
/// and 12 in 16-byte blocks, 0 in four bytes per texel. Every TextureMap with one of these codes fills its pixels
/// exactly so. Another code is `None`.
pub fn storage(code: u32) -> Option<Storage> {
    match code {
        2 | 3 => Some(Storage::Blocks(8)),
        4 | 5 | 12 => Some(Storage::Blocks(16)),
        0 => Some(Storage::Pixels(4)),
        _ => None,
    }
}

/// The Direct3D 9 format (a `D3DFORMAT` value) the game creates a texture of this format code with, as its renderer
/// does on the PC build's own files: 2 and 3 `D3DFMT_DXT1`, 4 `D3DFMT_DXT3`, 5 and 12 `D3DFMT_DXT5`. Code 0 (four
/// bytes per texel, [`storage`]) and any other code are `None`: the game was not seen creating a texture of code 0.
pub fn d3d_format(code: u32) -> Option<u32> {
    const fn fourcc(s: &[u8; 4]) -> u32 {
        u32::from_le_bytes(*s)
    }
    match code {
        2 | 3 => Some(fourcc(b"DXT1")),
        4 => Some(fourcc(b"DXT3")),
        5 | 12 => Some(fourcc(b"DXT5")),
        _ => None,
    }
}

fn word(c: &mut Cursor<'_>, name: &'static str) -> Result<u32, Error> {
    Ok(c.u32(LE).field(name)?)
}
fn fixed(c: &mut Cursor<'_>, name: &'static str, read: u32) -> Result<(), Error> {
    let found = word(c, name)?;
    if found == read {
        Ok(())
    } else {
        Err(Error::Value {
            what: name,
            found,
            read,
        })
    }
}
fn byte(c: &mut Cursor<'_>, name: &'static str, read: u8) -> Result<(), Error> {
    let found = c.u8().field(name)?;
    if found == read {
        Ok(())
    } else {
        Err(Error::Value {
            what: name,
            found: u32::from(found),
            read: u32::from(read),
        })
    }
}

impl StoredTextureMap {
    /// Reads a TextureMap object's body (the module documentation), refusing a value it fixes, a compiled copy that
    /// disagrees with the head, or bytes after it. The levels are sliced by [`StoredTextureMap::levels`].
    pub fn parse(body: &[u8]) -> Result<StoredTextureMap, Error> {
        let mut c = Cursor::new(body);
        byte(&mut c, "managed byte", 1)?;
        let id = word(&mut c, "id")?;
        fixed(&mut c, "class", TEXTURE_MAP)?;
        let width = word(&mut c, "width")?;
        let height = word(&mut c, "height")?;
        let depth = word(&mut c, "depth")?;
        let format = word(&mut c, "format code")?;
        let dimension = word(&mut c, "dimension")?;
        if !matches!(dimension, 1 | 2) {
            return Err(Error::Value {
                what: "dimension",
                found: dimension,
                read: 1,
            });
        }
        let gamma = word(&mut c, "gamma")?;
        let mip_count = word(&mut c, "mip count")?;
        let mut address = [0; 3];
        for a in &mut address {
            *a = word(&mut c, "address mode")?;
        }
        let mut filters = [0; 3];
        for f in &mut filters {
            *f = word(&mut c, "filter")?;
        }
        let map_type = word(&mut c, "map type")?;
        let category = word(&mut c, "category")?;
        let unknown_flags = c.array::<3>().field("flags")?;
        let user_category = word(&mut c, "user category")?;
        byte(&mut c, "compiled pointer byte", 0)?;
        let compiled_id = word(&mut c, "compiled id")?;
        fixed(&mut c, "compiled class", COMPILED_TEXTURE_MAP)?;
        let length = c.count(LE, 1).field("pixel byte length")?;
        let start = c.position();
        c.take(length).field("pixels")?;
        let pixels = start..c.position();
        fixed(&mut c, "platform", 2)?;
        fixed(&mut c, "SDK version", 32)?;
        for (what, stored) in [
            ("width", width),
            ("height", height),
            ("depth", depth),
            ("mip count", mip_count),
            ("format code", format),
            ("dimension", dimension),
            ("gamma", gamma),
            ("address U", address[0]),
            ("address V", address[1]),
            ("address W", address[2]),
            ("magnification filter", filters[0]),
            ("minification filter", filters[1]),
            ("mip filter", filters[2]),
        ] {
            let found = word(&mut c, what)?;
            if found != stored {
                return Err(Error::Compiled { what, found, stored });
            }
        }
        if c.remaining() != 0 {
            return Err(Error::Trailing(c.remaining()));
        }
        Ok(StoredTextureMap {
            id,
            width,
            height,
            depth,
            format,
            dimension,
            gamma,
            mip_count,
            address,
            filters,
            map_type,
            category,
            unknown_flags,
            user_category,
            compiled_id,
            pixels,
        })
    }

    /// The number of mip levels held per face: the mip count, or 1 where it is 0.
    pub fn level_count(&self) -> u32 {
        self.mip_count.max(1)
    }

    /// The number of faces: 6 for a cube map, 1 otherwise.
    pub fn faces(&self) -> u32 {
        if self.dimension == 2 { 6 } else { 1 }
    }

    /// The stored levels, face by face and largest first, as ranges of the body. Refused when the format code's
    /// storage is not established ([`storage`]) or the levels do not fill the pixels exactly.
    pub fn levels(&self) -> Result<Vec<Level>, Error> {
        let storage = storage(self.format).ok_or_else(|| Error::Pixels {
            why: format!("format code {} has no established storage", self.format),
        })?;
        let mut out = Vec::new();
        let mut at = self.pixels.start;
        for face in 0..self.faces() {
            for mip in 0..self.level_count() {
                let width = self.width.checked_shr(mip).unwrap_or(0).max(1);
                let height = self.height.checked_shr(mip).unwrap_or(0).max(1);
                let end = storage
                    .level_size(width, height)
                    .and_then(|n| at.checked_add(n))
                    .filter(|&e| e <= self.pixels.end)
                    .ok_or_else(|| Error::Pixels {
                        why: format!(
                            "face {face} level {mip} ({width} x {height}) runs past the {} pixel bytes",
                            self.pixels.len()
                        ),
                    })?;
                out.push(Level {
                    face,
                    mip,
                    width,
                    height,
                    bytes: at..end,
                });
                at = end;
            }
        }
        if at != self.pixels.end {
            return Err(Error::Pixels {
                why: format!("the levels end at {at}, the pixels at {}", self.pixels.end),
            });
        }
        Ok(out)
    }
}
