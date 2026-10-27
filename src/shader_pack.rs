//! The engine's shader pack (`ShaderDataDX11.bin` in Riders Republic's folder): DXBC containers back to back, then 16
//! bytes, the MD5 of every byte before them. Each container is checked by its own stored checksum
//! ([`dxcontainer::checksum`]) and the pack by its last 16 bytes; a container or pack whose digest disagrees is
//! refused, naming it. What a container is to the engine (which material, pass or permutation uses it) isn't read.
//!
//! The Riders Republic survey found the 2,443 containers (SM5; resource definitions stripped);
//! a census of this reader checks every digest.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

/// Why a shader pack can't be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A DXBC container is malformed: its header is shorter than 28 bytes, or its size field runs past the
    /// pack.
    #[error("shader pack container {index} at byte {at}: {why}")]
    Container {
        /// The container's position in the pack, counting from 0.
        index: usize,
        /// The byte offset in the pack where the container starts.
        at: usize,
        /// What is wrong with it: `"short header"` or `"size past the pack"`.
        why: &'static str,
    },
    /// The container's stored checksum (bytes 4 to 19) is not the checksum of its bytes, or its size field is
    /// under 32.
    #[error("shader pack container {index} at byte {at}: its checksum isn't its bytes'")]
    Checksum {
        /// The container's position in the pack, counting from 0.
        index: usize,
        /// The byte offset in the pack where the container starts.
        at: usize,
    },
    /// The pack does not end with exactly 16 bytes after its last container; `left` is the number of bytes
    /// found there.
    #[error("shader pack: {left} bytes after the containers, where its 16-byte MD5 is")]
    Trailer {
        /// The number of bytes found after the last container.
        left: usize,
    },
    /// The last 16 bytes are not the MD5 of every byte before them.
    #[error("shader pack: its last 16 bytes aren't the MD5 of the bytes before them")]
    Digest,
}

/// The pack's containers, in stored order, each checked.
pub fn containers(pack: &[u8]) -> Result<Vec<&[u8]>, Error> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while pack.get(at..at.saturating_add(4)) == Some(b"DXBC".as_slice()) {
        let index = out.len();
        let size = pack
            .get(at.saturating_add(24)..at.saturating_add(28))
            .and_then(|w| w.try_into().ok())
            .map(u32::from_le_bytes)
            .and_then(|s| usize::try_from(s).ok())
            .ok_or(Error::Container {
                index,
                at,
                why: "short header",
            })?;
        let end = at.checked_add(size).ok_or(Error::Container {
            index,
            at,
            why: "size past the pack",
        })?;
        let container = pack.get(at..end).ok_or(Error::Container {
            index,
            at,
            why: "size past the pack",
        })?;
        let stored = container.get(4..20);
        if size < 32 || dxcontainer::checksum(container).as_ref().map(|c| c.as_slice()) != stored {
            return Err(Error::Checksum { index, at });
        }
        out.push(container);
        at = end;
    }
    let left = pack.len().saturating_sub(at);
    if left != 16 {
        return Err(Error::Trailer { left });
    }
    if pack.get(at..) != Some(primase_foundations::hash::md5(pack.get(..at).unwrap_or_default()).as_slice()) {
        return Err(Error::Digest);
    }
    Ok(out)
}
