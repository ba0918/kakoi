//! Where the command guard is placed: for each program a rule names, the real program on
//! the isolation's `PATH`, whether a guard goes in front of it or the program is skipped
//! with a reason, and the table the guard reads when it starts. Pure: what is on `PATH`
//! is a fact from `executables`.

use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::guard::GuardRule;

use crate::layers::{Directive, GuardEntry, LayerOrigin};
use crate::mounts::ResolvedItem;

/// The kakoi-only tmpfs the guards and the relocated programs are placed in,
/// under the tmpfs bwrap makes for `/dev` so that it covers nothing of the host.
pub const GUARD_ROOT: &str = "/dev/kakoi-guard";

/// The directory of the guards, put first on `PATH`. Nothing else is in it: any file
/// there would be a command on `PATH`.
pub const GUARD_LOCATION: &str = "/dev/kakoi-guard/bin";

/// The table a guard reads to learn what it guards, in the same tmpfs.
pub const GUARD_TABLE: &str = "/dev/kakoi-guard/table";

/// Where a program relocated for `guard-absolute-path` goes, one directory per program.
const RELOCATED: &str = "/dev/kakoi-guard/real";

/// What the outer layer found for a program on the isolation's `PATH`: nothing, or the
/// first name found, with what it resolves to and whether that is a regular file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramFact {
    NotFound,
    Found {
        /// The name found, under an entry of `PATH`.
        candidate: PathBuf,
        /// The real path it resolves to; none for a dangling link.
        real: Option<PathBuf>,
        /// Whether that is a regular file.
        regular: bool,
    },
}

/// A program with a guard in front of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedGuard {
    pub program: String,
    /// The directory of the guard, first on `PATH`.
    pub location: PathBuf,
    /// The real program as found on `PATH`.
    pub found: PathBuf,
    /// Where the real program is placed again when a rule asks for `guard-absolute-path`:
    /// its own path then holds a guard.
    pub relocated: Option<PathBuf>,
    /// Where each rule applied to this program came from.
    pub sources: Vec<LayerOrigin>,
}

impl PlacedGuard {
    /// The guard itself: the program's name in the guard location.
    pub fn guard(&self) -> PathBuf {
        self.location.join(&self.program)
    }
}

/// A program with a rule and no guard, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedGuard {
    pub program: String,
    pub reason: String,
}

/// The guards of a run: those placed, those skipped, the paths of the real programs to
/// overlay with a guard (`guard-absolute-path`) and where each is relocated to, and the
/// table the guards read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuardPlan {
    pub placed: Vec<PlacedGuard>,
    pub skipped: Vec<SkippedGuard>,
    /// Each real program overlaid with a guard, and where it is relocated.
    pub overlaid: Vec<(PathBuf, PathBuf)>,
    pub table: GuardTable,
    /// The real path of kakoi's own executable, which every guard is.
    pub executable: Option<PathBuf>,
}

/// What the guards read when they start: for each path a guard is started from, the
/// program to execute when nothing is denied and the rules to apply. Paths are bytes so
/// that a name that is not UTF-8 survives.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GuardTable {
    pub entries: Vec<TableEntry>,
}

/// One path a guard is started from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableEntry {
    pub location: Vec<u8>,
    pub execute: Vec<u8>,
    pub rules: Vec<GuardRule>,
}

impl GuardTable {
    /// The table as the guard reads it.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("the table serializes")
    }

    /// The table in `bytes`; none when they are not one.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// The entry of a guard started from `location`.
    pub fn entry(&self, location: &Path) -> Option<&TableEntry> {
        self.entries
            .iter()
            .find(|entry| entry.location == location.as_os_str().as_bytes())
    }
}

/// Decides the guards of `guards` (the merged rules) from what was found on `PATH`
/// (`facts`, by program; `path_present` false when the isolation has no `PATH`), the
/// mount items as resolved, and the real path of kakoi itself.
pub fn place_guards(
    guards: &[GuardEntry],
    path_present: bool,
    facts: &BTreeMap<String, ProgramFact>,
    mounts: &[ResolvedItem],
    kakoi: Option<&Path>,
) -> GuardPlan {
    let mut plan = GuardPlan {
        executable: kakoi.map(Path::to_path_buf),
        ..GuardPlan::default()
    };
    // The programs with their real path, in the order their first rule is written.
    let mut found: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for program in programs(guards) {
        match check(path_present, facts.get(program), mounts, kakoi) {
            Ok((candidate, real)) => found.push((program.to_string(), candidate, real)),
            Err(reason) => plan.skipped.push(SkippedGuard {
                program: program.to_string(),
                reason: reason.to_string(),
            }),
        }
    }
    // Every program that resolves to the same real program gets the rules of all of them.
    let rules_of = |real: &Path| -> Vec<&GuardEntry> {
        guards
            .iter()
            .filter(|entry| {
                found
                    .iter()
                    .any(|(program, _, other)| other == real && *program == entry.rule.program)
            })
            .collect()
    };
    let mut relocations: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (program, candidate, real) in &found {
        let entries = rules_of(real);
        let relocated = entries
            .iter()
            .any(|entry| entry.rule.guard_absolute_path)
            .then(|| relocation(&mut relocations, real));
        let placed = PlacedGuard {
            program: program.clone(),
            location: PathBuf::from(GUARD_LOCATION),
            found: candidate.clone(),
            relocated: relocated.clone(),
            sources: entries.iter().map(|entry| entry.origin.clone()).collect(),
        };
        plan.table.entries.push(TableEntry {
            location: bytes(&placed.guard()),
            execute: bytes(relocated.as_deref().unwrap_or(candidate)),
            rules: entries.iter().map(|entry| entry.rule.clone()).collect(),
        });
        plan.placed.push(placed);
    }
    for (real, relocated) in &relocations {
        plan.table.entries.push(TableEntry {
            location: bytes(real),
            execute: bytes(relocated),
            rules: rules_of(real)
                .iter()
                .map(|entry| entry.rule.clone())
                .collect(),
        });
    }
    plan.overlaid = relocations;
    plan
}

/// The programs `guards` name, each once, in the order of their first rule.
fn programs(guards: &[GuardEntry]) -> Vec<&str> {
    let mut programs: Vec<&str> = Vec::new();
    for entry in guards {
        if !programs.contains(&entry.rule.program.as_str()) {
            programs.push(&entry.rule.program);
        }
    }
    programs
}

/// The found name and the real path of `program`, or why it gets no guard.
fn check(
    path_present: bool,
    fact: Option<&ProgramFact>,
    mounts: &[ResolvedItem],
    kakoi: Option<&Path>,
) -> Result<(PathBuf, PathBuf), &'static str> {
    if !path_present {
        return Err("the isolation's environment has no PATH");
    }
    let Some(ProgramFact::Found {
        candidate,
        real,
        regular,
    }) = fact
    else {
        return Err("not found on the isolation's PATH");
    };
    let Some(real) = real.as_ref().filter(|_| *regular) else {
        return Err("the program found on PATH is not a regular file");
    };
    if hidden(real, mounts) {
        return Err("the program is hidden by a `hide` mount item");
    }
    if kakoi == Some(real.as_path()) {
        return Err("the program found on PATH is kakoi itself");
    }
    Ok((candidate.clone(), real.clone()))
}

/// Whether the last mount item that covers `real` (the item's path or one of its
/// ancestors) hides it.
fn hidden(real: &Path, mounts: &[ResolvedItem]) -> bool {
    mounts
        .iter()
        .rev()
        .find(|item| real.starts_with(&item.real))
        .is_some_and(|item| item.directive == Directive::Hide)
}

/// Where `real` is relocated, the same place for every program that resolves to it.
fn relocation(relocations: &mut Vec<(PathBuf, PathBuf)>, real: &Path) -> PathBuf {
    if let Some((_, relocated)) = relocations.iter().find(|(other, _)| other == real) {
        return relocated.clone();
    }
    let name = real.file_name().unwrap_or(real.as_os_str());
    let relocated = Path::new(RELOCATED)
        .join(relocations.len().to_string())
        .join(name);
    relocations.push((real.to_path_buf(), relocated.clone()));
    relocated
}

fn bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}
