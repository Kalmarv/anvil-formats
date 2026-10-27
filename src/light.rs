//! WhiteRoom SpotLight records. Offsets established in the Riders Republic survey of lighting. Stored intensity
//! units and shadow booleans remain source data; a caller must state its calibration.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::fields::{BodyError, LE, body_error, length, refuse};
use primase_foundations::bytes::{Field, View};

/// A SpotLight's stored light, every number as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredSpotLight {
    /// The LightColor's four lanes (red, green, blue, alpha).
    pub color: [f32; 4],
    /// The stored intensity (f32 at offset 54), finite and not negative; its unit is not interpreted.
    pub intensity: f32,
    /// The stored luminance (f32 at offset 58), finite and not negative; its unit is not interpreted.
    pub luminance: f32,
    /// The cone's inner angle, radians.
    pub inner: f32,
    /// The cone's outer angle, radians (at most pi).
    pub outer: f32,
    /// The light's range (f32 at offset 320), finite and above zero; its unit is not interpreted.
    pub range: f32,
    /// The near distance (f32 at offset 324), finite and not negative.
    pub near: f32,
}
/// The SpotLight component's type id.
pub const SPOT_LIGHT: u32 = 0x8032_0FB8;

/// A SpotLight body's length.
const SPOT_LENGTH: usize = 328;
/// A LightInstance's length.
const INSTANCE_LENGTH: usize = 311;

/// Why a SpotLight or LightInstance body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body has the wrong length, a wrong LightColor class, or a float that is negative or not finite;
    /// the inner error names the field and offset.
    #[error("light {0}")]
    Body(#[from] BodyError),
}
body_error!(Error, Error::Body);

impl StoredSpotLight {
    /// A SpotLight's body (328 bytes), every float finite and not negative, the cone ordered.
    pub fn parse(body: &[u8]) -> Result<Self, Error> {
        if body.len() != SPOT_LENGTH {
            return Err(length("SpotLight length", body.len(), "328 bytes").into());
        }
        let v = View::new(body);
        let class = v.u32(34, LE).field("LightColor class")?;
        if class != 0xA797_67ED {
            return Err(refuse("LightColor class", 34, class.into(), "0xa79767ed").into());
        }
        let float = |at: usize, what: &'static str| -> Result<f32, Error> {
            let bits = v.u32(at, LE).field(what)?;
            let value = f32::from_bits(bits);
            if value.is_finite() && value >= 0.0 {
                Ok(value)
            } else {
                Err(refuse(what, at, bits.into(), "a finite number, not negative").into())
            }
        };
        let light = Self {
            color: [
                float(38, "color red")?,
                float(42, "color green")?,
                float(46, "color blue")?,
                float(50, "color alpha")?,
            ],
            intensity: float(54, "intensity")?,
            luminance: float(58, "luminance")?,
            inner: float(308, "inner angle")?,
            outer: float(312, "outer angle")?,
            range: float(320, "range")?,
            near: float(324, "near")?,
        };
        if light.inner > light.outer {
            return Err(refuse(
                "inner angle",
                308,
                light.inner.to_bits().into(),
                "at most the outer angle",
            )
            .into());
        }
        if light.outer > std::f32::consts::PI {
            return Err(refuse("outer angle", 312, light.outer.to_bits().into(), "at most pi").into());
        }
        if light.range == 0.0 {
            return Err(refuse("range", 320, light.range.to_bits().into(), "above zero").into());
        }
        Ok(light)
    }
}

/// The light reference is embedded in LightInstance, not its Visual's object field. Its leading 03 01 is the
/// established managed reference form here; it must not be passed to the generic null-reference decoder.
pub fn instance_reference(bytes: &[u8]) -> Result<u64, Error> {
    if bytes.len() != INSTANCE_LENGTH {
        return Err(length("LightInstance length", bytes.len(), "311 bytes").into());
    }
    let v = View::new(bytes);
    let class = v.u32(12, LE).field("BoundingVolume class")?;
    if class != 0x4AEC_3476 {
        return Err(refuse("BoundingVolume class", 12, class.into(), "0x4aec3476").into());
    }
    let form = v.array::<2>(44).field("light reference")?;
    if form != [3, 1] {
        return Err(refuse(
            "light reference form",
            44,
            u64::from(u16::from_le_bytes(form)),
            "the managed form 03 01",
        )
        .into());
    }
    Ok(v.u64(46, LE).field("light reference id")?)
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    reason = "fixtures"
)]
mod tests {
    use super::*;
    #[test]
    fn managed_light_reference_is_not_mistaken_for_a_null_material_reference() {
        let mut instance = vec![0; 311];
        instance[12..16].copy_from_slice(&0x4AEC_3476u32.to_le_bytes());
        instance[44..46].copy_from_slice(&[3, 1]);
        instance[46..54].copy_from_slice(&0x1D5_EF76_A006u64.to_le_bytes());
        assert_eq!(instance_reference(&instance).unwrap(), 0x1D5_EF76_A006);
        instance[44] = 0;
        assert!(instance_reference(&instance).is_err());
    }
    #[test]
    fn spot_fields_remain_stored_values_and_invalid_cones_are_refused() {
        let mut body = vec![0; 328];
        body[34..38].copy_from_slice(&0xA797_67EDu32.to_le_bytes());
        for (offset, value) in [
            (38, 1.0f32),
            (42, 0.891),
            (46, 0.560),
            (50, 1.0),
            (54, 5.3e9),
            (58, 3.4e7),
            (308, 0.2),
            (312, 0.3),
            (320, 20.0),
            (324, 0.1),
        ] {
            body[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let stored = StoredSpotLight::parse(&body).unwrap();
        assert_eq!(stored.intensity, 5.3e9);
        assert_eq!(stored.range, 20.0);
        body[308..312].copy_from_slice(&0.4f32.to_le_bytes());
        assert!(StoredSpotLight::parse(&body).is_err());
    }
}
