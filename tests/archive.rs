//! `Archive::resource` holds a block's declared decoded size to a bound the caller can change, over a version-26 forge
//! laid out here by hand and written to a temporary file.
use std::path::PathBuf;

use anvil_formats::archive::{Archive, At, Error, MAX_RECORD};
use anvil_formats::data::{BLOCK_MAGIC, Error as DataError};
use anvil_formats::forge::FILEDATA_HEADER;

const HEADER: usize = 1046;
const D2: usize = HEADER + 40;
const INDEX: usize = D2 + 48;

/// A version-26 forge of `rows` (name, index id, name id, payload) as the format lays it out: header size 1046,
/// DataHeader1 (40) there, DataHeader2 after it, the index table (16-byte records) and the name table (188), each with
/// one spare record, then each entry: its FILEDATA header and its payload.
fn forge(rows: &[(&str, u32, u64, &[u8])]) -> Vec<u8> {
    let n = rows.len();
    let names = INDEX + 16 * (n + 1);
    let end = names + 188 * (n + 1);
    let mut b = vec![0u8; end];
    let put = |b: &mut Vec<u8>, at: usize, bytes: &[u8]| b[at..at + bytes.len()].copy_from_slice(bytes);
    put(&mut b, 0, b"scimitar\0");
    put(&mut b, 0x09, &26i32.to_le_bytes());
    put(&mut b, 0x0D, &(HEADER as u64).to_le_bytes());
    put(&mut b, 0x15, &16i64.to_le_bytes());
    put(&mut b, 0x1D, &1i32.to_le_bytes());
    // DataHeader1: entries, three i32, an i64, MaxFilesForThisIndex, one FileSet, DataHeader2's offset
    put(&mut b, HEADER, &(n as i32).to_le_bytes());
    put(&mut b, HEADER + 4, &1i32.to_le_bytes());
    put(&mut b, HEADER + 16, &(-1i64).to_le_bytes());
    put(&mut b, HEADER + 24, &(n as i32 + 1).to_le_bytes());
    put(&mut b, HEADER + 28, &1i32.to_le_bytes());
    put(&mut b, HEADER + 32, &(D2 as i64).to_le_bytes());
    put(&mut b, D2, &(n as i32).to_le_bytes());
    put(&mut b, D2 + 4, &1i32.to_le_bytes());
    put(&mut b, D2 + 8, &(INDEX as i64).to_le_bytes());
    put(&mut b, D2 + 16, &(-1i64).to_le_bytes());
    put(&mut b, D2 + 28, &(n as i32).to_le_bytes());
    put(&mut b, D2 + 32, &(names as i64).to_le_bytes());
    put(&mut b, D2 + 40, &(end as i64).to_le_bytes());
    for (k, (name, id, name_id, payload)) in rows.iter().enumerate() {
        let offset = b.len();
        let size = payload.len() as i32;
        let i = INDEX + 16 * k;
        put(&mut b, i, &(offset as i64).to_le_bytes());
        put(&mut b, i + 8, &id.to_le_bytes());
        put(&mut b, i + 12, &size.to_le_bytes());
        let m = names + 188 * k;
        put(&mut b, m, &size.to_le_bytes());
        put(&mut b, m + 4, &name_id.to_le_bytes());
        put(&mut b, m + 40, &1_287_524_118i32.to_le_bytes());
        put(&mut b, m + 44, name.as_bytes());
        put(&mut b, m + 176, &7u32.to_le_bytes());
        let mut h = vec![0u8; FILEDATA_HEADER as usize];
        h[..8].copy_from_slice(b"FILEDATA");
        h[8..8 + name.len()].copy_from_slice(name.as_bytes());
        h[391..395].copy_from_slice(&id.to_le_bytes());
        h[395..399].copy_from_slice(&size.to_le_bytes());
        h[399..407].copy_from_slice(&name_id.to_le_bytes());
        b.extend_from_slice(&h);
        b.extend_from_slice(payload);
    }
    b
}

/// A compression-version-1 block of `chunks` (decoded length, stored bytes): count u16, (u16, u16) sizes, then each
/// chunk's Adler-32 from 0 and its bytes.
fn block(chunks: &[(u16, &[u8])]) -> Vec<u8> {
    let mut b = BLOCK_MAGIC.to_le_bytes().to_vec();
    b.extend_from_slice(&1u16.to_le_bytes());
    b.push(2);
    b.extend_from_slice(&32768u16.to_le_bytes());
    b.extend_from_slice(&32768u16.to_le_bytes());
    b.extend_from_slice(&(chunks.len() as u16).to_le_bytes());
    for (decoded, stored) in chunks {
        b.extend_from_slice(&decoded.to_le_bytes());
        b.extend_from_slice(&(stored.len() as u16).to_le_bytes());
    }
    for (_, stored) in chunks {
        b.extend_from_slice(&primase_foundations::hash::adler32(0, stored).to_le_bytes());
        b.extend_from_slice(stored);
    }
    b
}

/// A resource block: type id, payload length, name, the file header byte, class id, extension, body.
fn resource(body: &[u8]) -> Vec<u8> {
    let name = "Thing";
    let mut r = 0x1234_5678u32.to_le_bytes().to_vec();
    r.extend_from_slice(&(12 + body.len() as u32).to_le_bytes());
    r.extend_from_slice(&(name.len() as u32).to_le_bytes());
    r.extend_from_slice(name.as_bytes());
    r.push(0);
    r.extend_from_slice(&7u64.to_le_bytes());
    r.extend_from_slice(&0x1234_5678u32.to_le_bytes());
    r.extend_from_slice(body);
    r
}

/// The forge as a file in the temporary directory, opened as an archive.
fn open(name: &str, rows: &[(&str, u32, u64, &[u8])]) -> (Archive, PathBuf) {
    let path = std::env::temp_dir().join(format!("anvil-formats-{}-{name}.forge", std::process::id()));
    std::fs::write(&path, forge(rows)).expect("a hand-built forge");
    (Archive::open(&[&path]).expect("a hand-built forge"), path)
}

#[test]
fn the_decoded_size_bound_is_the_archives_to_change() {
    let body = [9u8; 100];
    let decoded = resource(&body);
    let entry = [block(&[(4, b"meta")]), block(&[(decoded.len() as u16, &decoded)])].concat();
    let (mut archive, path) = open("bound", &[("Thing", 0x80C, 2, &entry)]);
    let at = At { forge: 0, entry: 0 };
    assert_eq!(archive.max_record(), MAX_RECORD);
    assert_eq!(archive.resource(at).expect("a hand-built forge").body, body);
    // a bound under the block's size refuses it, naming the bound's reason, and raising it reads it
    archive.set_max_record(50);
    assert!(matches!(
        archive.resource(at),
        Err(Error::Data {
            source: DataError::Size { .. },
            ..
        })
    ));
    archive.set_max_record(1 << 20);
    assert_eq!(archive.resource(at).expect("a hand-built forge").body, body);
    std::fs::remove_file(path).expect("a hand-built forge");
}

#[test]
fn a_block_over_the_default_bound_is_refused_until_the_bound_is_raised() {
    // 1,100 chunks declaring 65,535 decoded bytes each (72 MB, over 64 MiB) and storing none: only the sizes matter
    let chunks = vec![(65_535u16, &[][..]); 1_100];
    let entry = [block(&[(4, b"meta")]), block(&chunks)].concat();
    let (mut archive, path) = open("default", &[("Big", 0x80C, 2, &entry)]);
    let at = At { forge: 0, entry: 0 };
    assert!(matches!(
        archive.resource(at),
        Err(Error::Data {
            source: DataError::Size { .. },
            ..
        })
    ));
    archive.set_max_record(1 << 30);
    // past the bound the sizes are no longer the reason: the empty chunks fail to decode
    assert!(!matches!(
        archive.resource(at),
        Err(Error::Data {
            source: DataError::Size { .. },
            ..
        })
    ));
    std::fs::remove_file(path).expect("a hand-built forge");
}

/// A version-26 forge's name records hold 0 where version 27's hold the type id: the listing says it has none.
#[test]
fn a_version_26_listing_states_no_type() {
    let (archive, path) = open("listing", &[("Thing", 0x80C, 2, b"abc")]);
    let listed: Vec<_> = archive.listing().collect();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].1.type_id, listed[0].1.kind), (None, None));
    assert!(!archive.forge(0).expect("a hand-built forge").stores_type_ids());
    std::fs::remove_file(path).expect("a hand-built forge");
}
