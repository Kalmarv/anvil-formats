//! Several forges read as one: a ClassID is looked up in each, in the order the caller gives them, and the first
//! that holds it answers. Which copy wins where a base and its patch both hold an id is the caller's choice
//! (the Riders Republic survey finds the patch's copy differing only in some LODSelectors' flags, TextureMapSpecs and a
//! few others, and leads to "the patch wins"; that is not established from the game), so nothing here orders them.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "reader I/O: opens the forge files the caller names"
)]

use std::fs::File;
use std::path::Path;

use crate::data::{Resource, read_entry};
use crate::forge::Forge;

/// The largest decoded size of a block an [`Archive`] accepts unless told otherwise (64 MiB). A block that declares
/// more is refused with [`crate::data::Error::Size`], never truncated; [`Archive::set_max_record`] raises or lowers the
/// bound for an archive whose resources are larger (or to refuse a hostile one sooner).
pub const MAX_RECORD: usize = 64 << 20;

/// Why an archive can't be opened or an entry read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Forge(#[from] crate::forge::Error),
    #[error("no forge {0} in the archive")]
    NoForge(usize),
    #[error("forge {forge}, entry {entry}: {source}")]
    Data {
        forge: usize,
        entry: usize,
        #[source]
        source: crate::data::Error,
    },
}

/// Forges searched in a stated order.
#[derive(Debug)]
pub struct Archive {
    forges: Vec<(Forge, File)>,
    max_record: usize,
}

/// An entry of one of an archive's forges: the forge's position in the order given, and the entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct At {
    pub forge: usize,
    pub entry: usize,
}

impl Archive {
    /// The forges at `paths`, searched in that order.
    pub fn open(paths: &[impl AsRef<Path>]) -> Result<Archive, Error> {
        let forges = paths
            .iter()
            .map(|p| Ok((Forge::open(p)?, File::open(p)?)))
            .collect::<Result<_, Error>>()?;
        Ok(Archive {
            forges,
            max_record: MAX_RECORD,
        })
    }

    /// The largest decoded size [`Archive::resource`] accepts for a block ([`MAX_RECORD`] until set).
    pub fn max_record(&self) -> usize {
        self.max_record
    }

    /// Accept blocks of up to `bytes` decoded bytes from now on. The bound is what an entry's declared sizes are held
    /// to before memory is made for them, so it is also the most one [`Archive::resource`] allocates for a block.
    pub fn set_max_record(&mut self, bytes: usize) {
        self.max_record = bytes;
    }

    /// The first forge, in the order given, holding `id`, and the entry there.
    pub fn find(&self, id: u64) -> Option<At> {
        self.forges
            .iter()
            .enumerate()
            .find_map(|(forge, (f, _))| f.find(id).map(|entry| At { forge, entry }))
    }

    /// The forge at position `forge`.
    pub fn forge(&self, forge: usize) -> Option<&Forge> {
        self.forges.get(forge).map(|(f, _)| f)
    }

    /// The resource at `at`: its entry decoded (each block of at most [`Archive::max_record`] bytes) and its header
    /// parsed.
    pub fn resource(&mut self, at: At) -> Result<Resource, Error> {
        let (forge, file) = self.forges.get_mut(at.forge).ok_or(Error::NoForge(at.forge))?;
        let payload = forge.payload(file, at.entry)?;
        let data = |source| Error::Data {
            forge: at.forge,
            entry: at.entry,
            source,
        };
        let block = read_entry(&payload, self.max_record).map_err(data)?;
        Resource::parse(&block.resource.data).map_err(data)
    }
}
