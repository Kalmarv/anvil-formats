//! A forge's `PrefetchingFileInfos` entry: its streamed block (version 2, algorithm 0: LZO1X, its chunks running to the
//! entry's end) and its records, laid out here by hand with their refusals, then all six of Riders Republic's forges'
//! entries read (`installs.riders`), each record checked against the forges' indexes.
mod inputs;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;

use anvil_formats::data::{self, ALGORITHM_LZO1X, BLOCK_MAGIC, Error as DataError, read_stream_to_end};
use anvil_formats::forge::Forge;
use anvil_formats::grid::GRID_CELL_DATA_BLOCK;
use anvil_formats::prefetch::{Error, Listed, Member, NAME, PrefetchingFileInfos, Record};
use inputs::{Game, install_file};

/// A streamed block's header: magic, version 2, `algorithm`, the two u16 Riders Republic's streamed blocks hold (0, 4).
fn header(algorithm: u8) -> Vec<u8> {
    let mut b = BLOCK_MAGIC.to_le_bytes().to_vec();
    b.extend(2u16.to_le_bytes());
    b.push(algorithm);
    b.extend(0u16.to_le_bytes());
    b.extend(4u16.to_le_bytes());
    b
}

/// A chunk stored raw (its stored size its decoded size): marker 1, the sizes, the checksum (Adler-32 from 0), the bytes.
fn raw_chunk(bytes: &[u8]) -> Vec<u8> {
    let n = u32::try_from(bytes.len()).expect("fits");
    let mut b = vec![1];
    b.extend(n.to_le_bytes());
    b.extend(n.to_le_bytes());
    b.extend(primase_foundations::hash::adler32(0, bytes).to_le_bytes());
    b.extend(bytes);
    b
}

/// A chunk LZO1X-compressed by hand: one literal run of `bytes` (4 to 238 of them: the first byte is 17 plus the run,
/// as miniLZO writes a stream's opening literals), then the end marker (17, 0, 0).
fn lzo1x_chunk(bytes: &[u8]) -> Vec<u8> {
    assert!((4..=238).contains(&bytes.len()));
    let mut stream = vec![u8::try_from(17 + bytes.len()).expect("fits")];
    stream.extend(bytes);
    stream.extend([17, 0, 0]);
    let mut b = vec![1];
    b.extend(u32::try_from(stream.len()).expect("fits").to_le_bytes());
    b.extend(u32::try_from(bytes.len()).expect("fits").to_le_bytes());
    b.extend(primase_foundations::hash::adler32(0, &stream).to_le_bytes());
    b.extend(stream);
    b
}

/// Decoded records as the format lays them out: the table of (id, size, offset), the record bytes' length, the bytes.
fn infos(records: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut b = u32::try_from(records.len()).expect("fits").to_le_bytes().to_vec();
    let mut offset = 0u32;
    for (id, r) in records {
        let size = u32::try_from(r.len()).expect("fits");
        b.extend(id.to_le_bytes());
        b.extend(size.to_le_bytes());
        b.extend(offset.to_le_bytes());
        offset += size;
    }
    b.extend(offset.to_le_bytes());
    for (_, r) in records {
        b.extend(r);
    }
    b
}

/// A record: its groups of (flag, id) members, then its listed entries of (id, kind, values).
fn record(groups: &[&[(u8, u64)]], listed: &[(u64, u8, &[u16])]) -> Vec<u8> {
    let mut b = u16::try_from(groups.len()).expect("fits").to_le_bytes().to_vec();
    for g in groups {
        b.extend(u16::try_from(g.len()).expect("fits").to_le_bytes());
        for &(flag, id) in *g {
            b.push(flag);
            b.extend(id.to_le_bytes());
        }
    }
    b.extend(u16::try_from(listed.len()).expect("fits").to_le_bytes());
    for &(id, kind, values) in listed {
        b.extend(id.to_le_bytes());
        b.push(kind);
        b.extend(u16::try_from(values.len()).expect("fits").to_le_bytes());
        for v in values {
            b.extend(v.to_le_bytes());
        }
    }
    b
}

#[test]
fn a_streamed_lzo1x_block_reads_to_the_entry_s_end() {
    let mut b = header(ALGORITHM_LZO1X);
    b.extend(lzo1x_chunk(b"prefetching "));
    b.extend(raw_chunk(b"file infos"));
    let block = read_stream_to_end(&b, 0, 64).unwrap();
    assert_eq!(block.data, b"prefetching file infos");
    assert_eq!((block.version, block.algorithm, block.unknown), (2, 0, [0, 4]));
    assert_eq!(block.chunks.len(), 2);
    assert_eq!(
        (block.chunks[0].decoded_size, block.chunks[0].stored_size),
        (12, 16)
    );
    // the block-with-a-table reader takes algorithm 0 too, its flags a table block's (bit 15 of the second u16 set),
    // and refuses the streamed block as one
    assert!(matches!(
        data::read_block(&b, 0, 64),
        Err(DataError::Streamed { at: 0, flags: 4 })
    ));
    let mut table = header(ALGORITHM_LZO1X);
    table[13..15].copy_from_slice(&0x8004u16.to_le_bytes());
    table.extend(1u32.to_le_bytes());
    let chunk = lzo1x_chunk(b"abcd");
    table.extend(4u32.to_le_bytes());
    table.extend(8u32.to_le_bytes());
    table.extend(&chunk[9..]);
    assert_eq!(data::read_block(&table, 0, 64).unwrap().0.data, b"abcd");
}

#[test]
fn a_streamed_block_out_of_its_layout_is_refused() {
    let mut b = header(ALGORITHM_LZO1X);
    b.extend(raw_chunk(b"one"));
    b.extend(raw_chunk(b"two"));
    // no chunk at all
    assert!(matches!(
        read_stream_to_end(&header(ALGORITHM_LZO1X), 0, 64),
        Err(DataError::Chunks { count: 0, .. })
    ));
    // a chunk cut short at the end of the bytes, and a byte after the last chunk
    assert!(matches!(
        read_stream_to_end(&b[..b.len() - 1], 0, 64),
        Err(DataError::Short(_))
    ));
    let mut longer = b.clone();
    longer.push(1);
    assert!(read_stream_to_end(&longer, 0, 64).is_err());
    // decoded sizes over the bound
    assert!(matches!(
        read_stream_to_end(&b, 0, 5),
        Err(DataError::Size { .. })
    ));
    // a marker other than 1, a checksum not the bytes'
    let mut marker = b.clone();
    marker[15] = 2;
    assert!(matches!(
        read_stream_to_end(&marker, 0, 64),
        Err(DataError::Marker { marker: 2, .. })
    ));
    let mut sum = b.clone();
    sum[24] ^= 1;
    assert!(matches!(
        read_stream_to_end(&sum, 0, 64),
        Err(DataError::Checksum { .. })
    ));
    // an algorithm not read
    let mut other = header(3);
    other.extend(raw_chunk(b"one"));
    assert!(matches!(
        read_stream_to_end(&other, 0, 64),
        Err(DataError::Algorithm { algorithm: 3, .. })
    ));
}

#[test]
fn records_read_as_laid() {
    let bytes = infos(&[
        (8, record(&[], &[])),
        (40, record(&[], &[(51, 1, &[])])),
        (
            1_773_413_991_756,
            record(
                &[&[(0, 7)], &[(1, 9), (0, 10)]],
                &[(11, 4, &[]), (12, 1, &[0, 3])],
            ),
        ),
    ]);
    let p = PrefetchingFileInfos::parse(&bytes).unwrap();
    assert_eq!(p.records.len(), 3);
    assert_eq!(
        p.records[0],
        Record {
            id: 8,
            groups: vec![],
            listed: vec![]
        }
    );
    assert_eq!(
        p.get(40).unwrap().listed,
        vec![Listed {
            id: 51,
            kind: 1,
            values: vec![]
        }]
    );
    let r = p.get(1_773_413_991_756).unwrap();
    assert_eq!(
        r.groups,
        vec![
            vec![Member { flag: 0, id: 7 }],
            vec![Member { flag: 1, id: 9 }, Member { flag: 0, id: 10 }]
        ]
    );
    assert_eq!(
        r.listed[1],
        Listed {
            id: 12,
            kind: 1,
            values: vec![0, 3]
        }
    );
    assert!(p.get(9).is_none());
    // read through its block
    let mut block = header(ALGORITHM_LZO1X);
    block.extend(raw_chunk(&bytes));
    assert_eq!(PrefetchingFileInfos::read(&block, 1 << 20).unwrap(), p);
    assert!(matches!(
        PrefetchingFileInfos::read(&block, 8),
        Err(Error::Block(_))
    ));
}

#[test]
fn records_out_of_their_layout_are_refused() {
    let good = || vec![(8, record(&[], &[(51, 1, &[])])), (9, record(&[], &[]))];
    let refused = |b: &[u8], what: &str| {
        let e = PrefetchingFileInfos::parse(b).unwrap_err().to_string();
        assert!(e.contains(what), "{e}");
    };
    // ids not ascending
    let mut ids = good();
    ids[1].0 = 8;
    refused(&infos(&ids), "record id");
    // an offset that is not where the record before it ends
    let mut gap = infos(&good());
    gap[4 + 16 + 12] += 1;
    refused(&gap, "record offset");
    // a stated length other than the sizes added up
    let mut length = infos(&good());
    length[4 + 32] += 1;
    refused(&length, "record bytes length");
    // a byte after the record bytes
    let mut after = infos(&good());
    after.push(0);
    refused(&after, "after its end");
    // a record with a byte after what it holds, and one whose listed count runs past its size
    let mut long = good();
    long[0].1.push(0);
    refused(&infos(&long), "after its end");
    let mut short = good();
    short[1].1 = vec![0, 0, 1, 0];
    refused(&infos(&short), "listed id");
    // a count of more rows than the bytes hold
    let mut count = infos(&good());
    count[0] = 200;
    refused(&count, "record count");
}

/// One forge's entry as the census measured it: the forge, its entries, its records, the entries they list, the
/// records with groups, the decoded bytes and the chunks.
const FORGES: [(&str, usize, usize, usize, usize, usize, usize); 6] = [
    ("DataPC.forge", 163_356, 163_354, 198_447, 0, 5_450_005, 21),
    ("DataPC_patch_01.forge", 49_363, 49_361, 76_631, 0, 1_830_169, 7),
    (
        "DataPC_MAD_USNAT.forge",
        767_463,
        767_461,
        1_695_568,
        6_675,
        34_303_570,
        131,
    ),
    (
        "DataPC_MAD_USNAT_patch_01.forge",
        320_777,
        320_775,
        824_351,
        4_898,
        15_729_802,
        61,
    ),
    ("DataPC_MAD_WhiteRoom.forge", 388, 386, 417, 2, 12_341, 1),
    ("DataPC_MAD_WhiteRoom_patch_01.forge", 337, 335, 481, 0, 11_999, 1),
];

/// Each of the six forges ends with its `PrefetchingFileInfos`: one LZO1X-streamed block whose every chunk decodes
/// (and which the entry reader refuses as streamed),
/// holding a record for every entry of the forge but the `GlobalMetaFile` and itself. Every id a record holds is an
/// index id of one of the six forges; groups are only in GridCellDataBlocks' records and name entries of type
/// 0x41813744; values are only in GridCellDataBlocks' records.
#[test]
fn every_riders_forge_s_records_name_entries_of_the_forges() {
    let mut forges = Vec::new();
    // in FORGES' order
    let paths = [
        install_file(Game::Riders, "DataPC.forge"),
        install_file(Game::Riders, "DataPC_patch_01.forge"),
        install_file(Game::Riders, "DataPC_MAD_USNAT.forge"),
        install_file(Game::Riders, "DataPC_MAD_USNAT_patch_01.forge"),
        install_file(Game::Riders, "DataPC_MAD_WhiteRoom.forge"),
        install_file(Game::Riders, "DataPC_MAD_WhiteRoom_patch_01.forge"),
    ];
    for (path, (name, ..)) in paths.into_iter().zip(FORGES) {
        let Some(path) = path else {
            return;
        };
        assert!(path.ends_with(name));
        forges.push((path.clone(), Forge::open(&path).unwrap()));
    }
    // every index id of the six, with its entry's type
    let types: BTreeMap<u64, u32> = forges
        .iter()
        .flat_map(|(_, f)| f.entries().map(|(i, n)| (i.id, n.type_id)))
        .collect();
    for ((path, forge), (name, entries, records, listed, grouped, decoded, chunks)) in
        forges.iter().zip(FORGES)
    {
        assert_eq!(forge.index.len(), entries, "{name}");
        let k = forge
            .entries()
            .position(|(_, n)| n.name == NAME)
            .expect("a PrefetchingFileInfos");
        assert_eq!(k, entries - 1, "{name}: the last entry");
        let mut file = File::open(path).unwrap();
        let payload = forge.payload(&mut file, k).unwrap();
        // an entry's two blocks are not what it holds: the entry reader refuses it as streamed
        assert!(
            matches!(
                data::read_entry(&payload, 64 << 20),
                Err(DataError::Streamed { at: 0, flags: 4 })
            ),
            "{name}"
        );
        // nor is the GlobalMetaFile, the first entry, data blocks: its first eight bytes are no block magic
        let meta = forge.entries().position(|(_, n)| n.name == "GlobalMetaFile");
        assert_eq!(meta, Some(0), "{name}");
        assert!(
            matches!(
                data::read_entry(&forge.payload(&mut file, 0).unwrap(), 64 << 20),
                Err(DataError::Magic {
                    at: 0,
                    found: 0x0000_0004_0000_0001
                })
            ),
            "{name}"
        );
        let block = read_stream_to_end(&payload, 0, 64 << 20).unwrap();
        assert_eq!(
            (
                block.algorithm,
                block.unknown,
                block.chunks.len(),
                block.data.len()
            ),
            (ALGORITHM_LZO1X, [0, 4], chunks, decoded),
            "{name}"
        );
        let p = PrefetchingFileInfos::parse(&block.data).unwrap();
        assert_eq!(p.records.len(), records, "{name}");
        let names: BTreeSet<&str> = forge.entries().map(|(_, n)| n.name.as_str()).collect();
        assert!(names.contains("GlobalMetaFile"), "{name}");
        let own: BTreeSet<u64> = forge
            .entries()
            .filter(|(_, n)| n.name != NAME && n.name != "GlobalMetaFile")
            .map(|(i, _)| i.id)
            .collect();
        assert!(
            p.records.iter().map(|r| r.id).eq(own.iter().copied()),
            "{name}: a record per entry"
        );
        assert_eq!(
            p.records.iter().map(|r| r.listed.len()).sum::<usize>(),
            listed,
            "{name}"
        );
        assert_eq!(
            p.records.iter().filter(|r| !r.groups.is_empty()).count(),
            grouped,
            "{name}"
        );
        for r in &p.records {
            let kind = types[&r.id];
            for m in r.groups.iter().flatten() {
                assert_eq!(kind, GRID_CELL_DATA_BLOCK, "{name}: groups on {:#x}", r.id);
                assert_eq!(types.get(&m.id), Some(&0x4181_3744), "{name}: member {:#x}", m.id);
                assert!(m.flag <= 1, "{name}");
            }
            for l in &r.listed {
                assert!(
                    types.contains_key(&l.id),
                    "{name}: listed {:#x} in no forge",
                    l.id
                );
                assert!(l.kind == 1 || l.kind == 4, "{name}: kind {}", l.kind);
                if !l.values.is_empty() {
                    assert_eq!(kind, GRID_CELL_DATA_BLOCK, "{name}: values on {:#x}", r.id);
                }
            }
        }
    }
}
