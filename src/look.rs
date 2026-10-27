//! Event look presets. Decode stored values; the second preset scalar and forecast lane roles
//! are not proven sun/wind/fog inputs. Their names alone don't say how a renderer applies them.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::fields::{BodyError, LE, body_error, refuse};
use crate::reference::{self, Reference};
use primase_foundations::bytes::{Field, View};

/// An AmbianceAndTimeOfDayPreset or a ForecastPreset, as stored.
#[derive(Clone, Debug, PartialEq)]
pub enum StoredLook {
    /// An ambiance and time-of-day preset.
    Ambiance {
        /// The season's ClassID.
        season: u64,
        /// The forecast preset it refers to, if any.
        forecast: Option<Reference>,
        /// The hour of day, in `[0, 24)`.
        hour: f32,
        /// An angle in degrees, in `[0, 360]`; what it turns is not established.
        angle: f32,
        /// The byte ending the body, as stored.
        flag: u8,
    },
    /// A forecast preset: twelve lanes whose roles are not established.
    Forecast {
        /// The twelve f32 lanes at offsets 1 to 48, finite; their roles are not established.
        values: [f32; 12],
    },
}
/// The AmbianceAndTimeOfDayPreset type id.
pub const AMBIANCE_PRESET: u32 = 0xE8EC_CA01;
/// The ForecastPreset type id.
pub const FORECAST_PRESET: u32 = 0x79B0_E782;

/// Why a look preset body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body is short or fails one of the preset's checks (managed header, hour or angle range); the inner
    /// error names the field and offset.
    #[error("look preset {0}")]
    Body(#[from] BodyError),
    /// The body's type id and length is none of the two profiles read: an AmbianceAndTimeOfDayPreset of 29
    /// bytes or a ForecastPreset of 49 bytes.
    #[error("look preset: type {type_id:#010x} with a {length}-byte body is not a profile read here")]
    Unsupported {
        /// The type id that was passed.
        type_id: u32,
        /// The body's length in bytes.
        length: usize,
    },
}
body_error!(Error, Error::Body);

impl StoredLook {
    /// The preset a resource of type `type_id` (`AMBIANCE_PRESET` or `FORECAST_PRESET`) holds in `body`.
    pub fn parse(type_id: u32, body: &[u8]) -> Result<Self, Error> {
        let v = View::new(body);
        let managed = v.u8(0).field("managed header")?;
        if managed != 1 {
            return Err(refuse("managed header", 0, managed.into(), "1").into());
        }
        let float = |at: usize, what: &'static str| -> Result<f32, Error> {
            let bits = v.u32(at, LE).field(what)?;
            let value = f32::from_bits(bits);
            if value.is_finite() {
                Ok(value)
            } else {
                Err(refuse(what, at, bits.into(), "a finite number").into())
            }
        };
        match (type_id, body.len()) {
            (AMBIANCE_PRESET, 29) if body.get(1) == Some(&0) => {
                let season = v.u64(2, LE).field("season reference")?;
                let forecast = reference::parse(v.array(10).field("forecast reference")?)
                    .map_err(|source| BodyError::Reference { at: 10, source })?;
                let hour = float(20, "hour")?;
                let angle = float(24, "angle")?;
                if !(0.0..24.0).contains(&hour) {
                    return Err(refuse("hour", 20, hour.to_bits().into(), "in [0, 24)").into());
                }
                if !(0.0..=360.0).contains(&angle) {
                    return Err(refuse("angle", 24, angle.to_bits().into(), "in [0, 360]").into());
                }
                Ok(Self::Ambiance {
                    season,
                    forecast,
                    hour,
                    angle,
                    flag: v.u8(28).field("flag")?,
                })
            }
            (FORECAST_PRESET, 49) => {
                let mut values = [0.; 12];
                for (value, at) in values.iter_mut().zip((1usize..).step_by(4)) {
                    *value = float(at, "forecast lane")?;
                }
                Ok(Self::Forecast { values })
            }
            _ => Err(Error::Unsupported {
                type_id,
                length: body.len(),
            }),
        }
    }
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
    fn references_hour_and_unknown_angle_are_kept_separate_and_lengths_are_exact() {
        let mut body = vec![1, 0];
        body.extend(17u64.to_le_bytes());
        body.extend([3, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        body.extend(7f32.to_le_bytes());
        body.extend(135f32.to_le_bytes());
        body.push(0);
        assert!(matches!(
            StoredLook::parse(AMBIANCE_PRESET, &body),
            Ok(StoredLook::Ambiance {
                season: 17,
                forecast: None,
                hour: 7.0,
                angle: 135.0,
                flag: 0
            })
        ));
        body.push(0);
        assert!(StoredLook::parse(AMBIANCE_PRESET, &body).is_err());
        let mut forecast = vec![1];
        forecast.extend([0; 48]);
        assert!(
            matches!(StoredLook::parse(FORECAST_PRESET, &forecast), Ok(StoredLook::Forecast { values }) if values == [0.; 12])
        );
    }
}
