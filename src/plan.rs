//! The plan (specification section 2): the bwrap argument list with the file descriptor
//! positions as symbols (section 14). Pure.

use std::ffi::OsString;
use std::path::Path;

use crate::layers::Directive;
use crate::mounts::{EntryKind, ResolvedItem};
use crate::policy::NetworkMode;

/// One bwrap argument. The descriptors are symbols: their numbers are assigned right
/// before the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    Literal(OsString),
    /// The descriptor of the empty file a `hide` of a file is bound from.
    EmptyFile,
    /// The descriptor the seccomp filter is read from.
    Seccomp,
}

impl Argument {
    fn text(text: impl Into<OsString>) -> Self {
        Argument::Literal(text.into())
    }
}

/// The bwrap arguments: the fixed part in the order of specification section 14, then the
/// mount items in the order they were resolved. No argument sets an environment variable.
pub fn bwrap_arguments(
    network_mode: NetworkMode,
    current_dir: &Path,
    items: &[ResolvedItem],
) -> Vec<Argument> {
    let mut arguments = vec![
        Argument::text("--ro-bind"),
        Argument::text("/"),
        Argument::text("/"),
        Argument::text("--dev"),
        Argument::text("/dev"),
        Argument::text("--proc"),
        Argument::text("/proc"),
        Argument::text("--unshare-all"),
    ];
    if network_mode == NetworkMode::Host {
        arguments.push(Argument::text("--share-net"));
    }
    arguments.extend([
        Argument::text("--die-with-parent"),
        Argument::text("--chdir"),
        Argument::text(current_dir),
        Argument::text("--seccomp"),
        Argument::Seccomp,
    ]);
    for item in items {
        let real = Argument::text(item.real.as_os_str());
        match (item.directive, item.kind) {
            (Directive::Rw | Directive::RwFile, _) => {
                arguments.extend([Argument::text("--bind"), real.clone(), real]);
            }
            (Directive::Ro, _) => {
                arguments.extend([Argument::text("--ro-bind"), real.clone(), real]);
            }
            (Directive::Hide, EntryKind::Directory) => {
                arguments.extend([Argument::text("--tmpfs"), real]);
            }
            (Directive::Hide, EntryKind::NotDirectory) => {
                arguments.extend([Argument::text("--ro-bind-data"), Argument::EmptyFile, real]);
            }
        }
    }
    arguments
}
