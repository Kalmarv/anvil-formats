//! A Material's or TextureSet's body: [`StoredReferences`] reads the two references both start with and keeps the rest
//! as stored; [`StoredMaterial`] and [`StoredTextureSet`] read the whole bodies. What a reference is for (its slot's
//! role) is not read.
//!
//! The body (little-endian, after the resource's class id and extension): a byte (1), then two 10-byte references
//! ([`crate::reference`]: `01`, a byte and a u64 ClassID, or `03` and nine zeros for none), then the rest. No
//! count of references is stored before them, and the rest may hold more: two are what is established.
//!
//! Established on three Materials and a TextureSet (Riders Republic build 25394987, base WhiteRoom forge, entries 26,
//! 210, 162 and 163). There, a Material's
//! second reference resolves to a TextureSet in all three and its first to a MaterialTemplate (210's in the same forge;
//! 26's and 162's in DataPC.forge); the
//! TextureSet's two resolve to TextureMaps. Another first byte, a reference
//! [`crate::reference`] refuses, or a body shorter than the two references, is refused, naming it.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::fields::LE;
pub use crate::mesh::MATERIAL;
use crate::reference::{self, Reference};
use primase_foundations::bytes::{Cursor, Field, FieldError};

/// A Material's flags as stored: body 41 to 84 (of the 102 bytes after its references, 20 to 63), the fields the
/// engine's Material deserialiser reads there in its order, named by its class registry (Riders Republic): `BlendMode` (a u32, of which the engine keeps 4 bits), `AlphaDisplayMode` (a u32, 2 bits),
/// `AlphaTestValue` and 35 booleans, a byte each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialFlags {
    /// `BlendMode` (u32 at body offset 41), as stored.
    pub blend_mode: u32,
    /// `AlphaDisplayMode` (u32 at body offset 45), as stored.
    pub alpha_display_mode: u32,
    /// `AlphaTestValue` (u8 at body offset 49), as stored.
    pub alpha_test_value: u8,
    /// Boolean j at body 50 + j.
    pub booleans: [u8; 35],
}

impl MaterialFlags {
    /// Booleans by their registry names (index j).
    pub const Z_WRITE_DISABLED_OPAQUE: usize = 0;
    /// Index 1: depth writes are disabled for the alpha pass.
    pub const Z_WRITE_DISABLED_ALPHA: usize = 1;
    /// Index 2: the depth test is disabled.
    pub const Z_TEST_DISABLED: usize = 2;
    /// Index 3: the Material is a layer material.
    pub const LAYER_MATERIAL: usize = 3;
    /// Index 10: the Material is drawn before the transparent pass.
    pub const PRE_TRANSPARENT: usize = 10;
    /// Index 11: the normal is flipped on back faces.
    pub const FLIP_NORMAL_ON_BACK_FACE: usize = 11;
    /// Index 15: the Material is rendered in low resolution.
    pub const RENDER_IN_LOW_RESOLUTION: usize = 15;
    /// Index 24: alpha testing is enabled (see `alpha_test_value`).
    pub const ALPHA_TEST_ENABLED: usize = 24;
    /// Index 25: the Material is drawn two-sided.
    pub const TWO_SIDED: usize = 25;
    /// The boolean whose name's CRC-32 is `42452A12` (the name not known).
    pub const BOOLEAN_42452A12: usize = 34;
}

/// A record type: equals the CRC-32 of the ASCII string `TextureSet` (the name a label).
pub const TEXTURE_SET: u32 = 0xD70E_6670;
/// A record type: equals the CRC-32 of the ASCII string `TextureMap` (the name a label).
pub const TEXTURE_MAP: u32 = 0xA2B7_E917;
/// A record type: equals the CRC-32 of the ASCII string `MaterialTemplate` (the name a label).
pub const MATERIAL_TEMPLATE: u32 = 0xBCFB_3C7A;

/// Why a Material or TextureSet body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body is shorter than the first byte and two references (21 bytes).
    #[error("reference body: {have} bytes, shorter than its two references (21)")]
    Short {
        /// The body's length in bytes.
        have: usize,
    },
    /// The first byte is not 1.
    #[error("reference body: byte {found} at 0, only 1 is read")]
    First {
        /// The first byte found.
        found: u8,
    },
    /// One of the two references is malformed.
    #[error("reference body: reference {index}: {source}")]
    Reference {
        /// Which reference, 0 or 1.
        index: usize,
        /// Why it was refused.
        #[source]
        source: crate::reference::Error,
    },
}

/// A Material's or TextureSet's body as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredReferences {
    /// The two references, in order (`None`: the stored form referring to nothing).
    pub references: [Option<Reference>; 2],
    /// Everything after them, uninterpreted.
    pub rest: Vec<u8>,
}

impl StoredReferences {
    /// The body parsed.
    pub fn parse(body: &[u8]) -> Result<StoredReferences, Error> {
        let have = body.len();
        let (head, rest) = body.split_first_chunk::<21>().ok_or(Error::Short { have })?;
        let (first, refs) = head.split_first().ok_or(Error::Short { have })?;
        if *first != 1 {
            return Err(Error::First { found: *first });
        }
        let (a, b) = refs.split_at(reference::LENGTH);
        let read = |index: usize, bytes: &[u8]| {
            let bytes: [u8; reference::LENGTH] = bytes.try_into().map_err(|_| Error::Short { have })?;
            reference::parse(bytes).map_err(|source| Error::Reference { index, source })
        };
        Ok(StoredReferences {
            references: [read(0, a)?, read(1, b)?],
            rest: rest.to_vec(),
        })
    }
}

/// A Material's body read whole: a byte (1), two references, 102 bytes kept as stored, a u32 count, and that many
/// property records ending the body. A record is a 16-byte header (a u32 key, a u32 class, a zero u16, a u32 type,
/// a zero u16) and a payload whose length its (class, type) gives ([`Payload`]); another (class, type), a nonzero u16,
/// a class echo unlike the class or a body not ended by the last record is refused. Established by the census on
/// every Material of Riders Republic (13,579) and by RRAssetStudio reading the same framing; the names key, class and
/// type, and what a payload's fields mean, are RRAssetStudio's labels, not read here.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredMaterial {
    /// The two references at the start: a Material's MaterialTemplate and TextureSet on the fixtures, or `None` where stored as the empty form.
    pub references: [Option<Reference>; 2],
    /// The 102 bytes after the references (its flags at 20 to 63: [`StoredMaterial::flags`]).
    pub unread: Vec<u8>,
    /// The property records, in stored order.
    pub properties: Vec<Property>,
}

/// One property record.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    /// The property's key (u32); a texture property's key is the CRC-32 of the second string of a descriptor in the Material's template ([`crate::template::Descriptor::key`]).
    pub key: u32,
    /// The property's class (u32), echoed inside its payload for the structured payloads.
    pub class: u32,
    /// The property's type (u32); with the class it selects the payload.
    pub kind: u32,
    /// The payload the (class, type) selects.
    pub payload: Payload,
}

/// A property's payload by its (class, type), as stored. The `head` bytes and those named by offset are kept,
/// not interpreted.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    /// (0, 7): 4 bytes as an i32.
    Int(i32),
    /// (0, 10): 4 bytes as an f32.
    Float(f32),
    /// (0, 13): 16 bytes as four f32.
    Float4([f32; 4]),
    /// (0x7D08460D, 19), 45 bytes: 8 bytes, the class again, a byte, three u32, two references (the second a
    /// TextureMap on the fixtures).
    Texture {
        /// The 8 bytes before the echoed class, as stored.
        head: [u8; 8],
        /// The byte after the echoed class, as stored.
        byte: u8,
        /// The three u32 after the byte, as stored.
        words: [u32; 3],
        /// The two references that end the payload (the second is a TextureMap on the fixtures).
        references: [Option<Reference>; 2],
    },
    /// (0xC52E2125, 19), 47 bytes: 8 bytes, the class again, eight f32, three bytes.
    Floats8 {
        /// The 8 bytes before the echoed class, as stored.
        head: [u8; 8],
        /// The eight f32 after the echoed class, as stored.
        floats: [f32; 8],
        /// The three bytes ending the payload, as stored.
        bytes: [u8; 3],
    },
    /// (0xECE5D96C, 19), 36 bytes: 8 bytes, the class again, six f32.
    Floats6 {
        /// The 8 bytes before the echoed class, as stored.
        head: [u8; 8],
        /// The six f32 after the echoed class, as stored.
        floats: [f32; 6],
    },
    /// (0xBC0F16F5, 19), 45 bytes: 8 bytes, the class again, 33 bytes kept.
    Raw45 {
        /// The 8 bytes before the echoed class, as stored.
        head: [u8; 8],
        /// The 33 bytes after the echoed class, as stored.
        rest: Vec<u8>,
    },
}

/// Why a Material body can't be read whole.
#[derive(Debug, thiserror::Error)]
pub enum MaterialError {
    /// The body ends before a field is complete.
    #[error("Material body: {0}")]
    Short(#[from] FieldError),
    /// The first byte or one of the two references at the start is malformed.
    #[error("Material prefix: {0}")]
    Prefix(#[from] Error),
    /// A property record has a nonzero header u16, a class other than the echo, or a (class, type) not read; `index` is the record's position and `why` says which.
    #[error("Material property {index}: {why}")]
    Property {
        /// The record's position.
        index: usize,
        /// What is wrong with it.
        why: String,
    },
    /// Bytes remain after the last property; the count is how many.
    #[error("Material body: {0} bytes after its last property")]
    Trailing(usize),
}

impl StoredMaterial {
    /// Its flags (body 41 to 84: [`MaterialFlags`]).
    pub fn flags(&self) -> MaterialFlags {
        let word = |w: Option<&[u8]>| w.and_then(|w| w.try_into().ok()).map_or(0, u32::from_le_bytes);
        MaterialFlags {
            blend_mode: word(self.unread.get(20..24)),
            alpha_display_mode: word(self.unread.get(24..28)),
            alpha_test_value: self.unread.get(28).copied().unwrap_or(0),
            booleans: self
                .unread
                .get(29..64)
                .and_then(|b| b.try_into().ok())
                .unwrap_or([0; 35]),
        }
    }

    /// The body parsed.
    pub fn parse(body: &[u8]) -> Result<StoredMaterial, MaterialError> {
        let prefix = StoredReferences::parse(body)?;
        let mut c = Cursor::new(body);
        c.seek(21).field("102 bytes after the references")?;
        let unread = c.array::<102>().field("102 bytes after the references")?.to_vec();
        let count = c.count(LE, 16).field("property records")?;
        let mut properties = Vec::with_capacity(count);
        for index in 0..count {
            let key = c.u32(LE).field("property key")?;
            let class = c.u32(LE).field("property class")?;
            let [z0, z1] = c.array::<2>().field("property")?;
            let kind = c.u32(LE).field("property type")?;
            let [z2, z3] = c.array::<2>().field("property")?;
            if [z0, z1, z2, z3] != [0; 4] {
                return Err(MaterialError::Property {
                    index,
                    why: "a nonzero u16 in its header".into(),
                });
            }
            let echo = |c: &mut Cursor<'_>| -> Result<[u8; 8], MaterialError> {
                let head = c.array::<8>().field("property payload")?;
                let again = c.u32(LE).field("property class again")?;
                if again != class {
                    return Err(MaterialError::Property {
                        index,
                        why: format!("class {class:#010x}, {again:#010x} in its payload"),
                    });
                }
                Ok(head)
            };
            let payload = match (class, kind) {
                (0, 7) => Payload::Int(i32::from_le_bytes(c.array::<4>().field("property payload")?)),
                (0, 10) => Payload::Float(c.f32(LE).field("property payload")?),
                (0, 13) => Payload::Float4([
                    c.f32(LE).field("property payload")?,
                    c.f32(LE).field("property payload")?,
                    c.f32(LE).field("property payload")?,
                    c.f32(LE).field("property payload")?,
                ]),
                (0x7D08_460D, 19) => {
                    let head = echo(&mut c)?;
                    let [byte] = c.array::<1>().field("property payload")?;
                    let words = [
                        c.u32(LE).field("property payload")?,
                        c.u32(LE).field("property payload")?,
                        c.u32(LE).field("property payload")?,
                    ];
                    let references = [
                        property_reference(&mut c, index)?,
                        property_reference(&mut c, index)?,
                    ];
                    Payload::Texture {
                        head,
                        byte,
                        words,
                        references,
                    }
                }
                (0xC52E_2125, 19) => {
                    let head = echo(&mut c)?;
                    let mut floats = [0.0; 8];
                    for f in &mut floats {
                        *f = c.f32(LE).field("property payload")?;
                    }
                    Payload::Floats8 {
                        head,
                        floats,
                        bytes: c.array::<3>().field("property payload")?,
                    }
                }
                (0xECE5_D96C, 19) => {
                    let head = echo(&mut c)?;
                    let mut floats = [0.0; 6];
                    for f in &mut floats {
                        *f = c.f32(LE).field("property payload")?;
                    }
                    Payload::Floats6 { head, floats }
                }
                (0xBC0F_16F5, 19) => {
                    let head = echo(&mut c)?;
                    Payload::Raw45 {
                        head,
                        rest: c.array::<33>().field("property payload")?.to_vec(),
                    }
                }
                _ => {
                    return Err(MaterialError::Property {
                        index,
                        why: format!("class {class:#010x}, type {kind}: not read"),
                    });
                }
            };
            properties.push(Property {
                key,
                class,
                kind,
                payload,
            });
        }
        if c.remaining() != 0 {
            return Err(MaterialError::Trailing(c.remaining()));
        }
        Ok(StoredMaterial {
            references: prefix.references,
            unread,
            properties,
        })
    }
}

/// A TextureSet's body as stored: a byte (1), 19 references ([`crate::reference`]), a zero byte, 8 bytes and a
/// u32, 204 bytes in all. Established by the census: every TextureSet of Riders Republic (10,027) is exactly that,
/// ending the body (RRAssetStudio reads the same). Which reference is which texture's role is not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTextureSet {
    /// The 19 references, in stored order (`None`: the stored form referring to nothing).
    pub references: [Option<Reference>; 19],
    /// The 8 bytes after the zero byte.
    pub rest: [u8; 8],
    /// The u32 ending the body (0, 19, 20, 21 and 27 seen; what it counts is not established).
    pub last: u32,
}

/// Why a TextureSet body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum TextureSetError {
    /// The body is not 204 bytes; the length found.
    #[error("TextureSet body: {0} bytes, not 204")]
    Length(usize),
    /// A byte that must be fixed is another: the first byte must be 1 and the byte at offset 191 must be 0.
    #[error("TextureSet body: byte {found} at {at}, only {read} is read")]
    Byte {
        /// The byte's offset in the body.
        at: usize,
        /// The byte found.
        found: u8,
        /// The only value this reader accepts there.
        read: u8,
    },
    /// One of the 19 references is malformed.
    #[error("TextureSet reference {index}: {source}")]
    Reference {
        /// Which reference, counting from 0.
        index: usize,
        /// Why it was refused.
        #[source]
        source: crate::reference::Error,
    },
}

impl StoredTextureSet {
    /// The body parsed.
    pub fn parse(body: &[u8]) -> Result<StoredTextureSet, TextureSetError> {
        let body: &[u8; 204] = body.try_into().map_err(|_| TextureSetError::Length(body.len()))?;
        let (first, rest) = body
            .split_first_chunk::<1>()
            .ok_or(TextureSetError::Length(204))?;
        if first[0] != 1 {
            return Err(TextureSetError::Byte {
                at: 0,
                found: first[0],
                read: 1,
            });
        }
        let (refs, tail) = rest.split_at(19 * reference::LENGTH);
        let mut references = [None; 19];
        for (index, (slot, bytes)) in references
            .iter_mut()
            .zip(refs.as_chunks::<{ reference::LENGTH }>().0)
            .enumerate()
        {
            *slot =
                reference::parse(*bytes).map_err(|source| TextureSetError::Reference { index, source })?;
        }
        let [zero, r0, r1, r2, r3, r4, r5, r6, r7, l0, l1, l2, l3] = tail else {
            return Err(TextureSetError::Length(204));
        };
        if *zero != 0 {
            return Err(TextureSetError::Byte {
                at: 191,
                found: *zero,
                read: 0,
            });
        }
        Ok(StoredTextureSet {
            references,
            rest: [*r0, *r1, *r2, *r3, *r4, *r5, *r6, *r7],
            last: u32::from_le_bytes([*l0, *l1, *l2, *l3]),
        })
    }
}

/// A reference in property `index`'s payload.
fn property_reference(c: &mut Cursor, index: usize) -> Result<Option<Reference>, MaterialError> {
    reference::parse(c.array::<{ reference::LENGTH }>().field("reference")?).map_err(|e| {
        MaterialError::Property {
            index,
            why: e.to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(first: u8, refs: [[u8; 10]; 2], rest: &[u8]) -> Vec<u8> {
        let mut b = vec![first];
        b.extend(refs[0]);
        b.extend(refs[1]);
        b.extend(rest);
        b
    }

    const TO_7: [u8; 10] = [1, 1, 7, 0, 0, 0, 0, 0, 0, 0];
    const NONE: [u8; 10] = [3, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    #[test]
    fn a_body_reads_its_two_references_and_keeps_the_rest() {
        let r = StoredReferences::parse(&body(1, [TO_7, NONE], &[3, 0, 9])).expect("read");
        assert_eq!(r.references, [Some(Reference { mode: 1, id: 7 }), None]);
        assert_eq!(r.rest, [3, 0, 9]);
    }

    #[test]
    fn another_first_byte_a_bad_reference_or_a_short_body_is_refused() {
        assert!(matches!(
            StoredReferences::parse(&body(0, [TO_7, TO_7], &[])),
            Err(Error::First { found: 0 })
        ));
        let mut bad = NONE;
        bad[9] = 1;
        assert!(matches!(
            StoredReferences::parse(&body(1, [TO_7, bad], &[])),
            Err(Error::Reference { index: 1, .. })
        ));
        let b = body(1, [TO_7, TO_7], &[]);
        assert!(matches!(
            StoredReferences::parse(b.get(..20).expect("20")),
            Err(Error::Short { have: 20 })
        ));
    }

    #[test]
    fn a_texture_set_reads_its_19_references() {
        let mut b = vec![1];
        for k in 0..19u8 {
            b.extend(if k % 2 == 0 {
                [1, 1, k, 0, 0, 0, 0, 0, 0, 0]
            } else {
                NONE
            });
        }
        b.push(0);
        b.extend([0; 8]);
        b.extend(19u32.to_le_bytes());
        let t = StoredTextureSet::parse(&b).expect("read");
        assert_eq!(t.references[2], Some(Reference { mode: 1, id: 2 }));
        assert_eq!(t.references[1], None);
        assert_eq!(t.last, 19);
        let mut short = b.clone();
        short.pop();
        assert!(matches!(
            StoredTextureSet::parse(&short),
            Err(TextureSetError::Length(203))
        ));
        *b.get_mut(191).expect("204 bytes") = 1;
        assert!(matches!(
            StoredTextureSet::parse(&b),
            Err(TextureSetError::Byte { at: 191, .. })
        ));
    }
}
