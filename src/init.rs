//! Writing the built-in default out: the `init` form of specification section 4.1. This is
//! the one place the product writes, and it writes only the components of the assembled
//! configuration directory path, its `profile/` and `secrets/`, and the file itself
//! (section 14). Outer layer.

use std::fs::{self, DirBuilder, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::environment::PathState;
use crate::layers::BUILT_IN_DEFAULT;
use crate::startup::InitRequest;
use crate::workspace_facts::entry_state;

/// The mode `secrets/` is made with (specification section 4.1): its owner alone reads it.
const SECRETS_MODE: u32 = 0o700;

/// Writes the built-in default under the configuration directory, making the directories
/// that are not there, and returns the path as assembled: the line the run prints, with the
/// symbolic links left unresolved (specification section 4.1).
pub fn write_built_in_default(request: &InitRequest) -> Result<PathBuf, Diagnostic> {
    let mut components: Vec<&Path> = request.config_dir.ancestors().collect();
    components.reverse();
    for component in components {
        make_directory(component, None)?;
    }
    let profiles = request.config_dir.join("profile");
    make_directory(&profiles, None)?;
    make_directory(&request.config_dir.join("secrets"), Some(SECRETS_MODE))?;
    let path = profiles.join(format!("{}.toml", request.name));
    write_file(&path, BUILT_IN_DEFAULT)?;
    Ok(path)
}

/// Makes `path` a directory when nothing is there, with `mode` if one is given. Links on
/// the way and at the name are followed, because a user keeps the configuration directory
/// in dotfiles behind one; anything that is not a directory in the end stops the run
/// (specification section 4.1).
fn make_directory(path: &Path, mode: Option<u32>) -> Result<(), Diagnostic> {
    let failure = |reason: &str| Diagnostic::path(format!("{} {reason}", path.display()));
    match entry_state(path) {
        PathState::Directory(_) => return Ok(()),
        // A name behind which no real path is reached: a dangling link, or one whose
        // parent has no search bit. The two are told apart by nothing here, and under the
        // second the name may not be there at all, so the reason names both.
        PathState::Broken => return Err(failure(
            "cannot be made a directory: something is at the name, or the name cannot be looked at",
        )),
        PathState::NotDirectory(_) => return Err(failure("is not a directory")),
        PathState::Absent => {}
    }
    let mut builder = DirBuilder::new();
    if let Some(mode) = mode {
        builder.mode(mode);
    }
    builder
        .create(path)
        .map_err(|error| failure(&format!("cannot be created: {error}")))?;
    // The umask narrows what the builder asks for, so the mode is set once the directory
    // is there.
    if let Some(mode) = mode {
        fs::set_permissions(path, Permissions::from_mode(mode))
            .map_err(|error| failure(&format!("cannot be given the mode {mode:o}: {error}")))?;
    }
    Ok(())
}

/// Writes `text` at `path`, refusing to replace anything already there whatever its kind,
/// the name itself not followed (specification section 4.1). `O_EXCL` is the refusal that
/// counts; the check before it is only there to say what is in the way.
fn write_file(path: &Path, text: &str) -> Result<(), Diagnostic> {
    let failure = |reason: &str| Diagnostic::path(format!("{} {reason}", path.display()));
    match entry_state(path) {
        PathState::Absent => {}
        PathState::Broken => {
            return Err(failure(
                "cannot be written: something is at the name, or the name cannot be looked at",
            ))
        }
        PathState::Directory(_) | PathState::NotDirectory(_) => {
            return Err(failure("already exists"))
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| failure(&format!("cannot be written: {error}")))?;
    file.write_all(text.as_bytes())
        .map_err(|error| failure(&format!("cannot be written: {error}")))
}
