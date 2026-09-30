//! The command mode "listed" (specification core-listed-commands.md): the programs a run
//! lets start, and where the isolation's first process and its list are placed. Pure:
//! what is on the host arrives as `MountFacts`.

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::environment::{HomeDirectory, RealEntry};
use crate::layers::Policy;
use crate::listed::ListedRoot;
use crate::mounts::{expand, Expansion, MountFacts};
use crate::variables::Variables;

/// Where kakoi's own executable is placed as the isolation's first process, in the
/// kakoi-only tmpfs of the command guards.
pub const FIRST_PROCESS: &str = "/dev/kakoi-guard/first";

/// The list of the programs the first process allows, in the same tmpfs.
pub const ALLOWED_LIST: &str = "/dev/kakoi-guard/allowed";

/// The dynamic linker kakoi allows by itself: without it no dynamically linked program
/// starts.
pub const DYNAMIC_LINKER: &str = "/lib64/ld-linux-x86-64.so.2";

/// The programs a "listed" command mode lets start, as the plan decided them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLimits {
    /// The items of `commands.allow` that the host has and the isolation shows, as paths
    /// the first process opens inside.
    pub allowed: Vec<PathBuf>,
    /// The items skipped, with the reason.
    pub skipped: Vec<SkippedAllow>,
    /// The real path of kakoi's own executable, placed as the first process.
    pub executable: PathBuf,
}

/// An item of `commands.allow` that is not allowed, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedAllow {
    pub written: String,
    pub reason: String,
}

/// What the first process reads: the paths it allows to be executed besides the ones
/// kakoi allows by itself. Paths are bytes so that a name that is not UTF-8 survives.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AllowedList {
    pub paths: Vec<Vec<u8>>,
}

impl AllowedList {
    pub fn new(paths: &[PathBuf]) -> Self {
        Self {
            paths: paths
                .iter()
                .map(|path| path.as_os_str().as_bytes().to_vec())
                .collect(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("the list serializes")
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

/// The paths the items of `commands.allow` expand to, for the outer layer to look up.
pub fn lookups(policy: &Policy, variables: &Variables, home: &HomeDirectory) -> Vec<PathBuf> {
    policy
        .commands_allow
        .iter()
        .filter_map(|item| expand(item, variables, home).path().map(Path::to_path_buf))
        .collect()
}

/// The items of `commands.allow` that are allowed, and those skipped with the reason: a
/// variable without a value, nothing at the path, or, under the "listed" mount mode, a
/// real path the isolation does not show (specification REQ-476).
pub fn allowed_programs(
    policy: &Policy,
    variables: &Variables,
    home: &HomeDirectory,
    facts: &MountFacts,
    listed: Option<&ListedRoot>,
) -> (Vec<PathBuf>, Vec<SkippedAllow>) {
    let mut allowed = Vec::new();
    let mut skipped = Vec::new();
    for item in &policy.commands_allow {
        let reason = match expand(item, variables, home) {
            Expansion::Valueless(variable) => format!("`${{{}}}` has no value", variable.name()),
            Expansion::Path(path) => match facts.entry(&path) {
                RealEntry::Missing => "does not exist".to_string(),
                entry => {
                    let real = entry.path().expect("an entry that exists");
                    if listed.is_some_and(|root| !root.shows(real)) {
                        "is not among the places the \"listed\" mount mode shows".to_string()
                    } else {
                        allowed.push(path);
                        continue;
                    }
                }
            },
        };
        skipped.push(SkippedAllow {
            written: item.to_string(),
            reason,
        });
    }
    (allowed, skipped)
}
