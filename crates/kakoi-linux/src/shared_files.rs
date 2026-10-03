//! The shared file place `$XDG_RUNTIME_DIR/kakoi/` (specification REQ-460 to REQ-462): the
//! real files a run binds read-only where it would otherwise mount a file made from
//! data, the empty file of a `hide` and the resolver configuration of filtered. A file
//! made from data cannot be mounted over again by a kakoi nested inside, a real file
//! can. Only a run that is not nested uses the place; the others, and a run that finds
//! the place unfit, make the same files from data.
//!
//! Which place a plan names is decided from values; checking the place and making it
//! again is done on the file system, right before bwrap starts.

use std::fs::{self, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::plan::Argument;
pub use kakoi_plan::shared_files::{place, SharedFile};

/// `arguments` ready to start: when they bind files of the place, the place is checked and
/// the files made again as needed; if it then holds, each such file is its path,
/// otherwise its bind becomes the same file put from data. Arguments without a file of
/// the place are returned as they are, and the place is not touched.
pub fn settle(arguments: &[Argument]) -> Vec<Argument> {
    let mut files: Vec<SharedFile> = Vec::new();
    let mut place = None;
    for argument in arguments {
        if let Argument::SharedFile { file, path } = argument {
            files.push(*file);
            place = place.or_else(|| path.parent().map(Path::to_path_buf));
        }
    }
    let Some(place) = place else {
        return arguments.to_vec();
    };
    files.sort();
    files.dedup();
    if put_in_place(&place, &files) {
        arguments
            .iter()
            .map(|argument| match argument {
                Argument::SharedFile { path, .. } => Argument::Literal(path.clone().into()),
                other => other.clone(),
            })
            .collect()
    } else {
        in_data_form(arguments)
    }
}

/// `arguments` with each read-only bind of a file of the place replaced by the same file
/// put from data.
pub fn in_data_form(arguments: &[Argument]) -> Vec<Argument> {
    let mut result: Vec<Argument> = Vec::with_capacity(arguments.len());
    for argument in arguments {
        match argument {
            Argument::SharedFile { file, .. } => {
                // Every file of the place follows the `--ro-bind` that binds it.
                result.pop();
                result.push(Argument::Literal("--ro-bind-data".into()));
                result.push(file.from_data());
            }
            other => result.push(other.clone()),
        }
    }
    result
}

/// Makes `place` fit for `files`, and tells whether it is: the directory is made 0700 when
/// missing and must then be the user's own, not a link, and 0700; each file is made again
/// when it is not the user's own regular file of mode 0600 with its content, and must be
/// one afterwards (specification REQ-461).
fn put_in_place(place: &Path, files: &[SharedFile]) -> bool {
    // SAFETY: `getuid` has no preconditions.
    let uid = unsafe { libc::getuid() };
    if fs::symlink_metadata(place).is_err() {
        // Another run may make it at the same moment; what is there is checked below.
        let _ = fs::DirBuilder::new().mode(0o700).create(place);
    }
    let fit_directory =
        fs::symlink_metadata(place).is_ok_and(|metadata| is_fit_directory(&metadata, uid));
    fit_directory
        && files.iter().all(|&file| {
            let path = place.join(file.name());
            is_fit_file(&path, file, uid)
                || (make_again(place, file) && is_fit_file(&path, file, uid))
        })
}

fn is_fit_directory(metadata: &Metadata, uid: u32) -> bool {
    metadata.file_type().is_dir()
        && metadata.uid() == uid
        && metadata.permissions().mode() & 0o7777 == 0o700
}

fn is_fit_file(path: &Path, file: SharedFile, uid: u32) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    let expected = file.content();
    if !(metadata.file_type().is_file()
        && metadata.uid() == uid
        && metadata.permissions().mode() & 0o7777 == 0o600
        && metadata.len() == expected.len() as u64)
    {
        return false;
    }
    let mut content = Vec::new();
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .and_then(|opened| {
            opened
                .take(expected.len() as u64 + 1)
                .read_to_end(&mut content)
        })
        .is_ok_and(|_| content == expected)
}

/// Writes `file` under a name of this run's own and renames it into place, so that a run
/// starting at the same moment sees either the old file or the whole new one.
fn make_again(place: &Path, file: SharedFile) -> bool {
    let temporary = place.join(format!(".{}.{}", file.name(), std::process::id()));
    // Left by an earlier process of the same number, which has ended.
    let _ = fs::remove_file(&temporary);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut opened| {
            opened.write_all(&file.content())?;
            // The mask of the process may have taken bits off the mode asked for.
            opened.set_permissions(fs::Permissions::from_mode(0o600))
        });
    let renamed = written.and_then(|()| fs::rename(&temporary, place.join(file.name())));
    if renamed.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    renamed.is_ok()
}
