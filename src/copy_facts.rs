//! Reads what each `rw-copy` item that applies starts the isolation with (specification
//! sections 6.1 and 14): the bytes and the mode of a regular file, or the tree under a
//! directory. The outer layer; what becomes an argument is decided in `plan`.
//!
//! It reads, and never writes: the isolation gets a copy in memory and the host keeps what
//! it had. A source that cannot be read is a `path` diagnostic rather than a copy that
//! quietly starts with less than the host has; an entry of a kind no argument can recreate
//! is reported in the plan and the run goes on.

use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::copies::{
    CopiedEntry, CopySource, CopySources, FileContent, NotCopied, BYTE_LIMIT, ENTRY_LIMIT,
};
use crate::diagnostic::Diagnostic;
use crate::layers::Directive;
use crate::mounts::{EntryKind, ResolvedItem};

/// Reads the source of every `rw-copy` item of `items`, in the order the items are
/// mounted. An item whose real path names something that is neither a directory nor a
/// regular file, a directory that cannot be listed, a file that cannot be read, or a
/// source over the limits of `copies` ends the run with a `path` diagnostic.
pub fn read_copy_sources(items: &[ResolvedItem]) -> Result<CopySources, Diagnostic> {
    let mut collected = CopySources::default();
    for item in items {
        if item.directive != Directive::RwCopy {
            continue;
        }
        let mut budget = Budget::new(item);
        let source = match item.kind {
            EntryKind::Directory => {
                let mut entries = Vec::new();
                read_directory(
                    &item.real,
                    Path::new(""),
                    item,
                    &mut budget,
                    &mut entries,
                    &mut collected.not_copied,
                )?;
                CopySource::Directory(entries)
            }
            EntryKind::NotDirectory => {
                let (mode, content) = read_file(&item.real, item, &mut budget)?;
                CopySource::File { mode, content }
            }
        };
        collected.sources.insert(item.real.clone(), source);
    }
    Ok(collected)
}

/// What one item may still take: the entries and the bytes left of its limits.
struct Budget {
    entries: usize,
    bytes: u64,
    written: String,
    real: PathBuf,
}

impl Budget {
    fn new(item: &ResolvedItem) -> Self {
        Self {
            entries: ENTRY_LIMIT,
            bytes: BYTE_LIMIT,
            written: item.written.clone(),
            real: item.real.clone(),
        }
    }

    /// Takes one entry, or ends the run: a source this large is a path pointed at the
    /// wrong thing, and the isolation would hold all of it in memory.
    fn take_entry(&mut self) -> Result<(), Diagnostic> {
        self.entries = self.entries.checked_sub(1).ok_or_else(|| {
            Diagnostic::path(format!(
                "the `rw-copy` item `{}` at {} holds more than {ENTRY_LIMIT} entries; \
                 name something smaller, or use `ro` to show it without copying it",
                self.written,
                self.real.display()
            ))
        })?;
        Ok(())
    }

    /// What the item may still spend on file content.
    fn remaining(&self) -> u64 {
        self.bytes
    }

    /// Spends `bytes` of file content, or ends the run: a source this large is a path
    /// pointed at the wrong thing, and the isolation would hold all of it in memory.
    fn spend(&mut self, bytes: u64, path: &Path) -> Result<(), Diagnostic> {
        self.bytes = self.bytes.checked_sub(bytes).ok_or_else(|| {
            Diagnostic::path(format!(
                "the `rw-copy` item `{}` at {} holds more than {BYTE_LIMIT} bytes (reached at \
                 {}); name something smaller, or use `ro` to show it without copying it",
                self.written,
                self.real.display(),
                path.display()
            ))
        })?;
        Ok(())
    }
}

/// Reads `directory` into `entries`, at `relative` under the item's real path, each
/// directory before what is under it. Entries are taken in the byte order of their names,
/// so the same tree always gives the same arguments. A symbolic link is reproduced as a
/// link and not followed, so no walk leaves the tree or meets a loop.
fn read_directory(
    directory: &Path,
    relative: &Path,
    item: &ResolvedItem,
    budget: &mut Budget,
    entries: &mut Vec<CopiedEntry>,
    not_copied: &mut Vec<NotCopied>,
) -> Result<(), Diagnostic> {
    let read = fs::read_dir(directory).map_err(|error| unreadable(item, directory, error))?;
    let mut found: Vec<_> = read
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| unreadable(item, directory, error))?;
    found.sort_by_key(std::fs::DirEntry::file_name);
    for entry in found {
        let path = entry.path();
        let relative = relative.join(entry.file_name());
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| unreadable(item, &path, error))?;
        let kind = metadata.file_type();
        budget.take_entry()?;
        if kind.is_symlink() {
            let target = fs::read_link(&path).map_err(|error| unreadable(item, &path, error))?;
            entries.push(CopiedEntry::Symlink { relative, target });
        } else if kind.is_dir() {
            entries.push(CopiedEntry::Directory {
                relative: relative.clone(),
                mode: mode_of(&metadata),
            });
            read_directory(&path, &relative, item, budget, entries, not_copied)?;
        } else if kind.is_file() {
            let (mode, content) = read_file(&path, item, budget)?;
            entries.push(CopiedEntry::File {
                relative,
                mode,
                content,
            });
        } else {
            // A socket, a FIFO, or a device node: a copy of one is not the host's, and no
            // argument makes one in a tmpfs. Left out, and said so in the plan.
            not_copied.push(NotCopied {
                item: item.real.clone(),
                path,
                reason: "is not a regular file, a directory, or a symbolic link".to_string(),
            });
        }
    }
    Ok(())
}

/// Reads one regular file with its mode, within what `budget` has left. The kind is
/// checked on the open descriptor, and the open neither waits nor follows a link at the
/// path itself, so nothing put there between the resolution and here stops the start-up or
/// leads the read somewhere else. One byte more than the budget allows is read, so a file
/// over the limit is refused rather than silently cut short.
fn read_file(
    path: &Path,
    item: &ResolvedItem,
    budget: &mut Budget,
) -> Result<(u32, FileContent), Diagnostic> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| unreadable(item, path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| unreadable(item, path, error))?;
    if !metadata.file_type().is_file() {
        return Err(Diagnostic::path(format!(
            "the `rw-copy` item `{}` at {} cannot be copied: {} is not a regular file or a \
             directory, so there is no content to copy; use `rw-file` for a socket or a FIFO",
            item.written,
            item.real.display(),
            path.display()
        )));
    }
    let allowance = budget.remaining();
    let mut bytes = Vec::new();
    file.take(allowance + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| unreadable(item, path, error))?;
    budget.spend(bytes.len() as u64, path)?;
    Ok((mode_of(&metadata), FileContent::new(bytes)))
}

/// The permission bits to give the copy; the set-user, set-group, and sticky bits are
/// carried too, though the isolation's tmpfs is mounted `nosuid`.
fn mode_of(metadata: &fs::Metadata) -> u32 {
    metadata.permissions().mode() & 0o7777
}

/// A source that cannot be read ends the run: the directive promises the host's content,
/// and starting with less than that, without a word, is the one outcome to avoid.
fn unreadable(item: &ResolvedItem, path: &Path, error: std::io::Error) -> Diagnostic {
    Diagnostic::path(format!(
        "the `rw-copy` item `{}` at {} cannot be copied: {} cannot be read: {error}",
        item.written,
        item.real.display(),
        path.display()
    ))
}
