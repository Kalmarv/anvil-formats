//! A forge entry's payload: its compressed data blocks read, checked and decoded, and the resource they hold parsed
//! as far as its header (type, name, class and extension); the resource's body is kept as stored, not decoded.
//!
//! A data block (little-endian; offsets from the block's start):
//! - magic 0x1004FA9957FBAA33 (u64);
//! - the compression info, 11 bytes: version (u16), algorithm (u8), two u16 (observed 0 and 32772), chunk count
//!   (u32); in version 1, 9 bytes: the chunk count is a u16 (the two u16 observed 32768 and 32768);
//! - every chunk's decoded size and stored size (u32 each; u16 each in version 1), in order;
//! - then for each chunk in order: its checksum (u32: Adler-32 started from 0 over its stored bytes) and its stored
//!   bytes.
//!
//! Read here: version 2, algorithm 5 (zstd: established on Mesh resources and a three-chunk CompiledMip, Riders
//! Republic build 25394987), version 2, algorithm 0 (LZO1X: every chunk of the six Riders Republic forges'
//! `PrefetchingFileInfos` entries, streamed blocks, each checksum matching and each chunk decoding to exactly its stated
//! size), and version 1, algorithm 2 (LZO2A: every chunk of Shaun White Skateboarding's PC forges, algorithm 2 as
//! AnvilToolkit maps it for the Assassin's Creed 2 era); a chunk whose stored size equals its decoded size is stored
//! raw. The block's data is its
//! chunks' decoded bytes in order. An entry is two consecutive blocks: a metadata block (16 bytes decoded, kept as
//! stored) and the resource's. Anything else is refused, naming what it
//! found. A forge's `GlobalMetaFile` entry has no block magic: it is not data blocks, and is refused as such.
//!
//! A streamed block has the same magic and compression info but no chunk count or table: each chunk carries its own
//! sizes and checksum before its bytes. Shaun White Skateboarding's compressed shader databases hold one of a known
//! decoded size ([`read_stream`]); a Riders Republic forge's `PrefetchingFileInfos` entry is one block alone, its chunks
//! running to the entry's end ([`read_stream_to_end`], [`crate::prefetch`]). Observed on every block of both games: the
//! second u16 after the algorithm has bit 15 set where the block has a chunk table (32768, 32772) and clear where it is
//! streamed (0, 4).
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use primase_foundations::bytes::{Field, FieldError, View};
use primase_foundations::compress::{Decompressor, Lzo1x, Lzo2a, Zstd};

use crate::fields::LE;

/// A data block's first bytes.
pub const BLOCK_MAGIC: u64 = 0x1004_FA99_57FB_AA33;
/// The compression version read with zstd.
pub const VERSION: u16 = 2;
/// The compression version read with LZO2A.
pub const VERSION_1: u16 = 1;
/// Version 2's algorithm read with zstd.
pub const ALGORITHM_ZSTD: u8 = 5;
/// Version 2's algorithm read with LZO1X (miniLZO's format).
pub const ALGORITHM_LZO1X: u8 = 0;
/// Version 1's algorithm read (LZO2A).
pub const ALGORITHM_LZO2A: u8 = 2;
/// The checksum's starting value.
pub const CHECKSUM_INIT: u32 = 0;
/// The byte before each chunk of a streamed block ([`read_stream`]).
pub const STREAM_MARKER: u8 = 1;
/// The size of a streamed block's header: the magic and the compression info.
const STREAM_HEADER: usize = 15;

/// Why a payload can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The block does not start with the 8-byte block magic; `found` is the value read at `at`.
    #[error("block at {at}: no block magic (found {found:016x}): not a data block")]
    Magic {
        /// The offset of the block in the entry.
        at: usize,
        /// The eight bytes read there, as a little-endian u64.
        found: u64,
    },
    /// The compression version at offset 8 of the block is neither 1 nor 2.
    #[error("block at {at}: compression version {version}: only versions 1 and 2 are read")]
    Version {
        /// The offset of the block in the entry.
        at: usize,
        /// The compression version found.
        version: u16,
    },
    /// The version and algorithm byte (offset 10) are not a pair read: version 2 with 5 (zstd) or 0 (LZO1X), or
    /// version 1 with 2 (LZO2A).
    #[error(
        "block at {at}: compression version {version}, algorithm {algorithm}: only 2 with 5 (zstd) or 0 (LZO1X) and 1 with 2 (LZO2A) are read"
    )]
    Algorithm {
        /// The offset of the block in the entry.
        at: usize,
        /// The compression version found.
        version: u16,
        /// The algorithm byte found.
        algorithm: u8,
    },
    /// The block's chunk count is zero, past this machine's address space, or more than the bytes can hold;
    /// `why` says which.
    #[error("block at {at}: {count} chunks: {why}")]
    Chunks {
        /// The offset of the block in the entry.
        at: usize,
        /// The chunk count stored in the block.
        count: u32,
        /// What is wrong with the count.
        why: &'static str,
    },
    /// A chunk's sizes are unusable: past this machine's address space, over the bound given to
    /// [`read_block`], or a chunk running past the entry.
    #[error("block at {at}, chunk {chunk}: stored size {stored}, decoded size {decoded}: {why}")]
    Size {
        /// The offset of the block in the entry.
        at: usize,
        /// The chunk's position in the block.
        chunk: usize,
        /// The chunk's stored size in bytes.
        stored: u32,
        /// The chunk's decoded size in bytes.
        decoded: u32,
        /// What is wrong with the sizes.
        why: &'static str,
    },
    /// A chunk's stored checksum is not the Adler-32 (starting from 0) of its stored bytes.
    #[error(
        "block at {at}, chunk {chunk}: checksum {stored:08x}, computed {computed:08x} (Adler-32 from 0 over the stored bytes)"
    )]
    Checksum {
        /// The offset of the block in the entry.
        at: usize,
        /// The chunk's position in the block.
        chunk: usize,
        /// The checksum stored in the block.
        stored: u32,
        /// The checksum computed over the chunk's stored bytes.
        computed: u32,
    },
    /// A chunk of a streamed block ([`read_stream`]) does not start with the marker byte 1.
    #[error("streamed block at {at}, chunk {chunk}: marker {marker}, expected 1")]
    Marker {
        /// The offset of the block.
        at: usize,
        /// The chunk's position in the block.
        chunk: usize,
        /// The byte found.
        marker: u8,
    },
    /// A chunk failed to decompress.
    #[error("block at {at}: {source}")]
    Decode {
        /// The offset of the block in the entry.
        at: usize,
        /// The decompressor's error.
        #[source]
        source: primase_foundations::compress::Error,
    },
    /// The entry ends before a field is complete.
    #[error("{0}")]
    Short(#[from] FieldError),
    /// Chunk table or decoded sizes that do not fit this machine's address space; the string names which.
    #[error("{0}: sizes past the address space")]
    Overflow(&'static str),
    /// The two blocks end before the entry does; the count is how many bytes follow.
    #[error("the entry has {0} bytes after its two blocks")]
    Trailing(usize),
    /// The resource block's header is malformed or its lengths disagree with the block's size; the string
    /// says what.
    #[error("resource: {0}")]
    Resource(String),
}

/// One chunk of a block as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// The size in bytes the chunk decodes to.
    pub decoded_size: u32,
    /// The size in bytes the chunk occupies in the block; a chunk with `stored_size == decoded_size` is
    /// stored raw.
    pub stored_size: u32,
    /// The Adler-32 checksum (starting value 0) of the stored bytes, as stored and verified.
    pub checksum: u32,
}

/// One data block as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The compression version (u16 at offset 8): 2 for zstd and LZO1X, 1 for LZO2A.
    pub version: u16,
    /// The compression algorithm byte (offset 10): 5 for zstd, 0 for LZO1X, 2 for LZO2A.
    pub algorithm: u8,
    /// The two u16 after the algorithm, as stored (observed 0 and 32772 in Riders Republic's blocks with a chunk
    /// table, 0 and 4 in its streamed ones, 32768 and 32768 in Shaun White Skateboarding's with a table, 32768 and 0 in
    /// its streamed ones).
    pub unknown: [u16; 2],
    /// The block's chunk table, in stored order.
    pub chunks: Vec<Chunk>,
    /// The chunks' decoded bytes in order (the stored bytes where a chunk is stored raw).
    pub data: Vec<u8>,
}

impl Block {
    /// The decoded size of all its chunks.
    pub fn decoded_size(&self) -> usize {
        self.data.len()
    }
}

/// An entry's two blocks: the metadata (16 bytes, as stored) and the resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The first block: the entry's 16-byte metadata, as stored.
    pub metadata: Block,
    /// The second block: the resource.
    pub resource: Block,
}

/// A resource as its block holds it: the header parsed, the body as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// CRC-32 of the type's name.
    pub type_id: u32,
    /// The resource's name, read as UTF-8 from the length-prefixed field after the type id and lengths.
    pub name: String,
    /// The byte after the name: 0 (1,292,246 resources of the six forges' census) or 1 (9,091: every Animation, the
    /// one AnimTrackBuiltinTracksTable and three of 0x8DEEB8AD), which a table follows ([`Resource::header_rows`]).
    pub file_header: u8,
    /// With file header 1: the table between it and the payload, as stored: the bytes `02 00 00`, the row count (u32)
    /// and that many rows of 12 bytes. Their meaning isn't read (on an Animation: pairs of a hash and a count, the
    /// registry's object-type hashes among them). Empty with file header 0.
    pub header_rows: Vec<[u8; 12]>,
    /// The u64 that starts the payload: the resource's ClassID, which equals its entry's index id.
    pub class_id: u64,
    /// Observed equal to the type id.
    pub extension: u32,
    /// The rest of the payload, as stored.
    pub body: Vec<u8>,
}

/// The block at `at` in `bytes`, checked and decoded, and where the next one starts. `max_decoded` bounds the decoded
/// size its chunks may declare in all.
pub fn read_block(bytes: &[u8], at: usize, max_decoded: usize) -> Result<(Block, usize), Error> {
    let Info {
        version,
        algorithm,
        unknown,
        decoder,
    } = info(bytes, at)?;
    let info = |off: usize| at.saturating_add(off);
    let overflow = Error::Overflow;
    let narrow = version == VERSION_1;
    let count = if narrow {
        View::new(bytes).u16(info(15), LE).field("chunk count")?.into()
    } else {
        View::new(bytes).u32(info(15), LE).field("chunk count")?
    };
    // the chunk table's start and a row's size: (u16, u16) rows after a u16 count in version 1
    let (table, row): (usize, usize) = if narrow { (17, 4) } else { (19, 8) };
    let n = usize::try_from(count).map_err(|_| Error::Chunks {
        at,
        count,
        why: "past this machine",
    })?;
    if n == 0 {
        return Err(Error::Chunks {
            at,
            count,
            why: "a block holds at least one",
        });
    }
    // every chunk needs its row of sizes and 4 bytes of checksum: a count the bytes can't hold is refused before
    // allocating
    let tables = n
        .checked_mul(row.saturating_add(4))
        .ok_or(overflow("chunk table"))?;
    if tables > bytes.len().saturating_sub(at) {
        return Err(Error::Chunks {
            at,
            count,
            why: "more than the bytes can hold",
        });
    }
    let mut chunks = Vec::with_capacity(n);
    let mut total = 0usize;
    for k in 0..n {
        let pair = info(table)
            .checked_add(k.checked_mul(row).ok_or(overflow("chunk table"))?)
            .ok_or(overflow("chunk table"))?;
        let (decoded_size, stored_size) = if narrow {
            let second = pair.checked_add(2).ok_or(overflow("chunk table"))?;
            (
                u32::from(View::new(bytes).u16(pair, LE).field("chunk decoded size")?),
                u32::from(View::new(bytes).u16(second, LE).field("chunk stored size")?),
            )
        } else {
            let second = pair.checked_add(4).ok_or(overflow("chunk table"))?;
            (
                View::new(bytes).u32(pair, LE).field("chunk decoded size")?,
                View::new(bytes).u32(second, LE).field("chunk stored size")?,
            )
        };
        let decoded = usize::try_from(decoded_size).map_err(|_| Error::Size {
            at,
            chunk: k,
            stored: stored_size,
            decoded: decoded_size,
            why: "decoded size past this machine",
        })?;
        total = total.checked_add(decoded).ok_or(overflow("decoded sizes"))?;
        if total > max_decoded {
            return Err(Error::Size {
                at,
                chunk: k,
                stored: stored_size,
                decoded: decoded_size,
                why: "decoded sizes over the stated bound",
            });
        }
        chunks.push(Chunk {
            decoded_size,
            stored_size,
            checksum: 0,
        });
    }
    let mut data = Vec::new();
    data.try_reserve_exact(total)
        .map_err(|_| overflow("decoded bytes"))?;
    let mut cursor = info(table)
        .checked_add(n.checked_mul(row).ok_or(overflow("chunk table"))?)
        .ok_or(overflow("chunk table"))?;
    for (k, chunk) in chunks.iter_mut().enumerate() {
        chunk.checksum = View::new(bytes).u32(cursor, LE).field("chunk checksum")?;
        let from = cursor.checked_add(4).ok_or(overflow("chunk"))?;
        cursor = decode_chunk(decoder, bytes, at, k, chunk, from, &mut data)?;
    }
    Ok((
        Block {
            version,
            algorithm,
            unknown,
            chunks,
            data,
        },
        cursor,
    ))
}

/// A block's compression info: the version, algorithm and the two u16 after it, and the decoder they name.
struct Info {
    version: u16,
    algorithm: u8,
    unknown: [u16; 2],
    decoder: &'static dyn Decompressor,
}

/// The block magic at `at` and the 7 bytes of compression info after it.
fn info(bytes: &[u8], at: usize) -> Result<Info, Error> {
    let magic = View::new(bytes).u64(at, LE).field("block magic")?;
    if magic != BLOCK_MAGIC {
        return Err(Error::Magic { at, found: magic });
    }
    // an offset past the address space is past the data: its read is refused
    let info = |off: usize| at.saturating_add(off);
    let version = View::new(bytes).u16(info(8), LE).field("compression version")?;
    if version != VERSION && version != VERSION_1 {
        return Err(Error::Version { at, version });
    }
    let algorithm = View::new(bytes).u8(info(10)).field("compression algorithm")?;
    let decoder: &'static dyn Decompressor = match (version, algorithm) {
        (VERSION, ALGORITHM_ZSTD) => &Zstd,
        (VERSION, ALGORITHM_LZO1X) => &Lzo1x,
        (VERSION_1, ALGORITHM_LZO2A) => &Lzo2a,
        _ => {
            return Err(Error::Algorithm {
                at,
                version,
                algorithm,
            });
        }
    };
    let unknown = [
        View::new(bytes).u16(info(11), LE).field("compression info")?,
        View::new(bytes).u16(info(13), LE).field("compression info")?,
    ];
    Ok(Info {
        version,
        algorithm,
        unknown,
        decoder,
    })
}

/// Checks chunk `k` of the block at `at`, whose stored bytes start at `from` (`chunk.checksum` already read), appends
/// its decoded bytes to `data`, and returns where its stored bytes end.
fn decode_chunk(
    decoder: &dyn Decompressor,
    bytes: &[u8],
    at: usize,
    k: usize,
    chunk: &Chunk,
    from: usize,
    data: &mut Vec<u8>,
) -> Result<usize, Error> {
    let size = |why| Error::Size {
        at,
        chunk: k,
        stored: chunk.stored_size,
        decoded: chunk.decoded_size,
        why,
    };
    let stored_len = usize::try_from(chunk.stored_size).map_err(|_| size("stored size past this machine"))?;
    let decoded = usize::try_from(chunk.decoded_size).map_err(|_| size("decoded size past this machine"))?;
    let stored = View::new(bytes)
        .slice(from, stored_len)
        .field("chunk stored bytes")?;
    let computed = primase_foundations::hash::adler32(CHECKSUM_INIT, stored);
    if computed != chunk.checksum {
        return Err(Error::Checksum {
            at,
            chunk: k,
            stored: chunk.checksum,
            computed,
        });
    }
    if stored_len == decoded {
        data.extend_from_slice(stored);
    } else {
        let out = decoder
            .decompress(stored, decoded)
            .map_err(|source| Error::Decode { at, source })?;
        data.extend_from_slice(&out);
    }
    from.checked_add(stored_len).ok_or(size("chunk past the entry"))
}

/// The streamed form of a block at `at` in `bytes`, which decodes to exactly `decoded_size` bytes, checked and
/// decoded, and where it ends. Shaun White Skateboarding's compressed shader databases (`ekshadersPC*compress.bin`)
/// hold one. Its header is a block's (magic, version 1, algorithm 2 (LZO2A), two u16: 32768 and 0 in every one) with
/// no chunk count or table: each chunk is a marker byte (1), its stored size, decoded size and checksum (u32 each,
/// the checksum as in a block), then its stored bytes; the chunks run until their decoded sizes reach
/// `decoded_size`. A marker other than 1, or chunks whose decoded sizes pass `decoded_size`, are refused.
pub fn read_stream(bytes: &[u8], at: usize, decoded_size: usize) -> Result<(Block, usize), Error> {
    stream(bytes, at, Until::Decoded(decoded_size))
}

/// The streamed block at `at` in `bytes` whose chunks run to the end of `bytes` (a Riders Republic forge's
/// `PrefetchingFileInfos` entry is one: version 2, algorithm 0, read by [`crate::prefetch`]), checked and decoded. Its
/// chunks are laid out as [`read_stream`]'s; there is at least one, the last ends exactly at the end of `bytes`, and
/// their decoded sizes together may not pass `max_decoded`.
pub fn read_stream_to_end(bytes: &[u8], at: usize, max_decoded: usize) -> Result<Block, Error> {
    stream(bytes, at, Until::End(max_decoded)).map(|(block, _)| block)
}

/// Where a streamed block's chunks stop.
#[derive(Clone, Copy)]
enum Until {
    /// Once their decoded sizes reach this.
    Decoded(usize),
    /// At the end of the bytes, their decoded sizes no more than this in all.
    End(usize),
}

impl Until {
    /// Whether another chunk follows, `decoded` bytes decoded so far and the next one at `cursor` of `len`.
    fn more(self, decoded: usize, cursor: usize, len: usize) -> bool {
        match self {
            Until::Decoded(size) => decoded < size,
            Until::End(_) => cursor < len,
        }
    }

    /// The most the chunks may decode to in all.
    fn bound(self) -> usize {
        match self {
            Until::Decoded(size) | Until::End(size) => size,
        }
    }
}

/// A streamed block's chunks read from `at` until `until`, checked and decoded, and where they end.
fn stream(bytes: &[u8], at: usize, until: Until) -> Result<(Block, usize), Error> {
    let Info {
        version,
        algorithm,
        unknown,
        decoder,
    } = info(bytes, at)?;
    let mut cursor = at.checked_add(STREAM_HEADER).ok_or(Error::Overflow("stream"))?;
    if matches!(until, Until::End(_)) && cursor >= bytes.len() {
        return Err(Error::Chunks {
            at,
            count: 0,
            why: "a block holds at least one",
        });
    }
    let mut chunks = Vec::new();
    // grown chunk by chunk, so a stated size the chunks don't back allocates nothing
    let mut data = Vec::new();
    while until.more(data.len(), cursor, bytes.len()) {
        let k = chunks.len();
        let marker = View::new(bytes).u8(cursor).field("chunk marker")?;
        if marker != STREAM_MARKER {
            return Err(Error::Marker { at, chunk: k, marker });
        }
        let field = |off: usize, what| View::new(bytes).u32(cursor.saturating_add(off), LE).field(what);
        let chunk = Chunk {
            stored_size: field(1, "chunk stored size")?,
            decoded_size: field(5, "chunk decoded size")?,
            checksum: field(9, "chunk checksum")?,
        };
        let remaining = until.bound().saturating_sub(data.len());
        if usize::try_from(chunk.decoded_size).map_or(true, |d| d > remaining) {
            return Err(Error::Size {
                at,
                chunk: k,
                stored: chunk.stored_size,
                decoded: chunk.decoded_size,
                why: "decoded sizes over the stated bound",
            });
        }
        let from = cursor.checked_add(13).ok_or(Error::Overflow("chunk"))?;
        cursor = decode_chunk(decoder, bytes, at, k, &chunk, from, &mut data)?;
        chunks.push(chunk);
    }
    Ok((
        Block {
            version,
            algorithm,
            unknown,
            chunks,
            data,
        },
        cursor,
    ))
}

/// An entry's payload (its bytes from the index's offset, the index's size): its metadata block and its resource
/// block, which must end the entry exactly. `max_decoded` bounds each block's declared decoded size.
pub fn read_entry(payload: &[u8], max_decoded: usize) -> Result<Entry, Error> {
    let (metadata, next) = read_block(payload, 0, max_decoded)?;
    let (resource, end) = read_block(payload, next, max_decoded)?;
    primase_foundations::bytes::checked_span(end, 0, 1, payload.len()).field("entry")?;
    let rest = payload.len().saturating_sub(end);
    if rest != 0 {
        return Err(Error::Trailing(rest));
    }
    Ok(Entry { metadata, resource })
}

impl Resource {
    /// A resource block's decoded bytes: type id (u32), payload length (i32), name length (i32), the name, one byte
    /// (0, or 1 and the table `02 00 00`, a row count and that many 12-byte rows), then the payload (class id u64,
    /// extension u32, the body), which must end the block exactly.
    pub fn parse(data: &[u8]) -> Result<Resource, Error> {
        let bad = |m: String| Error::Resource(m);
        let type_id = View::new(data).u32(0, LE).field("resource type")?;
        let payload_len = View::new(data).u32(4, LE).field("resource payload length")?;
        let name_len = View::new(data).u32(8, LE).field("resource name length")?;
        let name_len = usize::try_from(name_len).map_err(|_| bad("name length past this machine".into()))?;
        let name = View::new(data).slice(12, name_len).field("resource name")?;
        let name = String::from_utf8(name.to_vec()).map_err(|_| bad("name is not UTF-8".into()))?;
        let header_at = 12usize
            .checked_add(name_len)
            .ok_or(bad("name length overflows".into()))?;
        let file_header = *View::new(data)
            .slice(header_at, 1)
            .field("resource file header")?
            .first()
            .ok_or(bad("no file header byte".into()))?;
        let mut payload_at = header_at.checked_add(1).ok_or(bad("offset overflows".into()))?;
        let mut header_rows = Vec::new();
        if file_header == 1 {
            if View::new(data)
                .slice(payload_at, 3)
                .field("resource header table")?
                != [2, 0, 0]
            {
                return Err(bad("file header 1 without the table's bytes 02 00 00".into()));
            }
            let count_at = payload_at.checked_add(3).ok_or(bad("offset overflows".into()))?;
            let rows = usize::try_from(
                View::new(data)
                    .u32(count_at, LE)
                    .field("resource header table rows")?,
            )
            .map_err(|_| bad("row count past this machine".into()))?;
            let mut at = count_at.checked_add(4).ok_or(bad("offset overflows".into()))?;
            for _ in 0..rows.min(data.len()) {
                let row = View::new(data).slice(at, 12).field("resource header table row")?;
                header_rows.push(<[u8; 12]>::try_from(row).map_err(|_| bad("a row of 12 bytes".into()))?);
                at = at.checked_add(12).ok_or(bad("offset overflows".into()))?;
            }
            if header_rows.len() != rows {
                return Err(bad(format!(
                    "{rows} header rows in a block of {} bytes",
                    data.len()
                )));
            }
            payload_at = at;
        }
        let payload_len =
            usize::try_from(payload_len).map_err(|_| bad("payload length past this machine".into()))?;
        let end = payload_at
            .checked_add(payload_len)
            .ok_or(bad("payload length overflows".into()))?;
        if end != data.len() {
            return Err(bad(format!(
                "header and payload end at {end}, the block holds {} bytes",
                data.len()
            )));
        }
        let class_id = View::new(data).u64(payload_at, LE).field("resource class id")?;
        let extension = View::new(data)
            .u32(
                payload_at.checked_add(8).ok_or(bad("offset overflows".into()))?,
                LE,
            )
            .field("resource extension")?;
        let body_at = payload_at.checked_add(12).ok_or(bad("offset overflows".into()))?;
        let body = data
            .get(body_at..)
            .ok_or(bad("payload shorter than its class id and extension".into()))?;
        Ok(Resource {
            type_id,
            name,
            file_header,
            header_rows,
            class_id,
            extension,
            body: body.to_vec(),
        })
    }
}
