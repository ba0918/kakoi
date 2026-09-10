//! The plan as `--print-plan=json` shows it (specification section 13): one line of JSON
//! for LLM agents and tools, never for a person, with the same content as the full text
//! form and the summary's environment changes. The
//! shape is the contract of that section: the keys named there stay and keep their
//! meaning while `format_version` is 1; keys may be added. Secret values are `null`.
//! Paths and other OS strings that are not valid UTF-8 are shown lossily. Pure.

use std::collections::BTreeMap;
use std::ffi::OsStr;

use serde::Serialize;

use crate::layers::{Directive, LayerOrigin, Policy, PolicySource};
use crate::mounts::{EntryKind, ItemOrigin, SkippedRole};
use crate::plan::{Argument, Plan};
use crate::policy::{EnvMode, NetworkMode, PolicyPath};

/// The version of the shape: bumped when a key is removed or changes its meaning.
pub const FORMAT_VERSION: u32 = 1;

/// The JSON text of `plan`: one line, compact, ending in a newline. Not indented: a
/// person reads the summary, and a tool feeds this to `jq` or an LLM.
pub fn render(plan: &Plan) -> String {
    let document = PlanDocument::from(plan);
    let mut text = serde_json::to_string(&document).expect("the plan document serializes");
    text.push('\n');
    text
}

#[derive(Serialize)]
struct PlanDocument {
    format_version: u32,
    nested: bool,
    policy_sources: Vec<Source>,
    variables: Variables,
    home: String,
    policy: MergedPolicy,
    mounts: Vec<MountItem>,
    skipped_mounts: Vec<SkippedMount>,
    left_visible: Vec<LeftVisible>,
    skipped_paths: Vec<SkippedPath>,
    not_copied: Vec<NotCopied>,
    environment: BTreeMap<String, Option<String>>,
    environment_changes: EnvironmentChanges,
    command: Option<Command>,
    bwrap: String,
    bwrap_arguments: Vec<BwrapArgument>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Source {
    File { path: String },
    BuiltInDefault,
}

#[derive(Serialize)]
struct Variables {
    workspace: String,
    worktree: String,
    git_common_dir: Option<String>,
    config_dir: Option<String>,
}

#[derive(Serialize)]
struct MergedPolicy {
    mounts: Vec<PolicyMount>,
    scan: Vec<Scan>,
    hide_mounts: Vec<HideMounts>,
    network_mode: &'static str,
    env_mode: &'static str,
    env_pass: Vec<String>,
    env_set: BTreeMap<String, String>,
    env_unset: Vec<String>,
    path_prepend: Vec<String>,
    secrets: BTreeMap<String, String>,
    instead_of: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct PolicyMount {
    directive: &'static str,
    path: String,
    origin: Origin,
}

#[derive(Serialize)]
struct Scan {
    root: String,
    names: Vec<String>,
    exclude: Vec<String>,
    prune: Vec<String>,
}

#[derive(Serialize)]
struct HideMounts {
    under: String,
    fstype: Vec<String>,
}

/// Where a policy entry or a mount item came from: one of the three written layers, or
/// one of the generators of specification section 6.3.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Origin {
    Profile { path: String },
    BuiltInDefault,
    PolicyFile { path: String },
    CommandLine,
    Scan,
    HideMounts,
    Secret { name: String },
    ConfigSecrets,
}

#[derive(Serialize)]
struct MountItem {
    directive: &'static str,
    path: String,
    kind: &'static str,
    written: String,
    origin: Origin,
}

#[derive(Serialize)]
struct SkippedMount {
    directive: &'static str,
    written: String,
    origin: Origin,
    reason: String,
}

#[derive(Serialize)]
struct LeftVisible {
    link: String,
    reason: String,
}

#[derive(Serialize)]
struct SkippedPath {
    role: &'static str,
    written: String,
    reason: String,
}

/// An entry an `rw-copy` item could not take from the host: `item` is the item's real
/// path, `path` the entry, `reason` why it is not there inside.
#[derive(Serialize)]
struct NotCopied {
    item: String,
    path: String,
    reason: String,
}

#[derive(Serialize)]
struct EnvironmentChanges {
    mode: &'static str,
    kept: usize,
    unset: Vec<String>,
    set: BTreeMap<String, String>,
    secrets: Vec<String>,
}

#[derive(Serialize)]
struct Command {
    given: String,
    arguments: Vec<String>,
    path: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum BwrapArgument {
    Literal {
        value: String,
    },
    EmptyFile,
    SeccompFilter,
    /// One file of an `rw-copy` item; the content is not shown, only its length.
    CopiedFile {
        bytes: usize,
    },
}

impl From<&Plan> for PlanDocument {
    fn from(plan: &Plan) -> Self {
        let changes = &plan.environment_changes;
        PlanDocument {
            format_version: FORMAT_VERSION,
            nested: plan.nested,
            policy_sources: plan.policy_sources.iter().map(source).collect(),
            variables: Variables {
                workspace: text(&plan.variables.workspace),
                worktree: text(&plan.variables.worktree),
                git_common_dir: plan.variables.git_common_dir.as_deref().map(text),
                config_dir: plan.variables.config_dir.as_deref().map(text),
            },
            home: text(&plan.home),
            policy: merged_policy(&plan.policy),
            mounts: plan
                .mounts
                .items
                .iter()
                .map(|item| MountItem {
                    directive: directive(item.directive),
                    path: text(&item.real),
                    kind: match item.kind {
                        EntryKind::Directory => "directory",
                        EntryKind::NotDirectory => "not-directory",
                    },
                    written: item.written.clone(),
                    origin: item_origin(&item.origin),
                })
                .collect(),
            skipped_mounts: plan
                .mounts
                .skipped
                .iter()
                .map(|item| SkippedMount {
                    directive: directive(item.directive),
                    written: item.written.clone(),
                    origin: layer(&item.origin),
                    reason: item.reason.clone(),
                })
                .collect(),
            left_visible: plan
                .mounts
                .left_visible
                .iter()
                .map(|left| LeftVisible {
                    link: text(&left.link),
                    reason: left.reason.clone(),
                })
                .collect(),
            skipped_paths: plan
                .skipped_paths
                .iter()
                .map(|skipped| SkippedPath {
                    role: match skipped.role {
                        SkippedRole::ScanRoot => "scan-root",
                        SkippedRole::HideMountsUnder => "hide-mounts-under",
                        SkippedRole::PathPrepend => "path-prepend",
                    },
                    written: skipped.written.clone(),
                    reason: skipped.reason.clone(),
                })
                .collect(),
            not_copied: plan
                .not_copied
                .iter()
                .map(|entry| NotCopied {
                    item: text(&entry.item),
                    path: text(&entry.path),
                    reason: entry.reason.clone(),
                })
                .collect(),
            environment: plan
                .environment
                .shown()
                .iter()
                .map(|(name, value)| (text(name), value.as_deref().map(text)))
                .collect(),
            environment_changes: EnvironmentChanges {
                mode: if changes.inherited {
                    "inherit"
                } else {
                    "clear"
                },
                kept: changes.kept,
                unset: changes.unset.iter().map(text).collect(),
                set: changes
                    .set
                    .iter()
                    .map(|(name, value)| (text(name), value.clone()))
                    .collect(),
                secrets: changes.secrets.iter().map(text).collect(),
            },
            command: plan.command.as_ref().map(|command| Command {
                given: text(&command.command),
                arguments: command.arguments.iter().map(text).collect(),
                path: text(&command.path),
            }),
            bwrap: text(&plan.bwrap),
            bwrap_arguments: plan
                .arguments
                .iter()
                .map(|argument| match argument {
                    Argument::Literal(value) => BwrapArgument::Literal { value: text(value) },
                    Argument::EmptyFile => BwrapArgument::EmptyFile,
                    Argument::Seccomp => BwrapArgument::SeccompFilter,
                    Argument::CopiedFile(content) => BwrapArgument::CopiedFile {
                        bytes: content.bytes().len(),
                    },
                })
                .collect(),
        }
    }
}

fn merged_policy(policy: &Policy) -> MergedPolicy {
    MergedPolicy {
        mounts: policy
            .mounts
            .iter()
            .map(|item| PolicyMount {
                directive: directive(item.directive),
                path: item.path.to_string(),
                origin: layer(&item.origin),
            })
            .collect(),
        scan: policy
            .scan
            .iter()
            .map(|scan| Scan {
                root: scan.root.to_string(),
                names: scan.names.clone(),
                exclude: scan.exclude.clone(),
                prune: scan.prune.clone(),
            })
            .collect(),
        hide_mounts: policy
            .hide_mounts
            .iter()
            .map(|hide_mounts| HideMounts {
                under: hide_mounts.under.to_string(),
                fstype: hide_mounts.fstype.clone(),
            })
            .collect(),
        network_mode: match policy.network_mode {
            NetworkMode::Host => "host",
            NetworkMode::None => "none",
        },
        env_mode: match policy.env_mode {
            EnvMode::Inherit => "inherit",
            EnvMode::Clear => "clear",
        },
        env_pass: policy.env_pass.clone(),
        env_set: policy.env_set.clone(),
        env_unset: policy.env_unset.clone(),
        path_prepend: policy
            .path_prepend
            .iter()
            .map(PolicyPath::to_string)
            .collect(),
        secrets: policy
            .secrets
            .iter()
            .map(|(name, path)| (name.clone(), path.to_string()))
            .collect(),
        instead_of: policy.instead_of.clone(),
    }
}

fn directive(directive: Directive) -> &'static str {
    match directive {
        Directive::Rw => "rw",
        Directive::RwFile => "rw-file",
        Directive::RwCopy => "rw-copy",
        Directive::Ro => "ro",
        Directive::Hide => "hide",
    }
}

fn source(source: &PolicySource) -> Source {
    match source {
        PolicySource::File(path) => Source::File { path: text(path) },
        PolicySource::BuiltInDefault => Source::BuiltInDefault,
    }
}

fn layer(origin: &LayerOrigin) -> Origin {
    match origin {
        LayerOrigin::Profile(path) => Origin::Profile { path: text(path) },
        LayerOrigin::BuiltInDefault => Origin::BuiltInDefault,
        LayerOrigin::PolicyFile(path) => Origin::PolicyFile { path: text(path) },
        LayerOrigin::CommandLine => Origin::CommandLine,
    }
}

fn item_origin(origin: &ItemOrigin) -> Origin {
    match origin {
        ItemOrigin::Written(origin) => layer(origin),
        ItemOrigin::Scan => Origin::Scan,
        ItemOrigin::HideMounts => Origin::HideMounts,
        ItemOrigin::SecretFile(name) => Origin::Secret { name: name.clone() },
        ItemOrigin::ConfigSecrets => Origin::ConfigSecrets,
    }
}

/// A path or an OS string as text, lossily. JSON escapes control characters itself.
fn text(value: impl AsRef<OsStr>) -> String {
    value.as_ref().to_string_lossy().into_owned()
}
