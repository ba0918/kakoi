//! The plan (specification section 2): the isolation resolved in the order of stage 7 of
//! section 13, and the bwrap argument list with the file descriptor positions as symbols
//! (section 14). Pure.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::isolated_env::{assemble_environment, Environment, SecretFile};
use crate::layers::{Directive, Layer, Policy};
use crate::mounts::{
    loaded_policy_files, resolve_mounts, EntryKind, ExpandedPolicy, MountFacts, ResolvedItem,
    ResolvedMounts,
};
use crate::placement::{check_placement, protected_paths};
use crate::policy::NetworkMode;
use crate::variables::Variables;

/// Everything stage 7 decides from besides the facts: the layers and their merge, the
/// expansion, the variables, the host.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub layers: &'a [Layer],
    pub policy: &'a Policy,
    pub expanded: &'a ExpandedPolicy,
    pub variables: &'a Variables,
    pub home: &'a HomeDirectory,
    /// The configuration directory as derived, before realisation.
    pub config_dir: &'a Path,
    pub current_dir: &'a Path,
    pub host: &'a BTreeMap<OsString, OsString>,
}

/// The facts stage 7 needs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IsolationFacts {
    pub mounts: MountFacts,
    pub secrets: BTreeMap<String, SecretFile>,
}

/// The isolation as resolved: the mount items, the environment, the warnings so far, and
/// the real paths of the policy files read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isolation {
    pub mounts: ResolvedMounts,
    pub environment: Environment,
    pub warnings: Vec<Warning>,
    pub policy_files: Vec<PathBuf>,
}

/// Stage 7 of specification section 13, in its order: the identity and the kinds of the
/// mount items, the placement rules, then the secrets and the git count while assembling
/// the environment. The first diagnostic ends it.
pub fn resolve_isolation(inputs: &Inputs, facts: &IsolationFacts) -> Result<Isolation, Diagnostic> {
    let mounts = resolve_mounts(
        inputs.expanded,
        inputs.layers,
        inputs.variables,
        &facts.mounts,
    )?;
    let protected = protected_paths(inputs.expanded, inputs.layers, inputs.config_dir);
    let mut warnings = check_placement(
        &mounts,
        &protected,
        inputs.variables,
        inputs.home,
        inputs.current_dir,
        &facts.mounts,
    )?;
    // A `path-prepend` entry enters `PATH` as its real path; one that names nothing is
    // skipped like a mount item would be (specification section 5.2).
    let path_prepend: Vec<PathBuf> = inputs
        .expanded
        .path_prepend
        .iter()
        .filter_map(|entry| entry.path())
        .filter_map(|entry| facts.mounts.entry(entry).path().map(Path::to_path_buf))
        .collect();
    let assembled =
        assemble_environment(inputs.policy, inputs.host, &facts.secrets, &path_prepend)?;
    warnings.extend(assembled.warnings);
    Ok(Isolation {
        mounts,
        environment: assembled.environment,
        warnings,
        policy_files: loaded_policy_files(inputs.layers, &facts.mounts),
    })
}

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
