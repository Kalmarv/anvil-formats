//! LayeredSky's authored scattering keyframes. The two resource profiles are from the registry-based read of
//! the Riders Republic survey; unknown profiles fail rather than using guessed offsets.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::fields::{BodyError, LE, body_error, length, refuse};
use primase_foundations::bytes::{Cursor, Field, View};

/// One authored time of day's scattering tables, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyKeyframe {
    /// The hour of day, in `[0, 24)`.
    pub hour: f32,
    /// The keyframe's CRC as stored (u32 at offset 17 of the keyframe).
    pub crc: u32,
    /// 65,536 texels of four half floats.
    pub inscatter: Vec<u8>,
    /// 1,024 texels of two half floats.
    pub optical_depth: Vec<u8>,
    /// 768 f32 values.
    pub irradiance: Vec<u8>,
    /// 768 f32 values.
    pub sun_depth: Vec<u8>,
    /// 1,024 f32 values.
    pub sun_irradiance: Vec<u8>,
}

/// A LayeredSky's sun settings and keyframes, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredSky {
    /// The sun position mode (u32 at offset 12), as stored; what its values select is not interpreted.
    pub sun_position_mode: u32,
    /// The sun azimuth (f32 at offset 16), as stored.
    pub sun_azimuth: f32,
    /// The sun tilt (f32 at offset 20), as stored.
    pub sun_tilt: f32,
    /// The sun elevation (f32 at offset 24), as stored.
    pub sun_elevation: f32,
    /// The sun colour (four f32 at offsets 48 to 63), as stored.
    pub sun_color: [f32; 4],
    /// The sun intensity (f32 at offset 64), as stored.
    pub sun_intensity: f32,
    /// The three Mie phase settings (f32), as stored: at offsets 350, 354 and 358 in the 24-keyframe profile and 340, 344 and 348 in the one-keyframe profile.
    pub mie: [f32; 3],
    /// The keyframes in stored order, one one per authored time of day (24 in the 3,442,710-byte profile, 1 in the 143,994-byte one).
    pub keyframes: Vec<SkyKeyframe>,
}
/// The LayeredSky type id.
pub const LAYERED_SKY: u32 = 0x24D3_2CF8;
/// The WorldAmbiance type id.
pub const WORLD_AMBIANCE: u32 = 0x21C2_D472;
const KEYFRAME: u32 = 0x5CDF_6DE9;
const FRAME_BYTES: usize = 143_422;
/// A WorldAmbiance body's length.
const AMBIANCE_LENGTH: usize = 5277;

/// Why a sky body can't be read or sampled.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body is short, has an unestablished length, or fails one of the layout's checks; the inner error names the field and offset.
    #[error("sky {0}")]
    Body(#[from] BodyError),
    /// Sampling needs a finite hour and at least one keyframe.
    #[error(
        "sky: radiance needs a finite hour and at least one keyframe (hour {hour}, {keyframes} keyframes)"
    )]
    NoKeyframes {
        /// The hour that was asked for.
        hour: f32,
        /// How many keyframes the sky holds.
        keyframes: usize,
    },
    /// The direction passed to sampling has a non-finite component or is the zero vector.
    #[error("sky: a direction must be finite and not zero")]
    Direction,
    /// The Mie phase settings are not finite, an asymmetry is outside (-1, 1) or the mix is outside [0, 1].
    #[error(
        "sky: Mie phase settings must be finite, the two asymmetries inside (-1, 1) and the mix in [0, 1]"
    )]
    Mie,
    /// The scattering table is shorter than its sampling reads.
    #[error("sky: the scattering table has no texel at byte {at} ({length} bytes)")]
    Texel {
        /// The byte offset that was read.
        at: usize,
        /// The table's length in bytes.
        length: usize,
    },
    /// Sampling produced a radiance that is not finite.
    #[error("sky: the radiance is not finite")]
    NonFinite,
}
body_error!(Error, Error::Body);

impl StoredSky {
    /// A LayeredSky body of one of the two established profiles (3,442,710 bytes with 24 keyframes, 143,994 with
    /// one), every keyframe's tables read to its end.
    pub fn parse(body: &[u8]) -> Result<Self, Error> {
        let (start, count): (usize, usize) = match body.len() {
            3_442_710 => (582, 24),
            143_994 => (572, 1),
            found => {
                return Err(length(
                    "LayeredSky length",
                    found,
                    "3442710 bytes (24 keyframes) or 143994 (1)",
                )
                .into());
            }
        };
        let v = View::new(body);
        let count_at = start.saturating_sub(4);
        let stored = v.u32(count_at, LE).field("keyframe count")?;
        if usize::try_from(stored) != Ok(count) {
            return Err(refuse("keyframe count", count_at, stored.into(), "the profile's count").into());
        }
        let mut keyframes = Vec::with_capacity(count);
        for index in 0..count {
            let begin = index.saturating_mul(FRAME_BYTES).saturating_add(start);
            keyframes.push(Self::keyframe(
                v.slice(begin, FRAME_BYTES).field("keyframe")?,
                begin,
            )?);
        }
        let mie_at = if start == 582 {
            [350, 354, 358]
        } else {
            [340, 344, 348]
        };
        let float = |at: usize, what: &'static str| -> Result<f32, Error> {
            Ok(f32::from_bits(v.u32(at, LE).field(what)?))
        };
        Ok(Self {
            sun_position_mode: v.u32(12, LE).field("sun position mode")?,
            sun_azimuth: float(16, "sun azimuth")?,
            sun_tilt: float(20, "sun tilt")?,
            sun_elevation: float(24, "sun elevation")?,
            sun_intensity: float(64, "sun intensity")?,
            mie: [
                float(mie_at[0], "Mie")?,
                float(mie_at[1], "Mie")?,
                float(mie_at[2], "Mie")?,
            ],
            sun_color: [
                float(48, "sun color")?,
                float(52, "sun color")?,
                float(56, "sun color")?,
                float(60, "sun color")?,
            ],
            keyframes,
        })
    }

    /// The keyframe whose `FRAME_BYTES` are `frame`, at `origin` in the body.
    fn keyframe(frame: &[u8], origin: usize) -> Result<SkyKeyframe, Error> {
        let v = View::with_origin(frame, origin);
        let here = |local: usize| origin.saturating_add(local);
        let class = v.u32(8, LE).field("keyframe class")?;
        if class != KEYFRAME {
            return Err(refuse("keyframe class", here(8), class.into(), "0x5cdf6de9").into());
        }
        // ManagedObject's byte follows the object header, then TimeOfDay, CRC, five dimensions and a boolean.
        let hour = f32::from_bits(v.u32(13, LE).field("keyframe hour")?);
        if !hour.is_finite() || !(0.0..24.0).contains(&hour) {
            return Err(refuse(
                "keyframe hour",
                here(13),
                hour.to_bits().into(),
                "a finite hour in [0, 24)",
            )
            .into());
        }
        for (k, want) in [256u32, 64, 32, 32, 16].into_iter().enumerate() {
            let at = k.saturating_mul(4).saturating_add(21);
            let found = v.u32(at, LE).field("keyframe dimension")?;
            if found != want {
                return Err(refuse(
                    "keyframe dimension",
                    here(at),
                    found.into(),
                    "256, 64, 32, 32, 16 in turn",
                )
                .into());
            }
        }
        let generated = v.u8(41).field("generated flag")?;
        if generated != 1 {
            return Err(refuse("scattering generated flag", here(41), generated.into(), "1").into());
        }
        let mut c = Cursor::with_origin(frame, origin);
        c.seek(42).field("scattering tables")?;
        let mut buffer = |what: &'static str, elements: usize, stride: usize| -> Result<Vec<u8>, Error> {
            let at = c.position();
            let stored = c.u32(LE).field(what)?;
            if usize::try_from(stored) != Ok(elements) {
                return Err(refuse(what, here(at), stored.into(), "the profile's element count").into());
            }
            let bytes = elements.saturating_mul(stride);
            Ok(c.take(bytes).field(what)?.to_vec())
        };
        let key = SkyKeyframe {
            hour,
            crc: v.u32(17, LE).field("keyframe CRC")?,
            inscatter: buffer("inscatter", 65_536, 2)?,
            optical_depth: buffer("optical depth", 1_024, 2)?,
            irradiance: buffer("irradiance", 768, 4)?,
            sun_depth: buffer("sun depth", 768, 4)?,
            sun_irradiance: buffer("sun irradiance", 1_024, 4)?,
        };
        if c.remaining() != 0 {
            let left = u64::try_from(c.remaining()).unwrap_or(u64::MAX);
            return Err(refuse(
                "keyframe tail",
                here(c.position()),
                left,
                "no bytes after the tables",
            )
            .into());
        }
        Ok(key)
    }

    /// Static temporal interpolation between authored scattering tables, including the 23:00→00:00 interval.
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "finite floating point interpolation of authored tables, not byte addressing"
    )]
    pub fn radiance(
        &self,
        hour: f32,
        direction: [f32; 3],
        sun: [f32; 3],
        mie: [f32; 3],
    ) -> Result<[f32; 3], Error> {
        if !hour.is_finite() || self.keyframes.is_empty() {
            return Err(Error::NoKeyframes {
                hour,
                keyframes: self.keyframes.len(),
            });
        }
        let hour = hour.rem_euclid(24.0);
        let mut keys: Vec<_> = self.keyframes.iter().collect();
        keys.sort_by(|a, b| a.hour.total_cmp(&b.hour));
        let no_keys = Error::NoKeyframes {
            hour,
            keyframes: keys.len(),
        };
        let last = keys.len().checked_sub(1).ok_or(no_keys)?;
        let i = keys.iter().rposition(|k| k.hour <= hour).unwrap_or(last);
        let j = if i == last { 0 } else { i.saturating_add(1) };
        let (Some(left), Some(right)) = (keys.get(i), keys.get(j)) else {
            return Err(Error::NoKeyframes {
                hour,
                keyframes: keys.len(),
            });
        };
        let a = left.radiance(direction, sun, mie)?;
        if i == j {
            return Ok(a);
        }
        let start = left.hour;
        let end = right.hour + if j == 0 { 24.0 } else { 0.0 };
        let at = hour + if hour < start { 24.0 } else { 0.0 };
        let f = (at - start) / (end - start);
        let b = right.radiance(direction, sun, mie)?;
        let mut out = [0.0; 3];
        for ((v, a), b) in out.iter_mut().zip(a).zip(b) {
            *v = a + (b - a) * f;
        }
        Ok(out)
    }
}

/// IEEE-754 half to f32. No clamping to LDR: the scattering tables carry scene radiance.
#[allow(
    clippy::arithmetic_side_effects,
    reason = "IEEE-754 half conversion uses bounded exponent and floating point arithmetic"
)]
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = (bits >> 10) & 31;
    let mantissa = bits & 1023;
    match exponent {
        0 => sign * f32::from(mantissa) * 2.0f32.powi(-24),
        31 => {
            if mantissa == 0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + f32::from(mantissa) / 1024.0) * 2.0f32.powi(i32::from(exponent) - 15),
    }
}

impl SkyKeyframe {
    /// Static clear-sky slice of the stored single-scattering model. Caller supplies the sun and Mie
    /// values; the weather/time controllers, clouds, moon, Milky Way, exposure and stars are separate engine inputs.
    #[allow(
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        reason = "numeric 32x32x16 sampling stencil: coordinates clamped to fixed arrays; byte accesses use checked ranges below"
    )]
    pub fn radiance(&self, direction: [f32; 3], sun: [f32; 3], mie: [f32; 3]) -> Result<[f32; 3], Error> {
        let normalize = |v: [f32; 3]| -> Result<[f32; 3], Error> {
            let length = v.iter().map(|v| v * v).sum::<f32>().sqrt();
            if !length.is_finite() || length <= 0.0 {
                return Err(Error::Direction);
            }
            Ok(v.map(|v| v / length))
        };
        let direction = normalize(direction)?;
        let sun = normalize(sun)?;
        if mie.iter().any(|v| !v.is_finite())
            || mie[0].abs() >= 1.0
            || mie[1].abs() >= 1.0
            || !(0.0..=1.0).contains(&mie[2])
        {
            return Err(Error::Mie);
        }
        let cosine = direction
            .iter()
            .zip(sun)
            .map(|(d, s)| d * s)
            .sum::<f32>()
            .clamp(-1.0, 1.0);
        let radius = 6360.19f32;
        let distance = -radius * direction[2]
            + (radius * radius * (direction[2] * direction[2] - 1.0) + (radius + 60.0) * (radius + 60.0))
                .max(0.0)
                .sqrt();
        let low = (3.65587f32 * -0.34202).atan();
        let high = 3.65587f32.atan();
        let uvw = [
            0.015625 + ((3.65587 * sun[2].max(-0.34202)).atan() - low) / (high - low) * 0.96875,
            0.001047 * distance + 0.015625,
            (cosine + 1.0) * 0.46875 + 0.03125,
        ];
        let sizes = [32usize, 32, 16];
        let coordinates = std::array::from_fn::<_, 3, _>(|axis| {
            let value = (uvw[axis] * sizes[axis] as f32 - 0.5).clamp(0.0, (sizes[axis] - 1) as f32);
            (value.floor() as usize, value - value.floor())
        });
        let mut encoded = [0.0f32; 4];
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    let offsets = [x, y, z];
                    let weight: f32 = (0..3)
                        .map(|a| {
                            if offsets[a] == 0 {
                                1.0 - coordinates[a].1
                            } else {
                                coordinates[a].1
                            }
                        })
                        .product();
                    let p =
                        std::array::from_fn::<_, 3, _>(|a| (coordinates[a].0 + offsets[a]).min(sizes[a] - 1));
                    let at = ((p[2] * 32 + p[1]) * 32 + p[0]) * 8;
                    for (lane, value) in encoded.iter_mut().enumerate() {
                        let texel = at + lane * 2;
                        let pair = self
                            .inscatter
                            .get(texel..texel + 2)
                            .and_then(|b| b.try_into().ok())
                            .ok_or(Error::Texel {
                                at: texel,
                                length: self.inscatter.len(),
                            })?;
                        *value += half(u16::from_le_bytes(pair)) * weight;
                    }
                }
            }
        }
        let decoded = encoded.map(|value| value.powi(4));
        let pi = std::f32::consts::PI;
        let rayleigh = 3.0 / (16.0 * pi) * (1.0 + cosine * cosine);
        let phase = |g: f32| {
            3.0 / (8.0 * pi) * (1.0 - g * g) * (1.0 + cosine * cosine)
                / ((2.0 + g * g) * (1.0 + g * g - 2.0 * g * cosine).powf(1.5))
        };
        let mie_phase = phase(mie[0]) * (1.0 - mie[2]) + phase(mie[1]) * mie[2];
        let ratio = [1.0, 0.4296, 0.1752];
        let result = std::array::from_fn(|lane| {
            decoded[lane] * rayleigh
                + decoded[lane] * decoded[3] / decoded[0].max(0.0001) * ratio[lane] * mie_phase
        });
        if result.iter().any(|v| !v.is_finite()) {
            return Err(Error::NonFinite);
        }
        Ok(result)
    }
}

/// WorldAmbiance's managed references at registry-established offsets. No byte scan for coincidental ClassIDs.
pub fn ambiance(body: &[u8]) -> Result<(u64, u64), Error> {
    if body.len() != AMBIANCE_LENGTH {
        return Err(length("WorldAmbiance length", body.len(), "5277 bytes").into());
    }
    let v = View::new(body);
    let reference = |at: usize| -> Result<u64, Error> {
        let [tag, mode, a, b, c, d, e, f, g, h] = v.array::<10>(at).field("reference")?;
        if !matches!(tag, 1 | 3) {
            return Err(refuse("reference tag", at, tag.into(), "1 or 3").into());
        }
        if mode != 1 {
            return Err(refuse("reference mode", at.saturating_add(1), mode.into(), "1").into());
        }
        Ok(u64::from_le_bytes([a, b, c, d, e, f, g, h]))
    };
    Ok((reference(3)?, reference(4108)?))
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
    fn half_values_preserve_hdr_and_sign_and_bad_radiance_inputs_are_refused() {
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0x4000), 2.0);
        assert_eq!(half(0xbc00), -1.0);
        let key = SkyKeyframe {
            hour: 12.0,
            crc: 0,
            inscatter: vec![0; 131_072],
            optical_depth: vec![],
            irradiance: vec![],
            sun_depth: vec![],
            sun_irradiance: vec![],
        };
        assert_eq!(
            key.radiance([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.5, 0.99, 0.1])
                .unwrap(),
            [0.0; 3]
        );
        assert!(key.radiance([0.0; 3], [1.0, 0.0, 0.0], [0.5, 0.99, 0.1]).is_err());
        assert!(
            key.radiance([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [1.0, 0.99, 0.1])
                .is_err()
        );
    }
    #[test]
    fn the_small_sky_profile_accounts_for_every_buffer_and_rejects_a_wrong_count() {
        let mut body = vec![0; 143_994];
        body[568..572].copy_from_slice(&1u32.to_le_bytes());
        let frame = &mut body[572..];
        frame[8..12].copy_from_slice(&KEYFRAME.to_le_bytes());
        frame[12] = 1;
        frame[13..17].copy_from_slice(&12f32.to_le_bytes());
        for (index, size) in [256u32, 64, 32, 32, 16].into_iter().enumerate() {
            frame[21 + index * 4..25 + index * 4].copy_from_slice(&size.to_le_bytes());
        }
        frame[41] = 1;
        let mut at = 42;
        for (count, stride) in [(65_536u32, 2), (1_024, 2), (768, 4), (768, 4), (1_024, 4)] {
            frame[at..at + 4].copy_from_slice(&count.to_le_bytes());
            at += 4 + count as usize * stride;
        }
        assert_eq!(at, FRAME_BYTES);
        assert_eq!(StoredSky::parse(&body).unwrap().keyframes[0].hour, 12.0);
        body[572 + 42] = 1;
        assert!(StoredSky::parse(&body).is_err());
    }
}
