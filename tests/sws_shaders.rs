//! Shaun White Skateboarding's SDB2 shader databases in both forms: a compressed database laid out here by hand (raw
//! chunks of a streamed block) and its refusals, and every `ekshaders*.bin` of `installs.shaunwhite` read, each
//! compressed one decoding to its uncompressed sibling byte for byte.
// Test code: helpers outside #[test] functions unwrap too.
#![allow(clippy::unwrap_used)]

mod inputs;

use anvil_formats::data::{self, CHECKSUM_INIT, STREAM_MARKER};
use anvil_formats::shaunwhite::shaders::{self, COMPRESSED_TAG, Payload};
use inputs::{Game, install};
use primase_foundations::hash::adler32;

/// An SDB2 with no sources and one kind-5 (vertex) record holding `program`.
fn database(program: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(b"SDB2");
    for word in [39u32, 0, 1, 24, 24] {
        b.extend(word.to_le_bytes());
    }
    b.extend(5u32.to_le_bytes()); // kind
    b.extend(0u64.to_le_bytes()); // permutation
    b.extend([0; 16]); // dependency mask
    b.extend(60u32.to_le_bytes()); // payload offset: right after the table
    b.extend(u32::try_from(program.len()).unwrap().to_le_bytes());
    b.extend(program);
    b
}

/// `plain` as a compressed database whose streamed block holds it in raw chunks of at most `chunk` bytes.
fn compressed(plain: &[u8], chunk: usize) -> Vec<u8> {
    let mut b = vec![COMPRESSED_TAG];
    b.extend(u32::try_from(plain.len()).unwrap().to_le_bytes());
    b.extend(data::BLOCK_MAGIC.to_le_bytes());
    b.extend(data::VERSION_1.to_le_bytes());
    b.push(data::ALGORITHM_LZO2A);
    b.extend(32768u16.to_le_bytes());
    b.extend(0u16.to_le_bytes());
    for part in plain.chunks(chunk) {
        let n = u32::try_from(part.len()).unwrap().to_le_bytes();
        b.push(STREAM_MARKER);
        b.extend(n); // stored size
        b.extend(n); // decoded size: equal, so stored raw
        b.extend(adler32(CHECKSUM_INIT, part).to_le_bytes());
        b.extend(part);
    }
    b
}

#[test]
fn a_compressed_database_reads_as_the_database_it_holds() {
    let plain = database(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let packed = compressed(&plain, 16);
    assert_eq!(shaders::decompress(&packed).unwrap(), plain);
    assert_eq!(shaders::read(&packed).unwrap(), shaders::read(&plain).unwrap());
    let db = shaders::read(&packed).unwrap();
    assert!(
        matches!(&db.records[0].payload, Payload::Program { bytes, .. } if bytes == &[1, 2, 3, 4, 5, 6, 7, 8, 9])
    );
}

#[test]
fn a_compressed_database_is_refused_where_it_is_wrong() {
    let plain = database(&[1, 2, 3, 4]);
    let packed = compressed(&plain, 16);
    // a byte after the block
    let mut extra = packed.clone();
    extra.push(0);
    let e = shaders::read(&extra).unwrap_err().to_string();
    assert!(e.contains("1 bytes after the data block"), "{e}");
    // a chunk marker other than 1 (the first chunk's, after the 5-byte header and the block's 15)
    let mut marker = packed.clone();
    marker[20] = 2;
    let e = shaders::read(&marker).unwrap_err().to_string();
    assert!(e.contains("marker 2, expected 1"), "{e}");
    // a stored byte changed: the checksum no longer matches
    let mut flipped = packed.clone();
    *flipped.last_mut().unwrap() ^= 1;
    let e = shaders::read(&flipped).unwrap_err().to_string();
    assert!(e.contains("checksum"), "{e}");
    // a stated size smaller than the chunks
    let mut small = packed.clone();
    small[1..5].copy_from_slice(&3u32.to_le_bytes());
    let e = shaders::read(&small).unwrap_err().to_string();
    assert!(e.contains("over the stated bound"), "{e}");
    // a stated size the chunks don't reach
    let mut large = packed;
    let more = u32::try_from(plain.len() + 1).unwrap();
    large[1..5].copy_from_slice(&more.to_le_bytes());
    assert!(shaders::read(&large).is_err());
}

#[test]
fn every_shader_database_of_the_install_reads() {
    let Some(folder) = install(Game::ShaunWhite) else {
        return;
    };
    let mut read = Vec::new();
    for level in ["low", "med", "high"] {
        let plain = std::fs::read(folder.join(format!("ekshadersPC{level}.bin"))).unwrap();
        let packed = std::fs::read(folder.join(format!("ekshadersPC{level}compress.bin"))).unwrap();
        assert_eq!(packed.first(), Some(&COMPRESSED_TAG), "{level}");
        assert!(
            shaders::decompress(&packed).unwrap() == plain,
            "{level}: the compressed database decodes to other bytes"
        );
        let db = shaders::read(&plain).unwrap();
        read.push((level, db.sources.len(), db.records.len()));
    }
    // the init database has no uncompressed sibling
    let init = shaders::read(&std::fs::read(folder.join("ekshadersinitpccompress.bin")).unwrap()).unwrap();
    read.push(("init", init.sources.len(), init.records.len()));
    eprintln!("{read:?}");
    assert!(read.iter().all(|&(_, _, records)| records > 0), "{read:?}");
}
