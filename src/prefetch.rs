//! A forge's `PrefetchingFileInfos` entry: for every other entry of the forge, a record of the entries it lists. Read
//! from the six Riders Republic forges (build 25394987), each of which ends with one; Shaun White Skateboarding's
//! forges have none. What the game does with a record is not established (the name suggests the entries to fetch
//! with the one it is for).
//!
//! The entry is not two data blocks like a resource's but one streamed block (version 2, algorithm 0: LZO1X), its
//! chunks running to the entry's end ([`crate::data::read_stream_to_end`]). Decoded, little-endian:
//! - the record count `n` (u32), then `n` rows of 16 bytes: an entry's index id (u64), its record's size and its
//!   record's offset (u32 each) in the record bytes; the ids ascend, and each offset is where the record before it
//!   ends (the first 0);
//! - the record bytes' length (u32), which is the records' sizes added up, then the record bytes, ending the data;
//! - a record: a group count (u16), each group a member count (u16) and that many members of 9 bytes (a byte, then an
//!   entry's index id: u64), then a listed-entry count (u16), each listed entry 11 bytes or more (an entry's index id:
//!   u64, a byte, a value count: u16, then that many u16 values), ending at the record's size.
//!
//! Established by the census of the six forges: each lists every entry of its forge but the `GlobalMetaFile` and
//! itself (163,354 in `DataPC.forge`, 767,461 in `DataPC_MAD_USNAT.forge`), every record reads to its size, and every
//! id a record holds is the index id of an entry in one of the six forges (a patch's records list base-forge entries
//! too). Observed there and not interpreted: groups only in GridCellDataBlocks' records (6,675 and 4,898 in the two
//! USNAT forges, two in `DataPC_MAD_WhiteRoom.forge`), their members entries of type 0x41813744 named after the
//! game's activities (`EVE_...`, `TUT_...`, `HUB_...`), with the byte 0 or 1; the listed entry's byte 1 or 4; values
//! only in GridCellDataBlocks' records, on the Entities and EntityGroups they list.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::data;
use crate::fields::{BodyError, LE, body_error, end, refuse};
use primase_foundations::bytes::{Cursor, Field};

/// The name of a forge's entry that holds the records.
pub const NAME: &str = "PrefetchingFileInfos";

/// Why a `PrefetchingFileInfos` entry can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Its block does not read ([`data::read_stream_to_end`]).
    #[error("prefetching file infos: {0}")]
    Block(#[from] data::Error),
    /// The decoded bytes are short or break the layout; the inner error names the field and offset.
    #[error("prefetching file infos {0}")]
    Body(#[from] BodyError),
}
body_error!(Error, Error::Body);

/// A `PrefetchingFileInfos` entry decoded: one record per entry of the forge, in ascending order of the entries' ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrefetchingFileInfos {
    /// The records, by ascending [`Record::id`].
    pub records: Vec<Record>,
}

/// The record for one entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// The index id of the entry it is for.
    pub id: u64,
    /// Its groups, in stored order (observed only on GridCellDataBlocks).
    pub groups: Vec<Vec<Member>>,
    /// The entries it lists, in stored order.
    pub listed: Vec<Listed>,
}

/// A member of a record's group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    /// The byte before the id, as stored (observed 0 or 1).
    pub flag: u8,
    /// An entry's index id.
    pub id: u64,
}

/// An entry a record lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// The entry's index id.
    pub id: u64,
    /// The byte after the id, as stored (observed 1 or 4).
    pub kind: u8,
    /// The u16 values after it, as stored (observed only on Entities and EntityGroups a GridCellDataBlock lists).
    pub values: Vec<u16>,
}

/// One row of the table: the record's id, size and offset.
struct Row {
    id: u64,
    size: usize,
    offset: usize,
}

impl PrefetchingFileInfos {
    /// The entry's payload (as [`crate::forge::Forge::payload`] gives it) read: its block decoded, its decoded size
    /// bounded by `max_decoded`, and the records parsed.
    pub fn read(payload: &[u8], max_decoded: usize) -> Result<Self, Error> {
        let block = data::read_stream_to_end(payload, 0, max_decoded)?;
        Self::parse(&block.data)
    }

    /// The records of the entry's decoded bytes.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut c = Cursor::new(data);
        let n = c.count(LE, 16).field("record count")?;
        let mut rows = Vec::with_capacity(n);
        let mut total = 0usize;
        for _ in 0..n {
            let at = c.position();
            let id = c.u64(LE).field("record id")?;
            let size = c.u32(LE).field("record size")?;
            let offset = c.u32(LE).field("record offset")?;
            if rows.last().is_some_and(|r: &Row| r.id >= id) {
                return Err(refuse("record id", at, id, "above the one before it").into());
            }
            if widen(offset) != total {
                let read = "where the record before it ends";
                return Err(refuse("record offset", at.saturating_add(12), offset.into(), read).into());
            }
            total = total.checked_add(widen(size)).ok_or_else(|| {
                refuse(
                    "record size",
                    at.saturating_add(8),
                    size.into(),
                    "a size the bytes can hold",
                )
            })?;
            rows.push(Row {
                id,
                size: widen(size),
                offset: widen(offset),
            });
        }
        let at = c.position();
        let stated = c.u32(LE).field("record bytes length")?;
        let length = widen(stated);
        if length != total {
            return Err(refuse(
                "record bytes length",
                at,
                stated.into(),
                "the records' sizes added up",
            )
            .into());
        }
        let bytes = c.take(length).field("record bytes")?;
        end(&c)?;
        let base = c.position().saturating_sub(length);
        let records = rows
            .iter()
            .map(|row| {
                let span = bytes
                    .get(row.offset..row.offset.saturating_add(row.size))
                    .ok_or_else(|| refuse("record offset", base, 0, "within the record bytes"))?;
                record(row.id, span, base.saturating_add(row.offset))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { records })
    }

    /// The record for the entry with index id `id`, if there is one.
    pub fn get(&self, id: u64) -> Option<&Record> {
        let k = self.records.binary_search_by_key(&id, |r| r.id).ok()?;
        self.records.get(k)
    }
}

/// A u32 as a usize (every target this crate builds for holds one).
fn widen(n: u32) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// The record for entry `id` in `span`, which starts at `origin` in the decoded bytes.
fn record(id: u64, span: &[u8], origin: usize) -> Result<Record, Error> {
    let mut c = Cursor::with_origin(span, origin);
    let group_count = c.u16(LE).field("group count")?;
    let mut groups = Vec::new();
    for _ in 0..group_count {
        let members = c.u16(LE).field("member count")?;
        let mut group = Vec::new();
        for _ in 0..members {
            let flag = c.u8().field("member flag")?;
            let id = c.u64(LE).field("member id")?;
            group.push(Member { flag, id });
        }
        groups.push(group);
    }
    let count = c.u16(LE).field("listed count")?;
    let mut listed = Vec::new();
    for _ in 0..count {
        let id = c.u64(LE).field("listed id")?;
        let kind = c.u8().field("listed kind")?;
        let n = c.u16(LE).field("value count")?;
        let mut values = Vec::new();
        for _ in 0..n {
            values.push(c.u16(LE).field("value")?);
        }
        listed.push(Listed { id, kind, values });
    }
    end(&c)?;
    Ok(Record { id, groups, listed })
}
