//! Impostor resources (RR build 25394987). Layout evidence: the Riders Republic survey of the far-view data.
//! The four references are owned by the Impostor, not by its proxy Mesh's Material.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::fields::{BodyError, LE, body_error, end, f32s, length, refuse};
use crate::reference::{self, Reference};
use primase_foundations::bytes::{Cursor, Field};

/// An Impostor's stored cards and the references it owns.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredImpostor {
    /// The first 71 bytes, as stored.
    pub head: [u8; 71],
    /// The four texture references, in stored order.
    pub maps: [Option<Reference>; 4],
    /// The 21-word table before the cards; `table[0]` and `table[1]` are the top and side card counts, `table[14]`
    /// the coverage cutoff's bits.
    pub table: [u32; 21],
    /// How many top cards (0 or 1).
    pub top: usize,
    /// How many side cards (1 to 7).
    pub side: usize,
    /// The cards, 27 f32 each, top cards first.
    pub cards: Vec<[f32; 27]>,
}

/// The Impostor type id.
pub const IMPOSTOR: u32 = 0xDFCD_6E51;

/// A card's size in bytes.
const CARD: usize = 108;
/// Where the cards start: after the head, four references, a zero word and the 21-word table.
const CARDS_AT: usize = 199;
/// Where the 21-word table starts.
const TABLE_AT: usize = 115;

/// Why an Impostor body can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The body is short, malformed or fails one of the Impostor's checks; the inner error names the field
    /// and offset.
    #[error("Impostor {0}")]
    Body(#[from] BodyError),
}
body_error!(Error, Error::Body);

impl StoredImpostor {
    /// An Impostor's body: head, four references, the table, then the cards its counts state, to the body's end.
    pub fn parse(body: &[u8]) -> Result<Self, Error> {
        let mut c = Cursor::new(body);
        let head = c.array::<71>().field("header")?;
        let mut maps = [None; 4];
        for map in &mut maps {
            let at = c.position();
            *map = reference::parse(c.array().field("map reference")?)
                .map_err(|source| BodyError::Reference { at, source })?;
        }
        let at = c.position();
        let zero = c.u32(LE).field("word before the GPU table")?;
        if zero != 0 {
            return Err(refuse("word before the GPU table", at, zero.into(), "0").into());
        }
        let mut table = [0u32; 21];
        for word in &mut table {
            *word = c.u32(LE).field("GPU table")?;
        }
        let [top_count, side_count, .., cutoff, _, _, _, _, _, _] = table;
        let top = match usize::try_from(top_count) {
            Ok(top @ 0..=1) => top,
            _ => return Err(refuse("top card count", TABLE_AT, top_count.into(), "0 or 1").into()),
        };
        let side = match usize::try_from(side_count) {
            Ok(side @ 1..=7) => side,
            _ => {
                return Err(refuse(
                    "side card count",
                    TABLE_AT.saturating_add(4),
                    side_count.into(),
                    "1 to 7",
                )
                .into());
            }
        };
        let count = top.saturating_add(side);
        let want = count.saturating_mul(CARD).saturating_add(CARDS_AT);
        if body.len() != want {
            return Err(length("Impostor length", body.len(), "199 bytes and 108 per card").into());
        }
        let cutoff_at = TABLE_AT.saturating_add(14 * 4);
        let cutoff = f32::from_bits(cutoff);
        if !(0.0..=1.0).contains(&cutoff) {
            return Err(refuse("coverage cutoff", cutoff_at, cutoff.to_bits().into(), "in [0, 1]").into());
        }
        let mut cards = Vec::with_capacity(count);
        for _ in 0..count {
            let at = c.position();
            let card = f32s::<27>(&mut c, "card")?;
            if let Some((lane, v)) = card.iter().enumerate().find(|(_, v)| !v.is_finite()) {
                return Err(refuse(
                    "card value",
                    at.saturating_add(lane.saturating_mul(4)),
                    v.to_bits().into(),
                    "a finite number",
                )
                .into());
            }
            for (lane, name) in [(21usize, "atlas width"), (22, "atlas height")] {
                if let Some(&v) = card.get(lane).filter(|&&v| v <= 0.0) {
                    return Err(refuse(
                        name,
                        at.saturating_add(lane.saturating_mul(4)),
                        v.to_bits().into(),
                        "above zero",
                    )
                    .into());
                }
            }
            cards.push(card);
        }
        end(&c)?;
        Ok(Self {
            head,
            maps,
            table,
            top,
            side,
            cards,
        })
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

    fn fixture() -> Vec<u8> {
        let mut body = vec![0; 199 + 108];
        for index in 0..4 {
            body[71 + index * 10] = 1;
            body[72 + index * 10] = 1;
            body[73 + index * 10..81 + index * 10].copy_from_slice(&(100 + index as u64).to_le_bytes());
        }
        body[119..123].copy_from_slice(&1u32.to_le_bytes());
        body[171..175].copy_from_slice(&0.5f32.to_le_bytes());
        body[199 + 84..199 + 88].copy_from_slice(&1f32.to_le_bytes());
        body[199 + 88..199 + 92].copy_from_slice(&1f32.to_le_bytes());
        body
    }

    #[test]
    fn cards_and_owned_texture_references_are_read_and_malformed_counts_refused() {
        let mut bytes = fixture();
        let parsed = StoredImpostor::parse(&bytes).unwrap();
        assert_eq!(parsed.maps[1].unwrap().id, 101);
        assert_eq!((parsed.top, parsed.side, parsed.cards.len()), (0, 1, 1));
        assert_eq!(f32::from_bits(parsed.table[14]), 0.5);
        bytes.pop();
        assert!(
            StoredImpostor::parse(&bytes)
                .unwrap_err()
                .to_string()
                .contains("bytes")
        );
        let mut bytes = fixture();
        bytes[119..123].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(StoredImpostor::parse(&bytes).is_err());
    }
}
