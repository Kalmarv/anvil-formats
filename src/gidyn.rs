//! GIDynResource's established one-volume/one-box profile (RR, 418 bodies).
//! Preserve all four encoded streams. Probe positions are established; irradiance, visibility bit semantics,
//! sample origin and the GPU GIDynData texture construction are not. Nothing here invents probe colours.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::fields::{BodyError, LE, body_error, end, refuse};
use primase_foundations::bytes::{Cursor, Field};

/// A GIDynResource's one volume, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredGiDyn {
    pub world_id: u64,
    pub cell_id: u32,
    /// The PositionOffset the probe positions are relative to.
    pub position: [f32; 3],
    pub samples: u32,
    pub probes: u32,
    pub indoor: bool,
    /// The one box's low and high voxel corners.
    pub voxel_bounds: [[i32; 3]; 2],
    pub subvolume_word: u32,
    /// The four encoded streams, as stored.
    pub streams: [Vec<u8>; 4],
    pub decoded_bytes: [u32; 3],
}

/// The GIDynResource type id.
pub const GI_DYN_RESOURCE: u32 = 0xBAE9_8B30;
const VOLUME: u32 = 0x684A_17AD;
const DESC: u32 = 0xCE0C_489A;
const SUB: u32 = 0xACC1_B118;
const BOX: u32 = 0xE96B_B4A2;

/// Why a GIDynResource body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("GIDynResource {0}")]
    Body(#[from] BodyError),
}
body_error!(Error, Error::Body);

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

/// Validate an encoded stream (at `origin` in the body) without allocating its decoded size. The u16 grid's flag bit
/// is retained.
fn rle(
    data: &[u8],
    origin: usize,
    what: &'static str,
    width: usize,
    expected: u64,
    probes: Option<u32>,
) -> Result<(), Error> {
    let mut c = Cursor::with_origin(data, origin);
    let mut total = 0u64;
    let escape = if width == 1 { 0xFE } else { 0xFFFE };
    // a cursor over the stream counts from the stream's start: the body's offset adds the origin
    let here = |c: &Cursor<'_>| origin.saturating_add(c.position());
    let value = |c: &mut Cursor<'_>| -> Result<u32, Error> {
        match c.take(width).field(what)? {
            [a] => Ok(u32::from(*a)),
            [a, b] => Ok(u32::from(u16::from_le_bytes([*a, *b]))),
            _ => Err(refuse(what, here(c), 0, "a 1 or 2 byte word").into()),
        }
    };
    while c.remaining() != 0 {
        let at = here(&c);
        let first = value(&mut c)?;
        let (count, word) = if first == escape {
            (value(&mut c)?, value(&mut c)?)
        } else {
            (1, first)
        };
        if count == 0 {
            return Err(refuse(what, at, 0, "a run of at least one value").into());
        }
        if let Some(probes) = probes
            && word != 0xFFFF
            && (word & 0x7FFF) >= probes
        {
            return Err(refuse(what, at, (word & 0x7FFF).into(), "a probe below the probe count").into());
        }
        total = total
            .checked_add(u64::from(count))
            .filter(|&t| t <= expected)
            .ok_or_else(|| {
                refuse(
                    what,
                    at,
                    u64::from(count),
                    "runs that stay within the decoded size",
                )
            })?;
    }
    if total != expected {
        return Err(refuse(what, origin, total, "as many values as the sizes state").into());
    }
    Ok(())
}

#[allow(
    clippy::arithmetic_side_effects,
    reason = "IEEE-754 half conversion has bounded exponent and floating point arithmetic"
)]
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 31;
    let fraction = f32::from(bits & 1023);
    sign * match exponent {
        0 => fraction * 2f32.powi(-24),
        31 => {
            if fraction == 0.0 {
                f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => (1.0 + fraction / 1024.0) * 2f32.powi(i32::from(exponent) - 15),
    }
}

impl StoredGiDyn {
    /// A GIDynResource's body in the one-volume, one-box profile, every stream checked to decode to the sizes the
    /// footer states.
    pub fn parse(body: &[u8]) -> Result<Self, Error> {
        let mut c = Cursor::new(body);
        let at = c.position();
        let managed = c.u8().field("managed header")?;
        if managed != 1 {
            return Err(refuse("managed header", at, managed.into(), "1").into());
        }
        let world_id = c.u64(LE).field("world id")?;
        let cell_id = c.u32(LE).field("cell id")?;
        let at = c.position();
        let volumes = c.u32(LE).field("volume count")?;
        if volumes != 1 {
            return Err(refuse("volume count", at, volumes.into(), "1 (the one-volume profile)").into());
        }
        header(&mut c, "GIDynVolume", VOLUME)?;
        header(&mut c, "GIDynDesc", DESC)?;
        let at = c.position();
        let position = [
            f32::from_bits(c.u32(LE).field("PositionOffset")?),
            f32::from_bits(c.u32(LE).field("PositionOffset")?),
            f32::from_bits(c.u32(LE).field("PositionOffset")?),
        ];
        if !position.iter().all(|x| x.is_finite()) {
            return Err(refuse("PositionOffset", at, 0, "finite numbers").into());
        }
        let samples = c.u32(LE).field("sample count")?;
        let probes = c.u32(LE).field("probe count")?;
        let at = c.position();
        let indoor = match c.u8().field("IsIndoor")? {
            0 => false,
            1 => true,
            other => return Err(refuse("IsIndoor", at, other.into(), "0 or 1").into()),
        };
        let at = c.position();
        let subvolumes = c.u32(LE).field("subvolume count")?;
        if subvolumes != 1 {
            return Err(refuse(
                "subvolume count",
                at,
                subvolumes.into(),
                "1 (the one-box profile)",
            )
            .into());
        }
        header(&mut c, "GIDynSubVolume", SUB)?;
        header(&mut c, "GIDynBox", BOX)?;
        let box_at = c.position();
        let mut voxel_bounds = [[0; 3]; 2];
        for corner in &mut voxel_bounds {
            for axis in corner {
                *axis = c.i32(LE).field("voxel box")?;
            }
        }
        let mut voxels = 1u64;
        let [lo, hi] = voxel_bounds;
        for (lo, hi) in lo.into_iter().zip(hi) {
            let extent = i64::from(hi).saturating_sub(i64::from(lo));
            let extent = u64::try_from(extent)
                .ok()
                .filter(|&e| e != 0)
                .ok_or_else(|| refuse("voxel box", box_at, 0, "a high corner above the low corner"))?;
            voxels = voxels
                .checked_mul(extent)
                .ok_or_else(|| refuse("voxel box", box_at, extent, "a voxel count that fits 64 bits"))?;
        }
        let subvolume_word = c.u32(LE).field("subvolume word")?;
        let mut streams: [Vec<u8>; 4] = std::array::from_fn(|_| Vec::new());
        let mut stream_at = [0usize; 4];
        for (stream, start) in streams.iter_mut().zip(&mut stream_at) {
            // a stream's size is bounded by the bytes that remain before it is copied
            let n = c.count(LE, 1).field("stream size")?;
            *start = c.position();
            *stream = c.take(n).field("stream")?.to_vec();
        }
        let decoded_bytes = [
            c.u32(LE).field("decoded size")?,
            c.u32(LE).field("decoded size")?,
            c.u32(LE).field("decoded size")?,
        ];
        end(&c)?;
        let footer_at = c.position().saturating_sub(12);
        if Some(u64::from(decoded_bytes[0])) != u64::from(samples).checked_mul(10) {
            return Err(refuse(
                "footer sample bytes",
                footer_at,
                decoded_bytes[0].into(),
                "10 per sample",
            )
            .into());
        }
        if Some(u64::from(decoded_bytes[2])) != voxels.checked_mul(2) {
            return Err(refuse(
                "footer grid bytes",
                footer_at.saturating_add(8),
                decoded_bytes[2].into(),
                "2 per voxel of the box",
            )
            .into());
        }
        let mask_bytes = u64::from(probes)
            .checked_mul(24)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| refuse("probe count", 0, probes.into(), "a count whose masks fit 64 bits"))?;
        if u64::from(decoded_bytes[1]) < mask_bytes {
            return Err(refuse(
                "footer V2 bytes",
                footer_at.saturating_add(4),
                decoded_bytes[1].into(),
                "at least its two-word header and 24 bytes of direction masks per probe",
            )
            .into());
        }
        let [s0, s1, s2, s3] = &streams;
        let [a0, a1, a2, a3] = stream_at;
        rle(
            s0,
            a0,
            "sample stream",
            2,
            u64::from(samples).saturating_mul(5),
            None,
        )?;
        rle(s1, a1, "V2 stream", 1, u64::from(decoded_bytes[1]), None)?;
        rle(s2, a2, "grid stream", 2, voxels, Some(probes))?;
        if u64::try_from(s3.len()).ok() != u64::from(probes).checked_mul(8) {
            return Err(refuse(
                "probe vector",
                a3,
                u64::try_from(s3.len()).unwrap_or(u64::MAX),
                "8 bytes per probe",
            )
            .into());
        }
        let result = Self {
            world_id,
            cell_id,
            position,
            samples,
            probes,
            indoor,
            voxel_bounds,
            subvolume_word,
            streams,
            decoded_bytes,
        };
        for (k, bytes) in result.probe_vectors().iter().enumerate() {
            let [_, _, _, _, _, _, a, b] = *bytes;
            if [a, b] != [0, 0] {
                return Err(refuse(
                    "probe fourth lane",
                    a3.saturating_add(k.saturating_mul(8)).saturating_add(6),
                    u64::from(u16::from_le_bytes([a, b])),
                    "0 (the established profile)",
                )
                .into());
            }
        }
        if result.probe_positions().flatten().any(|x| !x.is_finite()) {
            return Err(refuse("probe position", a3, 0, "finite numbers").into());
        }
        Ok(result)
    }

    /// The probe vector stream's 8-byte records.
    fn probe_vectors(&self) -> &[[u8; 8]] {
        let [.., probes] = &self.streams;
        probes.as_chunks::<8>().0
    }

    /// Source coordinates in metres; the fourth half lane is validated separately. No voxel-to-metre guess.
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "floating point probe offsets; exact eight-byte records destructured without index arithmetic"
    )]
    pub fn probe_positions(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.probe_vectors().iter().map(|b| {
            let [a, b, c, d, e, f, _, _] = *b;
            let [x, y, z] = self.position;
            [
                x + half(u16::from_le_bytes([a, b])),
                y + half(u16::from_le_bytes([c, d])),
                z + half(u16::from_le_bytes([e, f])),
            ]
        })
    }
}
