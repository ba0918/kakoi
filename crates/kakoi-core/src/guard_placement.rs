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

/// What the outer layer found at one name of a program on the isolation's `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameFact {
    /// The name, under an entry of `PATH`.
    pub candidate: PathBuf,
    /// The name with the links of its directory resolved, not its own: where the name
    /// itself is, which a `hide` item may cover even when what it resolves to is
    /// visible. None when the directory cannot be resolved.
    pub name: Option<PathBuf>,
    /// The real path it resolves to; none for a dangling link.
    pub real: Option<PathBuf>,
    /// Whether that is a regular file this process may execute.
    pub executable: bool,
    /// Which file that is; none when it cannot be told.
    pub file: Option<FileId>,
}

/// The real program among `names` (in the order of `PATH`): the first the shell inside
/// would start by that name, one whose name and what it resolves to are not hidden by
/// `mounts` and that resolves to an executable regular file (specification REQ-446).
/// Directories, dangling links, regular files that cannot be executed, and names whose
/// place or real file is hidden are passed over.
pub fn real_program<'a>(names: &'a [NameFact], mounts: &[ResolvedItem]) -> Option<&'a NameFact> {
    names.iter().find(|fact| {
        fact.executable
            && fact
                .real
                .as_deref()
                .is_some_and(|real| !hidden(real, mounts))
            && !fact
                .name
                .as_deref()
                .is_some_and(|name| hidden(name, mounts))
    })
}

/// A file told apart from every other by its device and inode, whatever path names it:
/// two hard links to one file are the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileId {
    pub device: u64,
    pub inode: u64,
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

/// Decides the guards of `guards` (the merged rules) from the names found on `PATH`
/// (`facts`, by program, in the order of `PATH`; `path_present` false when the isolation
/// has no `PATH`), the mount items as resolved, and the real path of kakoi itself and
/// which file it is.
pub fn place_guards(
    guards: &[GuardEntry],
    path_present: bool,
    facts: &BTreeMap<String, Vec<NameFact>>,
    mounts: &[ResolvedItem],
    kakoi: Option<&Path>,
    kakoi_file: Option<FileId>,
) -> GuardPlan {
    let mut plan = GuardPlan {
        executable: kakoi.map(Path::to_path_buf),
        ..GuardPlan::default()
    };
    // The programs with their real path, in the order their first rule is written.
    let mut found: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for program in programs(guards) {
        match check(
            path_present,
            facts.get(program).map_or(&[][..], Vec::as_slice),
            mounts,
            kakoi,
            kakoi_file,
        ) {
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
    names: &[NameFact],
    mounts: &[ResolvedItem],
    kakoi: Option<&Path>,
    kakoi_file: Option<FileId>,
) -> Result<(PathBuf, PathBuf), &'static str> {
    if !path_present {
        return Err("the isolation's environment has no PATH");
    }
    let Some(NameFact {
        candidate,
        real: Some(real),
        file,
        ..
    }) = real_program(names, mounts)
    else {
        return Err("not found on the isolation's PATH");
    };
    if kakoi == Some(real.as_path()) || (file.is_some() && *file == kakoi_file) {
        return Err("the program found on PATH is kakoi itself");
    }
    Ok((candidate.clone(), real.clone()))
}

/// Whether the last mount item that covers `path` (the item's path or one of its
/// ancestors) hides it.
fn hidden(path: &Path, mounts: &[ResolvedItem]) -> bool {
    mounts
        .iter()
        .rev()
        .find(|item| path.starts_with(&item.real))
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
