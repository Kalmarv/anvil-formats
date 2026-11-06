//! Shaun White Skateboarding (PC) Animation and AnimSet objects: a clip's bone tracks with their key times and values,
//! and the sets of clip pairs that name them, read as stored.
//!
//! An Animation is a block-allocator object: its header byte is 1 ([`crate::objects::StoredObject::header`]) and the
//! object runs past its declared body, so [`Animation::parse`] takes the bytes from the body's start to the object's
//! end (the body and its trailer). Those bytes (little-endian):
//! - a table ([`TableRow`]): the bytes `02 00 00`, a u32 row count and that many rows of 12 bytes, each a class id
//!   (0, or a class of the clip's event objects such as `AnimTrackEvent` and `AudioEvent`), a u16 (0), a u16 and a
//!   u32, kept as stored;
//! - the Animation object: the managed byte (1), the id, the class ([`ANIMATION`]), the clip's length in seconds (f32),
//!   a u32 naming the clip's track layout (the clips that share the word share their mappings, for 274 of its 275
//!   values), a u32 and a u16 kept as stored, a u32 kept as stored (the clip's id plus 3 on most clips), the
//!   `AnimTrackEvent` array (a u32 count and the events, kept as bytes: [`Events`]) and a u32 kept as stored;
//! - the `AnimTrackData` pointer: tag 0, an id, the class [`ANIM_TRACK_DATA`], the layout word again, the mappings (a
//!   u32 count; each an id, the class [`MAPPING`] and the CRC-32 of the name of the bone the track at its position
//!   drives, 0 for the root's motion) and a u16;
//! - the tracks: a u32 count, one track for each mapping and in their order, each a block ([`Track`]): its tag, a u32
//!   size (the size of the track in memory), a u32 key count `k` (at least 1), the key times of the keys after the
//!   first (a u8 each, or a u16 each where bit 0 of the tag is set; the first key is at time 0) and `k` values of the
//!   width the kind (the tag shifted right by 2) gives ([`value_width`]). The size is `2 + times`, plus 2 where bit 1
//!   of the tag is set, rounded up to the kind's alignment ([`value_align`]), plus the values' bytes.
//!
//! The tracks end at the object's last byte. A key time is a whole number of 1/60 s; each track's keys rise
//! strictly, and the last key of a track falls on the clip's length times 60 (on all but 25 of the 6,593,436
//! tracks).
//!
//! The values of each kind ([`Track::values`]):
//! - kind 4: a rotation in 48 bits (the game's `PackedQuat48`), a smallest-three quaternion, decoded as the game's own
//!   unpack does ([`packed_quat48`]): three u16 words, the low 15 bits of the first two and the third as an i16 the
//!   components kept, in the order of their components, `v * f32(sqrt(2) / 32767) - f32(1 / sqrt(2))`; the index of
//!   the component left out (0 x, 1 y, 2 z, 3 w) bit 15 of the first word its low bit, of the second its high bit, the
//!   other way round from Riders Republic's kind 4 as [`crate::animation`] reads it; the left-out component the root
//!   of what the others leave of 1, 0 where they leave less. Bit 15 of the third word is 0 on every key of the
//!   install;
//! - kind 5: a rotation in 64 bits, a smallest-three quaternion laid out as Riders Republic's kind 5; kind 6: the x,
//!   y and z of a rotation as f32, w the positive root;
//! - kind 8: a translation packed in a u32: x the top 11 bits (21 to 31), y the next 11 (10 to 20), z the low 10, each
//!   two's complement, a unit a millimetre; kind 9: a translation as three i16, a unit a millimetre;
//! - kind 7: three f32, kind 10 one f32, kinds 11 and 17 a byte each, whose meaning is not read.
//!
//! A bone's translation track comes before its rotation track; the mappings' CRCs rise.
//!
//! An AnimSet's body: the managed byte (1), the id, the class ([`ANIM_SET`]), a u32 kept as stored (0), the pairs (a
//! u32 count of [`AnimPair`]s: an id, the class [`ANIM_PAIR`] and two u32 Animation ids) and a byte kept as stored.
//!
//! Every Animation and AnimSet of the 33 data forges of the PC patched build (forges of 2010-10-27) reads so, to its
//! exact end.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use primase_foundations::bytes::{Cursor, Field, FieldError};

use crate::animation::{quat_xyz, quat64};
use crate::fields::LE;

pub use crate::animation::{ANIMATION, TIME_TICKS_PER_SECOND};
/// The class id of a clip's track data (the CRC-32 of `AnimTrackData`).
pub const ANIM_TRACK_DATA: u32 = 0x0181_EFE8;
/// The class id of one track's mapping (the CRC-32 of `AnimTrackDataMapping`).
pub const MAPPING: u32 = 0x653C_AA76;
/// The class id of an animation set (the CRC-32 of `AnimSet`).
pub const ANIM_SET: u32 = 0x3198_4F55;
/// The class id of one pair of an animation set (the CRC-32 of `AnimPair`).
pub const ANIM_PAIR: u32 = 0x9DE9_A987;
/// A translation unit, in metres: a millimetre.
pub const TRANSLATION_UNIT: f32 = 0.001;

/// Why an Animation or AnimSet can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The bytes end before a field is complete.
    #[error("Animation: {0}")]
    Short(#[from] FieldError),
    /// A fixed value (the table's bytes, a managed byte, a pointer tag, a class id) is not what the layout gives.
    #[error("Animation {what}: {found:#x}, expected {expected:#x}")]
    Value {
        /// The field refused.
        what: &'static str,
        /// The value found.
        found: u64,
        /// The value this reader accepts there.
        expected: u64,
    },
    /// The `AnimTrackData` pointer that follows the events is not found.
    #[error("Animation: no AnimTrackData after the {0} events")]
    NoTrackData(u32),
    /// The track count is not the mapping count.
    #[error("Animation: {tracks} tracks for {mappings} mappings")]
    Tracks {
        /// The stored track count.
        tracks: u32,
        /// The mapping count.
        mappings: usize,
    },
    /// A track's tag selects a kind whose value width is not known.
    #[error("Animation track {track}: tag {tag:#04x} is kind {}, whose value width isn't read", tag >> 2)]
    Kind {
        /// The track's position.
        track: usize,
        /// The tag byte.
        tag: u8,
    },
    /// A track has no keys, or its size field is not the size its tag, keys and times give.
    #[error("Animation track {track}: {why}")]
    Track {
        /// The track's position.
        track: usize,
        /// What does not fit.
        why: String,
    },
    /// A packed rotation has a bit set that must be clear.
    #[error("Animation track values: {0}")]
    Values(String),
    /// Bytes follow the last track (of an Animation) or the last field (of an AnimSet).
    #[error("{0} bytes after the end")]
    Trailing(usize),
}

/// One row of an Animation's table, as stored. Its meaning is not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableRow {
    /// A class id, or 0.
    pub class: u32,
    /// A u16 (0 on every row).
    pub unknown: u16,
    /// A u16.
    pub kind: u16,
    /// A u32.
    pub count: u32,
}

/// An Animation's `AnimTrackEvent` array as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Events {
    /// The element count.
    pub count: u32,
    /// The events' bytes, from after the count to the u32 before the `AnimTrackData` pointer; their layout is not
    /// read.
    pub bytes: Vec<u8>,
}

/// An Animation as stored ([`crate::shaunwhite::animation`] for each field).
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    /// The table before the object.
    pub table: Vec<TableRow>,
    /// The Animation object's id.
    pub id: u32,
    /// The clip's length in seconds.
    pub length: f32,
    /// The word naming the clip's track layout.
    pub layout: u32,
    /// The u32 after the layout word.
    pub unknown: u32,
    /// The u16 after it.
    pub unknown_half: u16,
    /// The u32 after that (the clip's id plus 3 on most clips).
    pub unknown_id: u32,
    /// The `AnimTrackEvent` array.
    pub events: Events,
    /// The u32 between the events and the `AnimTrackData` pointer.
    pub unknown_tail: u32,
    /// The `AnimTrackData` object's id.
    pub track_data_id: u32,
    /// The `AnimTrackData` object's layout word (equal to [`Animation::layout`] on almost every clip).
    pub track_data_layout: u32,
    /// Each track's bone: the CRC-32 of its name (0 for the root's motion), the mappings in the tracks' order.
    pub mappings: Vec<u32>,
    /// The u16 after the mappings.
    pub track_data_tail: u16,
    /// The tracks, one per mapping.
    pub tracks: Vec<Track>,
}

/// One track of an Animation as stored. The bone it drives is the clip's mapping at its position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    /// The tag byte: the kind shifted left by 2, bit 0 set where the times are u16, bit 1 a flag that makes the
    /// track 2 bytes longer in memory (its meaning is not read).
    pub tag: u8,
    /// The size of the track in memory, checked against the tag, keys and times.
    pub size: u32,
    /// Each key's time in 1/60 s ([`TIME_TICKS_PER_SECOND`]), the first key's 0.
    pub times: Vec<u16>,
    /// The values, one of [`value_width`] bytes for each key, as stored; [`Track::values`] decodes them.
    pub values: Vec<u8>,
}

/// A track's values decoded ([`Track::values`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Values {
    /// Kinds 4, 5 and 6: unit quaternions `[x, y, z, w]`.
    Rotations(Vec<[f32; 4]>),
    /// Kinds 8 and 9: translations in metres.
    Translations(Vec<[f32; 3]>),
    /// Kind 7: three f32 a key, whose meaning is not read.
    Triples(Vec<[f32; 3]>),
    /// Kind 10: one f32 a key, whose meaning is not read.
    Scalars(Vec<f32>),
    /// Kinds 11 and 17: a byte a key, whose meaning is not read.
    Bytes(Vec<u8>),
}

/// The value width in bytes of a track kind (a tag shifted right by 2), `None` for a kind not seen.
pub fn value_width(kind: u8) -> Option<usize> {
    Some(match kind {
        11 | 17 => 1,
        8 | 10 => 4,
        4 | 9 => 6,
        5 => 8,
        6 | 7 => 12,
        _ => return None,
    })
}

/// The alignment of a track kind's values in memory: the width of their components.
pub fn value_align(kind: u8) -> Option<usize> {
    Some(match kind {
        11 | 17 => 1,
        4 | 9 => 2,
        5..=8 | 10 => 4,
        _ => return None,
    })
}

/// A translation packed in a u32: x bits 21 to 31, y bits 10 to 20, z bits 0 to 9, two's complement, in millimetres.
fn packed_translation(b: [u8; 4]) -> [f32; 3] {
    let u = u32::from_le_bytes(b);
    // shift each field to the top of an i32, then back: the arithmetic shift extends its sign
    let field = |top: u32, bits: u32| {
        let v = i32::from_ne_bytes((u << (32u32.saturating_sub(top).saturating_sub(bits))).to_ne_bytes());
        (v >> 32u32.saturating_sub(bits)) as f32 * TRANSLATION_UNIT
    };
    [field(21, 11), field(10, 11), field(0, 10)]
}

/// A kind 4 value, a `PackedQuat48`, as the game unpacks one: `sws.exe` (SHA-256 `c7eb65c8...8d35c4`) at 0x103F1D0,
/// and at 0x103F340 the two keys around a time for the kind 4 track sampler, both with the same arithmetic: each
/// component kept a double product with the f32 constant at 0x1967AE8 (`sqrt(2) / 32767` rounded to f32), rounded to
/// f32, less 1/sqrt(2) as an f32, in double, rounded again; the left-out one `sqrt(max(0, 1 - a^2 - b^2 - c^2))`, the
/// difference rounded to f32, the root taken in double and rounded. The index of the one left out is bit 15 of the
/// second word, as 2, plus bit 15 of the first.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the game rounds each double step to f32; the casts are those roundings"
)]
pub fn packed_quat48(b: [u8; 6]) -> [f32; 4] {
    const SCALE: f64 = f64::from_bits(0x3F06_A0CB_A000_0000);
    let [a0, a1, b0, b1, c0, c1] = b;
    let (w0, w1) = (u16::from_le_bytes([a0, a1]), u16::from_le_bytes([b0, b1]));
    let unpack = |v: f64| {
        let scaled = (v * SCALE) as f32;
        (f64::from(scaled) - f64::from(std::f32::consts::FRAC_1_SQRT_2)) as f32
    };
    let kept = [
        unpack(f64::from(w0 & 0x7FFF)),
        unpack(f64::from(w1 & 0x7FFF)),
        unpack(f64::from(i16::from_le_bytes([c0, c1]))),
    ];
    let rest = kept
        .iter()
        .fold(1.0f64, |rest, &c| rest - f64::from(c) * f64::from(c)) as f32;
    let root = if f64::from(rest) > 0.0 {
        f64::from(rest).sqrt() as f32
    } else {
        0.0
    };
    let [a, b, c] = kept;
    match (w1 >> 15, w0 >> 15) {
        (0, 0) => [root, a, b, c],
        (0, _) => [a, root, b, c],
        (_, 0) => [a, b, root, c],
        _ => [a, b, c, root],
    }
}

impl Track {
    /// The track's kind: the tag shifted right by 2.
    pub fn kind(&self) -> u8 {
        self.tag >> 2
    }

    /// The key times in seconds.
    pub fn seconds(&self) -> Vec<f32> {
        self.times
            .iter()
            .map(|&t| f32::from(t) / TIME_TICKS_PER_SECOND)
            .collect()
    }

    /// Whether the track drives a bone's rotation (kinds 4, 5 and 6).
    pub fn is_rotation(&self) -> bool {
        matches!(self.kind(), 4..=6)
    }

    /// Whether the track drives a bone's translation (kinds 8 and 9).
    pub fn is_translation(&self) -> bool {
        matches!(self.kind(), 8 | 9)
    }

    /// The values decoded (the module documentation).
    pub fn values(&self) -> Result<Values, Error> {
        let v = &self.values;
        Ok(match self.kind() {
            4 => Values::Rotations(v.as_chunks::<6>().0.iter().map(|b| packed_quat48(*b)).collect()),
            5 => Values::Rotations(
                v.as_chunks::<8>()
                    .0
                    .iter()
                    .map(|b| quat64(*b).map_err(|why| Error::Values(format!("a kind 5 value {why}"))))
                    .collect::<Result<_, _>>()?,
            ),
            6 => Values::Rotations(v.as_chunks::<12>().0.iter().map(|b| quat_xyz(*b)).collect()),
            7 => Values::Triples(
                v.as_chunks::<12>()
                    .0
                    .iter()
                    .map(|b| {
                        let [a, b, c] = b.as_chunks::<4>().0 else {
                            return [0.0; 3];
                        };
                        [a, b, c].map(|x| f32::from_le_bytes(*x))
                    })
                    .collect(),
            ),
            8 => Values::Translations(
                v.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| packed_translation(*b))
                    .collect(),
            ),
            9 => Values::Translations(
                v.as_chunks::<6>()
                    .0
                    .iter()
                    .map(|&[x0, x1, y0, y1, z0, z1]| {
                        [[x0, x1], [y0, y1], [z0, z1]]
                            .map(|h| f32::from(i16::from_le_bytes(h)) * TRANSLATION_UNIT)
                    })
                    .collect(),
            ),
            10 => Values::Scalars(
                v.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| f32::from_le_bytes(*b))
                    .collect(),
            ),
            _ => Values::Bytes(v.clone()),
        })
    }
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
/// An id and a class that must be `class`.
fn object(c: &mut Cursor<'_>, class: u32, name: &'static str) -> Result<u32, Error> {
    let id = word(c, name)?;
    expect(name, word(c, name)?, class)?;
    Ok(id)
}
/// A u32 count of items of `each` bytes that the rest holds.
fn count(c: &mut Cursor<'_>, name: &'static str, each: usize) -> Result<usize, Error> {
    Ok(c.count(LE, each).field(name)?)
}

/// The track data that follows the events: the `AnimTrackData` pointer, its layout word and its mappings.
struct TrackData {
    tail: u32,
    id: u32,
    layout: u32,
    mappings: Vec<u32>,
}

/// The track data read at the cursor: the u32 before the pointer, the pointer and its mappings; `None` where the
/// bytes there are not that.
fn track_data(c: &mut Cursor<'_>) -> Option<TrackData> {
    let tail = c.u32(LE).ok()?;
    if c.u8().ok()? != 0 {
        return None;
    }
    let id = c.u32(LE).ok()?;
    if c.u32(LE).ok()? != ANIM_TRACK_DATA {
        return None;
    }
    let layout = c.u32(LE).ok()?;
    let n = usize::try_from(c.u32(LE).ok()?).ok()?;
    if n.saturating_mul(12) > c.remaining() {
        return None;
    }
    let mut mappings = Vec::with_capacity(n);
    for _ in 0..n {
        c.u32(LE).ok()?;
        if c.u32(LE).ok()? != MAPPING {
            return None;
        }
        mappings.push(c.u32(LE).ok()?);
    }
    Some(TrackData {
        tail,
        id,
        layout,
        mappings,
    })
}

fn track(c: &mut Cursor<'_>, index: usize) -> Result<Track, Error> {
    let tag = c.u8().field("track tag")?;
    let kind = tag >> 2;
    let (Some(width), Some(align)) = (value_width(kind), value_align(kind)) else {
        return Err(Error::Kind { track: index, tag });
    };
    let size = word(c, "track size")?;
    let keys = usize::try_from(word(c, "track key count")?).unwrap_or(usize::MAX);
    let bad = |why: String| Error::Track { track: index, why };
    if keys == 0 {
        return Err(bad("no keys".into()));
    }
    let time_width = if tag & 1 == 1 { 2 } else { 1 };
    let stored = keys.saturating_sub(1);
    let times_len = stored.saturating_mul(time_width);
    let values_len = keys.saturating_mul(width);
    let head = 2usize
        .saturating_add(times_len)
        .saturating_add(if tag & 2 == 2 { 2 } else { 0 });
    let expected = head
        .div_ceil(align)
        .saturating_mul(align)
        .saturating_add(values_len);
    if usize::try_from(size).ok() != Some(expected) {
        return Err(bad(format!(
            "size {size}, where the tag, keys and times give {expected}"
        )));
    }
    let keys_bytes = c.take(times_len.saturating_add(values_len)).field("track keys")?;
    let (stored_times, values) = keys_bytes.split_at(times_len);
    let times = std::iter::once(0)
        .chain(if time_width == 2 {
            stored_times
                .as_chunks::<2>()
                .0
                .iter()
                .map(|t| u16::from_le_bytes(*t))
                .collect::<Vec<_>>()
        } else {
            stored_times.iter().map(|&t| u16::from(t)).collect()
        })
        .collect();
    let values = values.to_vec();
    Ok(Track {
        tag,
        size,
        times,
        values,
    })
}

impl Animation {
    /// An Animation read from the bytes from its body's start to its object's end, to their last byte.
    pub fn parse(bytes: &[u8]) -> Result<Animation, Error> {
        let mut c = Cursor::new(bytes);
        expect(
            "table bytes",
            u64::from(u32::from(c.u16(LE).field("table bytes")?)),
            2u64,
        )?;
        expect("table bytes", c.u8().field("table bytes")?, 0u8)?;
        let rows = count(&mut c, "table rows", 12)?;
        let mut table = Vec::with_capacity(rows);
        for _ in 0..rows {
            table.push(TableRow {
                class: word(&mut c, "table class")?,
                unknown: c.u16(LE).field("table row")?,
                kind: c.u16(LE).field("table row")?,
                count: word(&mut c, "table count")?,
            });
        }
        expect("managed byte", c.u8().field("managed byte")?, 1u8)?;
        let id = object(&mut c, ANIMATION, "Animation")?;
        let length = c.f32(LE).field("length")?;
        let layout = word(&mut c, "layout")?;
        let unknown = word(&mut c, "Animation word")?;
        let unknown_half = c.u16(LE).field("Animation half")?;
        let unknown_id = word(&mut c, "Animation word")?;
        let event_count = word(&mut c, "event count")?;
        let events_at = c.position();
        // the events' layout isn't read: the track data is the first place after them that reads as it
        let mut from = events_at;
        let (data, events) = loop {
            let mut probe = Cursor::new(bytes);
            probe.seek(from).field("track data")?;
            if let Some(data) = track_data(&mut probe) {
                let events = bytes.get(events_at..from).unwrap_or_default().to_vec();
                c.seek(probe.position()).field("track data")?;
                break (data, events);
            }
            if event_count == 0 || from >= bytes.len() {
                return Err(Error::NoTrackData(event_count));
            }
            from = from.saturating_add(1);
        };
        let track_data_tail = c.u16(LE).field("track data tail")?;
        let tracks = word(&mut c, "track count")?;
        if usize::try_from(tracks).ok() != Some(data.mappings.len()) {
            return Err(Error::Tracks {
                tracks,
                mappings: data.mappings.len(),
            });
        }
        let tracks = (0..data.mappings.len())
            .map(|i| track(&mut c, i))
            .collect::<Result<Vec<_>, _>>()?;
        if c.remaining() != 0 {
            return Err(Error::Trailing(c.remaining()));
        }
        Ok(Animation {
            table,
            id,
            length,
            layout,
            unknown,
            unknown_half,
            unknown_id,
            events: Events {
                count: event_count,
                bytes: events,
            },
            unknown_tail: data.tail,
            track_data_id: data.id,
            track_data_layout: data.layout,
            mappings: data.mappings,
            track_data_tail,
            tracks,
        })
    }

    /// The tracks with the bone each drives, in stored order.
    pub fn bone_tracks(&self) -> impl Iterator<Item = (u32, &Track)> {
        self.mappings.iter().copied().zip(&self.tracks)
    }
}

/// An AnimSet as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimSet {
    /// The AnimSet object's id.
    pub id: u32,
    /// The u32 after the class (0 on every set).
    pub unknown: u32,
    /// The pairs.
    pub pairs: Vec<AnimPair>,
    /// The last byte (0, or 1 on a few sets).
    pub flag: u8,
}

/// One pair of an AnimSet: two Animation ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimPair {
    /// The pair's id.
    pub id: u32,
    /// The first Animation's id.
    pub first: u32,
    /// The second Animation's id.
    pub second: u32,
}

impl AnimSet {
    /// An AnimSet read from its body, to its last byte.
    pub fn parse(body: &[u8]) -> Result<AnimSet, Error> {
        let mut c = Cursor::new(body);
        expect("managed byte", c.u8().field("managed byte")?, 1u8)?;
        let id = object(&mut c, ANIM_SET, "AnimSet")?;
        let unknown = word(&mut c, "AnimSet word")?;
        let n = count(&mut c, "pair count", 16)?;
        let pairs = (0..n)
            .map(|_| {
                Ok(AnimPair {
                    id: object(&mut c, ANIM_PAIR, "AnimPair")?,
                    first: word(&mut c, "pair animation")?,
                    second: word(&mut c, "pair animation")?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let flag = c.u8().field("AnimSet flag")?;
        if c.remaining() != 0 {
            return Err(Error::Trailing(c.remaining()));
        }
        Ok(AnimSet {
            id,
            unknown,
            pairs,
            flag,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `packed_quat48` is the game's own unpack bit for bit: each row's rotation is what `sws.exe`'s function at
    /// 0x103F1D0 wrote for the row's three words, run unmodified on the words (x86 emulation of the executable's
    /// mapped sections; its two-key form at 0x103F340 writes the same for every pair of the first four rows). The rows
    /// take every index of the left-out component, both ends of each word's range, a third word with bit 15 set (an
    /// i16 to the game) and words whose components leave less than nothing of 1.
    #[test]
    fn packed_quat48_is_the_games_unpack() {
        for (words, game) in [
            (
                [0x1234, 0x5678, 0x2345],
                [0x3F433AF1, 0xBF018814, 0x3E7E3C18, 0xBEA2849B],
            ),
            (
                [0x9234, 0x5678, 0x2345],
                [0xBF018814, 0x3F433AF1, 0x3E7E3C18, 0xBEA2849B],
            ),
            (
                [0x1234, 0xD678, 0x2345],
                [0xBF018814, 0x3E7E3C18, 0x3F433AF1, 0xBEA2849B],
            ),
            (
                [0x9234, 0xD678, 0x2345],
                [0xBF018814, 0x3E7E3C18, 0xBEA2849B, 0x3F433AF1],
            ),
            (
                [0xC000, 0xC000, 0x4000],
                [0x37B50000, 0x37B50000, 0x37B50000, 0x3F800000],
            ),
            (
                [0x0000, 0x0000, 0x0000],
                [0x00000000, 0xBF3504F3, 0xBF3504F3, 0xBF3504F3],
            ),
            (
                [0xFFFF, 0xFFFF, 0x7FFF],
                [0x3F3504F3, 0x3F3504F3, 0x3F3504F3, 0x00000000],
            ),
            (
                [0x7FFF, 0x0000, 0x3FFF],
                [0x3940AC39, 0x3F3504F3, 0xBF3504F3, 0xB7B50000],
            ),
            (
                [0x3A5C, 0xB1E7, 0x46D2],
                [0xBD7F2F60, 0xBE1F7AC4, 0x3F7BA24B, 0x3D9A6000],
            ),
            (
                [0x4000, 0x4000, 0xC000],
                [0x00000000, 0x37B50000, 0x37B50000, 0xBFB505A8],
            ),
            (
                [0x8001, 0x7FFE, 0x8000],
                [0xBF35021F, 0x00000000, 0x3F35021F, 0xC007C46B],
            ),
            (
                [0xE5A3, 0x9C40, 0x2B17],
                [0x3ED4EC46, 0xBECA3A48, 0xBE6C8E78, 0x3F49329C],
            ),
        ] {
            let [a, b, c] = words.map(u16::to_le_bytes);
            let bytes = [a[0], a[1], b[0], b[1], c[0], c[1]];
            assert_eq!(packed_quat48(bytes).map(f32::to_bits), game, "{words:04X?}");
        }
    }
}
