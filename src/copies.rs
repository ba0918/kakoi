//! What one `rw-copy` item hands the isolation: the host's content at the item's real path,
//! in the shape the bwrap arguments recreate it in (specification section 6.1). A directory
//! becomes a tmpfs seeded entry by entry; a regular file becomes a bound copy of its bytes.
//! Nothing here reaches back to the host, so the isolation's writes end with the run. The
//! reader is `copy_facts`. Pure.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The most bytes of file content one `rw-copy` item carries. The content is held in
/// memory twice over — once in the descriptors handed to bwrap, once in the isolation's
/// tmpfs — so an item pointed at something large is refused rather than paged in.
pub const BYTE_LIMIT: u64 = 64 << 20;

/// The most entries one `rw-copy` item carries. Each regular file also costs one open file
/// descriptor at the start.
pub const ENTRY_LIMIT: usize = 4096;

/// The content of one copied regular file. Shared so that cloning a plan stays cheap, and
/// shown by its length alone so that a `Debug` of a plan never dumps a file.
#[derive(Clone, PartialEq, Eq)]
pub struct FileContent(Arc<[u8]>);

impl FileContent {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Arc::from(bytes))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for FileContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileContent({} bytes)", self.0.len())
    }
}

/// One entry under a copied directory, at `relative` to the item's real path. The entries
/// of a source are ordered so that a directory comes before everything under it: bwrap
/// applies them in order and a mode is only carried by the argument that creates the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopiedEntry {
    Directory {
        relative: PathBuf,
        mode: u32,
    },
    File {
        relative: PathBuf,
        mode: u32,
        content: FileContent,
    },
    /// Reproduced as a link with the same target text, not followed: what the host has is
    /// what the isolation sees.
    Symlink {
        relative: PathBuf,
        target: PathBuf,
    },
}

impl CopiedEntry {
    pub fn relative(&self) -> &Path {
        match self {
            CopiedEntry::Directory { relative, .. }
            | CopiedEntry::File { relative, .. }
            | CopiedEntry::Symlink { relative, .. } => relative,
        }
    }
}

/// What is at one `rw-copy` item's real path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopySource {
    /// A regular file: the mode to give the copy and the bytes to fill it with.
    File { mode: u32, content: FileContent },
    /// A directory: the entries under it, each directory before what is under it.
    Directory(Vec<CopiedEntry>),
}

/// An entry a copy left out, with the reason: a kind no bwrap argument can recreate in a
/// tmpfs (a socket, a FIFO, a device node). Reported in the plan rather than passed over,
/// so that a missing entry is never a surprise from inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotCopied {
    /// The real path of the `rw-copy` item the entry is under.
    pub item: PathBuf,
    pub path: PathBuf,
    pub reason: String,
}

/// The sources of every `rw-copy` item that applies, by the item's real path, and every
/// entry left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CopySources {
    pub sources: BTreeMap<PathBuf, CopySource>,
    pub not_copied: Vec<NotCopied>,
}
