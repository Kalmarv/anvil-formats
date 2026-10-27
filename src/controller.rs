//! TimeOfDay/WeatherPropertyControllerData's registry-ordered entries.
//! The packed track header includes the first key's time in its upper half. No scanning of float payloads for
//! coincidental ClassIDs: each nested header, path node, key vector and trailer is consumed in order.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]
use crate::archive::{self, Archive, At};
use crate::fields::{BodyError, LE, body_error, end, refuse};
use primase_foundations::bytes::{Cursor, Field};

/// A controller's entries.
#[derive(Clone, Debug, PartialEq)]
pub struct Controller {
    /// The animated properties, in stored order.
    pub tracks: Vec<Track>,
}

/// One animated property: its path, kind, key times and key values.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// The property path's node ids (CRC-32 of each name).
    pub path: Vec<u32>,
    /// The track class ([`Controller`] reads float, vector and byte tracks).
    pub kind: u32,
    /// The key times, strictly increasing.
    pub times: Vec<u16>,
    /// One value per key; a scalar track fills the first lane.
    pub values: Vec<[f32; 4]>,
    /// The two bytes after the path's nodes, as stored.
    pub path_flags: [u8; 2],
    /// The u64 after the path flags, as stored; what it binds is not interpreted.
    pub binding: u64,
    /// The five bytes ending the track, as stored.
    pub trailer: [u8; 5],
}

/// The TimeOfDayPropertyControllerData type id.
pub const TIME_OF_DAY: u32 = 0xF0C8_5018;
/// The WeatherPropertyControllerData type id.
pub const WEATHER: u32 = 0x8870_04DC;
const ENTRY: u32 = 0x050A_3620;
const PATH: u32 = 0x3C00_B2E0;
const NODE: u32 = 0x1A87_50BA;
const FLOAT: u32 = 0xCFD6_6AAD;
const VECTOR: u32 = 0x8BE9_C203;
const BYTE: u32 = 0x5841_BD2B;

/// The most entries a controller is read with.
const MAX_ENTRIES: usize = 8192;

/// Why a controller can't be read, sampled or found.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body is short, outside the scalar-path profile this reader reads, or has bytes left over; the
    /// inner error names the field and offset.
    #[error("controller {0}")]
    Body(#[from] BodyError),
    /// A track whose key times and values don't pair up, or that has none.
    #[error("controller track: {times} key times for {values} values (one or more of each, paired)")]
    KeyVector {
        /// How many key times the track has.
        times: usize,
        /// How many key values it has.
        values: usize,
    },
    /// A track's key times do not strictly increase.
    #[error("controller track: key times are not strictly increasing")]
    UnorderedKeys,
    /// The time asked for, or the interpolated value, is not finite.
    #[error("controller track: the time or the interpolated value is not finite")]
    NonFinite,
    /// More than one track ends in the path asked for.
    #[error("controller: more than one track ends in the property {path}")]
    Ambiguous {
        /// The path asked for, joined with `/`.
        path: String,
    },
    /// A class that names no look controller.
    #[error("{class:#x}: not a look controller class (time of day {TIME_OF_DAY:#x}, weather {WEATHER:#x})")]
    NotAController {
        /// The class given.
        class: u32,
    },
    /// A resource named for a controller that has another type.
    #[error("{name}: expected controller {expected:#x}, found type {found:#x}")]
    WrongType {
        /// The entry's name.
        name: String,
        /// The controller type id that was expected.
        expected: u32,
        /// The type id the entry has.
        found: u32,
    },
    /// A look without its time-of-day controller.
    #[error("{name}: time-of-day controller absent")]
    Absent {
        /// The entry name that was looked for.
        name: String,
    },
    /// Reading the controller entry from the archive failed.
    #[error(transparent)]
    Archive(#[from] archive::Error),
}
body_error!(Error, Error::Body);

impl Track {
    /// The value at `time`: the first key's before it, the last's after the last, linear between (a byte track holds
    /// each key's value until the next).
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "finite floating point curve interpolation; no byte addressing"
    )]
    pub fn at(&self, time: f32) -> Result<[f32; 4], Error> {
        let key_vector = || Error::KeyVector {
            times: self.times.len(),
            values: self.values.len(),
        };
        if !time.is_finite() {
            return Err(Error::NonFinite);
        }
        if self.times.is_empty() || self.times.len() != self.values.len() {
            return Err(key_vector());
        }
        if self.times.windows(2).any(|pair| matches!(pair, [a,b] if a>=b)) {
            return Err(Error::UnorderedKeys);
        }
        let first = self.times.first().copied().ok_or_else(key_vector)?;
        if time <= f32::from(first) {
            return self.values.first().copied().ok_or_else(key_vector);
        }
        for (times, values) in self.times.windows(2).zip(self.values.windows(2)) {
            let ([start, end], [a, b]) = (times, values) else {
                return Err(key_vector());
            };
            if time <= f32::from(*end) {
                if self.kind == BYTE {
                    return Ok(*a);
                }
                let f = (time - f32::from(*start)) / (f32::from(*end) - f32::from(*start));
                let mut result = [0.0; 4];
                for ((out, a), b) in result.iter_mut().zip(a).zip(b) {
                    *out = a * (1.0 - f) + b * f;
                }
                if result.iter().any(|v| !v.is_finite()) {
                    return Err(Error::NonFinite);
                }
                return Ok(result);
            }
        }
        self.values.last().copied().ok_or_else(key_vector)
    }
}

/// An object header (a u64 id) and its class, which must be `class`.
fn header(c: &mut Cursor, what: &'static str, class: u32) -> Result<(), Error> {
    c.take(8).field(what)?;
    let at = c.position();
    let found = c.u32(LE).field(what)?;
    if found != class {
        return Err(refuse(what, at, found.into(), "the class the profile holds here").into());
    }
    Ok(())
}

/// Bytes the grammar fixes, refused at the first one that is not `want`'s.
fn fixed<const N: usize>(c: &mut Cursor, what: &'static str, want: [u8; N]) -> Result<(), Error> {
    let at = c.position();
    let found = c.array::<N>().field(what)?;
    if let Some((k, (&b, _))) = found.iter().zip(&want).enumerate().find(|(_, (b, w))| b != w) {
        return Err(refuse(
            what,
            at.saturating_add(k),
            b.into(),
            "the byte the scalar-path profile fixes here",
        )
        .into());
    }
    Ok(())
}

impl Controller {
    /// A controller's body: its entries in registry order, each a scalar property path and a float, vector or byte
    /// key curve, read to the body's end.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut c = Cursor::new(bytes);
        fixed(&mut c, "ManagedObject header", [1])?;
        let at = c.position();
        let count = c.u32(LE).field("entry count")?;
        let count = usize::try_from(count)
            .ok()
            .filter(|&n| n <= MAX_ENTRIES)
            .ok_or_else(|| refuse("entry count", at, count.into(), "at most 8192"))?;
        let mut tracks = Vec::with_capacity(count);
        for _ in 0..count {
            header(&mut c, "entry", ENTRY)?;
            header(&mut c, "property path", PATH)?;
            let at = c.position();
            let nodes = c.u32(LE).field("path node count")?;
            let nodes = usize::try_from(nodes)
                .ok()
                .filter(|n| (1..=32).contains(n))
                .ok_or_else(|| refuse("path node count", at, nodes.into(), "1 to 32"))?;
            let mut path = Vec::with_capacity(nodes);
            for _ in 0..nodes {
                header(&mut c, "path node", NODE)?;
                // Fixed scalar-path node profile; conditioned/indexed/solver nodes need their own grammar.
                fixed(&mut c, "path node conditions", [255; 2])?;
                fixed(&mut c, "path node conditions", [0; 8])?;
                fixed(&mut c, "path node flags", [1, 0, 0])?;
                path.push(c.u32(LE).field("path node id")?);
                fixed(&mut c, "path node index fields", [0; 25])?;
                fixed(&mut c, "path node solver", [3])?;
            }
            let path_flags = c.array().field("path flags")?;
            let binding = c.u64(LE).field("binding")?;
            c.take(8).field("track id")?;
            let at = c.position();
            let kind = c.u32(LE).field("track class")?;
            let width = match kind {
                FLOAT | BYTE => 1,
                VECTOR => 4,
                _ => {
                    return Err(
                        refuse("track class", at, kind.into(), "a float, vector or byte track").into(),
                    );
                }
            };
            let at = c.position();
            let packed = c.u32(LE).field("key count and first time")?;
            let keys = usize::from(u16::try_from(packed & 0xFFFF).unwrap_or(u16::MAX));
            if !(1..=4096).contains(&keys) {
                return Err(refuse("key count", at, (packed & 0xFFFF).into(), "1 to 4096").into());
            }
            let mut times = Vec::with_capacity(keys);
            times.push(u16::try_from(packed >> 16).unwrap_or(u16::MAX));
            let times_at = c.position();
            for _ in 1..keys {
                times.push(c.u16(LE).field("key time")?);
            }
            if let Some(k) = times.windows(2).position(|p| matches!(p, [a, b] if a >= b)) {
                let found = times.get(k.saturating_add(1)).copied().unwrap_or(0);
                return Err(refuse(
                    "key time",
                    times_at.saturating_add(k.saturating_mul(2)),
                    found.into(),
                    "above the key before it",
                )
                .into());
            }
            let mut values = Vec::with_capacity(keys);
            for _ in 0..keys {
                let mut value = [0.0; 4];
                for lane in value.iter_mut().take(width) {
                    let at = c.position();
                    *lane = if kind == BYTE {
                        f32::from(c.u8().field("key value")?)
                    } else {
                        let bits = c.u32(LE).field("key value")?;
                        let lane = f32::from_bits(bits);
                        if !lane.is_finite() {
                            return Err(refuse("key value", at, bits.into(), "a finite number").into());
                        }
                        lane
                    };
                }
                values.push(value);
            }
            tracks.push(Track {
                path,
                kind,
                times,
                values,
                path_flags,
                binding,
                trailer: c.array().field("track trailer")?,
            });
        }
        end(&c)?;
        Ok(Self { tracks })
    }
    /// Match the complete suffix, rejecting duplicate candidates instead of choosing a fog/sky property by its
    /// final name alone (several cloud and fog objects each have Albedo and Density).
    pub fn value(&self, path: &[&str], time: f32) -> Result<Option<[f32; 4]>, Error> {
        let ids: Vec<_> = path
            .iter()
            .map(|s| primase_foundations::hash::crc32(s.as_bytes()))
            .collect();
        let mut matches = self.tracks.iter().filter(|t| t.path.ends_with(&ids));
        let value = matches.next().map(|t| t.at(time)).transpose()?;
        if matches.next().is_some() {
            return Err(Error::Ambiguous { path: path.join("/") });
        }
        Ok(value)
    }
}

/// The name of a look's controller resource in the forges: `TOD_<look>` for the time of day, `WEA_<look>` for the
/// weather; `None` for a class that is no controller.
fn controller_name(class: u32, look: &str) -> Option<String> {
    match class {
        TIME_OF_DAY => Some(format!("TOD_{look}")),
        WEATHER => Some(format!("WEA_{look}")),
        _ => None,
    }
}

/// The plan's forge files that hold `name`, in search order, as the first entry of that name: nothing is decoded.
pub fn named_resource(archive: &Archive, name: &str) -> Option<At> {
    archive
        .listing()
        .find(|(_, entry)| entry.name == name)
        .map(|(forge, entry)| At {
            forge,
            entry: entry.entry,
        })
}

/// A look's authored controllers: the time-of-day one (`TOD_<look>`) and the weather one (`WEA_<look>`), which the
/// forges need not hold.
#[derive(Clone, Debug, PartialEq)]
pub struct LookControllers {
    /// The look's time-of-day controller, the entry named `TOD_` and the look's name.
    pub time_of_day: Controller,
    /// The look's weather controller (`WEA_` and the look's name), or `None` if the archive has no such entry.
    pub weather: Option<Controller>,
}

/// The authored controllers of the look `look`: the time-of-day one (`TOD_<look>`, which the atmosphere has) and the
/// weather one (`WEA_<look>`, when a forge holds it). Only the class of each resource is checked, and the
/// controller's own parse states what it can hold. The error is the reason; an absent time-of-day controller is one,
/// an absent weather controller is not.
pub fn look_controllers(archive: &mut Archive, look: &str) -> Result<LookControllers, Error> {
    let read = |archive: &mut Archive, class: u32| -> Result<Option<Controller>, Error> {
        let Some(name) = controller_name(class, look) else {
            return Err(Error::NotAController { class });
        };
        let Some(at) = named_resource(archive, &name) else {
            return Ok(None);
        };
        let resource = archive.resource(at)?;
        if resource.type_id != class {
            return Err(Error::WrongType {
                name,
                expected: class,
                found: resource.type_id,
            });
        }
        Controller::parse(&resource.body).map(Some)
    };
    let time_of_day = read(archive, TIME_OF_DAY)?.ok_or_else(|| Error::Absent {
        name: format!("TOD_{look}"),
    })?;
    let weather = read(archive, WEATHER)?;
    Ok(LookControllers { time_of_day, weather })
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
    fn packed_first_time_is_preserved_and_noon_interpolates_the_curve() {
        let mut b = vec![1];
        b.extend(1u32.to_le_bytes());
        for class in [ENTRY, PATH] {
            b.extend([0; 8]);
            b.extend(class.to_le_bytes());
        }
        b.extend(1u32.to_le_bytes());
        b.extend([0; 8]);
        b.extend(NODE.to_le_bytes());
        b.extend([255; 2]);
        b.extend([0; 8]);
        b.extend([1, 0, 0]);
        b.extend(primase_foundations::hash::crc32(b"Density").to_le_bytes());
        b.extend([0; 25]);
        b.push(3);
        b.extend([0; 2]);
        b.extend([0; 8]);
        b.extend([0; 8]);
        b.extend(FLOAT.to_le_bytes());
        b.extend(((1200u32 << 16) | 2).to_le_bytes());
        b.extend(1680u16.to_le_bytes());
        b.extend(2f32.to_le_bytes());
        b.extend(4f32.to_le_bytes());
        b.extend([0; 5]);
        let c = Controller::parse(&b).unwrap();
        assert_eq!(c.tracks[0].times, [1200, 1680]);
        assert_eq!(c.value(&["Density"], 1440.0).unwrap().unwrap()[0], 3.0);
        b.push(0);
        assert!(Controller::parse(&b).is_err());
    }
    #[test]
    fn a_look_names_its_time_of_day_and_weather_controllers() {
        assert_eq!(
            controller_name(TIME_OF_DAY, "MAD_EVENTS_Clear_Day").as_deref(),
            Some("TOD_MAD_EVENTS_Clear_Day")
        );
        assert_eq!(
            controller_name(WEATHER, "MAD_EVENTS_Clear_Day").as_deref(),
            Some("WEA_MAD_EVENTS_Clear_Day")
        );
        assert_eq!(controller_name(0x1234_5678, "look"), None);
    }
}
