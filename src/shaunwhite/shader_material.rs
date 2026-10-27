//! Fixed-size generic-material fields used by the native shader key setters. The base bitfields serialize
//! as separate values: blend@21, bits4/5@25/26, eight-bit reference@27, display mode@28, bits16..27@32..43,
//! bits29/30@44/45. Runtime bit28 is not serialized. These are not the shader define bit positions.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use super::material::{Error, Material};
use primase_foundations::bytes::{Endian, View};

/// Source fields used by the native generic-material key setters. `flags` is the reconstructed runtime
/// Material flag word, not a shader key. Texture presence follows the stored slots, including unread textures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialShader {
    pub flags: u32,
    pub layers: [bool; 2],
    pub half_lambert: bool,
    pub glossiness: bool,
    pub layer3_no_specular: bool,
    pub rim_alpha: bool,
    pub hide: u32,
    pub dynamic_uv: u32,
    pub texture_slots: Vec<String>,
    pub unsupported: Vec<String>,
}
const LE: Endian = Endian::Little;

/// Read source controls without choosing a pass, frame, vertex layout or shader permutation.
pub fn read(body: &[u8]) -> Result<MaterialShader, Error> {
    let m = Material::parse(body)?;
    let v = View::new(body);
    let flag = |at: usize| -> Result<bool, Error> {
        match body.get(at).copied() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            x => Err(Error::Invalid {
                what: "shader material bool",
                value: format!("at {at}: {x:?}"),
            }),
        }
    };
    let word = |at: usize| {
        v.u32(at, LE).map_err(|e| Error::Invalid {
            what: "shader material word",
            value: e.to_string(),
        })
    };
    let blend = word(21)?;
    let display = word(28)?;
    if blend > 15 || display > 3 {
        return Err(Error::Invalid {
            what: "base material flag enum",
            value: format!("blend {blend}, display {display}"),
        });
    }
    let reference = v.u8(27).map_err(|e| Error::Invalid {
        what: "shader material alpha reference",
        value: e.to_string(),
    })?;
    // `bit(on, n)`: bit `n` of the flag word, set when `on`.
    let bit = |on: bool, n: u32| u32::from(on).rotate_left(n);
    let mut flags = blend
        | bit(flag(25)?, 4)
        | bit(flag(26)?, 5)
        | u32::from(reference).rotate_left(6)
        | display.rotate_left(14);
    for (at, n) in (32usize..=43).zip(16u32..=27) {
        flags |= bit(flag(at)?, n);
    }
    flags |= bit(flag(44)?, 29) | bit(flag(45)?, 30);
    let mut unsupported = Vec::new();
    for (at, name) in [(310, "video texture"), (320, "multi-texture set")] {
        if flag(at)? {
            unsupported.push(name.to_owned());
        }
    }
    let hide = word(525)?;
    if hide > 2 {
        return Err(Error::Invalid {
            what: "InfluenceHidePixelMode",
            value: hide.to_string(),
        });
    }
    let dynamic_uv = word(496)?;
    if dynamic_uv > 2 {
        return Err(Error::Invalid {
            what: "dynamic UV mode",
            value: dynamic_uv.to_string(),
        });
    }
    // Non-normal alpha-display paths require further material key setters, not a similar normal-alpha record.
    if display != 0 || word(324)? != 0 || flag(319)? {
        unsupported.push("non-normal alpha-display/key fields".to_owned());
    }
    Ok(MaterialShader {
        flags,
        layers: [flag(122)?, flag(123)?],
        half_lambert: flag(553)?,
        glossiness: flag(554)?,
        layer3_no_specular: flag(495)?,
        rim_alpha: flag(92)?,
        hide,
        dynamic_uv,
        texture_slots: m.slots.iter().map(|s| s.name.to_owned()).collect(),
        unsupported,
    })
}
