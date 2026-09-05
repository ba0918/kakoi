//! The plan as `--print-plan` shows it (specification section 13): the merged policy, the
//! real paths of the policy files read, the four variables, each mount item applied or
//! skipped with the reason, each scan hit left visible with the reason, each scan root,
//! `hide-mounts` `under`, and `path-prepend` entry skipped with the reason, the final
//! environment with the secret values masked, the
//! resolved command, and the bwrap argument list with the descriptors as symbols. The
//! layout is not a contract; the values embedded are escaped so that no control character
//! reaches the terminal. Pure.

use std::ffi::OsStr;
use std::fmt::Write;

use crate::diagnostic::escape_control;
use crate::layers::{Directive, LayerOrigin, Policy};
use crate::mounts::{ItemOrigin, SkippedRole};
use crate::plan::{Argument, Plan};
use crate::policy::{EnvMode, NetworkMode, PolicyPath};

/// The text of `plan`. A nested run is marked on the first line.
pub fn render(plan: &Plan) -> String {
    let mut text = String::new();
    if plan.nested {
        text.push_str("nested: yes (PROCESS_WRAP=1; the plan is shown but would not be applied)\n");
    }
    text.push_str("policy files:\n");
    for file in &plan.policy_files {
        let _ = writeln!(text, "  {}", shown(file));
    }
    text.push_str("variables:\n");
    let _ = writeln!(text, "  workspace = {}", shown(&plan.variables.workspace));
    let _ = writeln!(text, "  worktree = {}", shown(&plan.variables.worktree));
    let _ = writeln!(
        text,
        "  git_common_dir = {}",
        plan.variables
            .git_common_dir
            .as_deref()
            .map_or_else(|| "(no value)".to_string(), shown)
    );
    let _ = writeln!(text, "  config_dir = {}", shown(&plan.variables.config_dir));
    render_policy(&mut text, &plan.policy);
    text.push_str("mounts:\n");
    for item in &plan.mounts.items {
        let _ = writeln!(
            text,
            "  {:<7} {} (from {}: `{}`)",
            directive(item.directive),
            shown(&item.real),
            item_origin(&item.origin),
            escape_control(&item.written)
        );
    }
    for item in &plan.mounts.skipped {
        let _ = writeln!(
            text,
            "  skipped {} `{}` (from {}): {}",
            directive(item.directive),
            escape_control(&item.written),
            layer(&item.origin),
            escape_control(&item.reason)
        );
    }
    for left in &plan.mounts.left_visible {
        let _ = writeln!(
            text,
            "  not hidden {} (from the scan): {}",
            shown(&left.link),
            escape_control(&left.reason)
        );
    }
    for skipped in &plan.skipped_paths {
        let role = match skipped.role {
            SkippedRole::ScanRoot => "mounts.scan root",
            SkippedRole::HideMountsUnder => "mounts.hide-mounts under",
            SkippedRole::PathPrepend => "env.path-prepend entry",
        };
        let _ = writeln!(
            text,
            "  skipped {role} `{}`: {}",
            escape_control(&skipped.written),
            escape_control(&skipped.reason)
        );
    }
    text.push_str("environment:\n");
    for (name, value) in plan.environment.shown() {
        match value {
            Some(value) => {
                let _ = writeln!(text, "  {}={}", shown(&name), shown(&value));
            }
            None => {
                let _ = writeln!(text, "  {}=<secret, not shown>", shown(&name));
            }
        }
    }
    let _ = writeln!(
        text,
        "command: {}",
        plan.command
            .as_deref()
            .map_or_else(|| "(none)".to_string(), shown)
    );
    let _ = writeln!(text, "bwrap: {}", shown(&plan.bwrap));
    text.push_str("bwrap arguments:\n");
    for argument in &plan.arguments {
        let _ = writeln!(text, "  {}", argument_text(argument));
    }
    text
}

fn render_policy(text: &mut String, policy: &Policy) {
    text.push_str("policy (merged):\n");
    for item in &policy.mounts {
        let _ = writeln!(
            text,
            "  mounts.{} `{}` (from {})",
            directive(item.directive),
            escape_control(&item.path.to_string()),
            layer(&item.origin)
        );
    }
    for scan in &policy.scan {
        let _ = writeln!(
            text,
            "  mounts.scan root=`{}` names={} exclude={} prune={}",
            escape_control(&scan.root.to_string()),
            list(&scan.names),
            list(&scan.exclude),
            list(&scan.prune)
        );
    }
    for hide_mounts in &policy.hide_mounts {
        let _ = writeln!(
            text,
            "  mounts.hide-mounts under=`{}` fstype={}",
            escape_control(&hide_mounts.under.to_string()),
            list(&hide_mounts.fstype)
        );
    }
    let network = match policy.network_mode {
        NetworkMode::Host => "host",
        NetworkMode::None => "none",
    };
    let _ = writeln!(text, "  network.mode = {network}");
    let env_mode = match policy.env_mode {
        EnvMode::Inherit => "inherit",
        EnvMode::Clear => "clear",
    };
    let _ = writeln!(text, "  env.mode = {env_mode}");
    let _ = writeln!(text, "  env.pass = {}", list(&policy.env_pass));
    for (name, value) in &policy.env_set {
        let _ = writeln!(
            text,
            "  env.set {}={}",
            escape_control(name),
            escape_control(value)
        );
    }
    let _ = writeln!(text, "  env.unset = {}", list(&policy.env_unset));
    let _ = writeln!(
        text,
        "  env.path-prepend = {}",
        list(
            &policy
                .path_prepend
                .iter()
                .map(PolicyPath::to_string)
                .collect::<Vec<_>>()
        )
    );
    for (name, path) in &policy.secrets {
        let _ = writeln!(
            text,
            "  secrets {} = `{}`",
            escape_control(name),
            escape_control(&path.to_string())
        );
    }
    for (original, replacement) in &policy.instead_of {
        let _ = writeln!(
            text,
            "  git.instead-of `{}` = `{}`",
            escape_control(original),
            escape_control(replacement)
        );
    }
}

fn directive(directive: Directive) -> &'static str {
    match directive {
        Directive::Rw => "rw",
        Directive::RwFile => "rw-file",
        Directive::Ro => "ro",
        Directive::Hide => "hide",
    }
}

fn layer(origin: &LayerOrigin) -> String {
    match origin {
        LayerOrigin::Profile(path) => format!("the profile {}", shown(path)),
        LayerOrigin::PolicyFile(path) => format!("the policy file {}", shown(path)),
        LayerOrigin::CommandLine => "the command line".to_string(),
    }
}

fn item_origin(origin: &ItemOrigin) -> String {
    match origin {
        ItemOrigin::Written(origin) => layer(origin),
        ItemOrigin::Scan => "the scan".to_string(),
        ItemOrigin::HideMounts => "hide-mounts".to_string(),
        ItemOrigin::SecretFile(name) => format!("the secret `{}`", escape_control(name)),
        ItemOrigin::ConfigSecrets => "the configuration directory's secrets/".to_string(),
    }
}

fn argument_text(argument: &Argument) -> String {
    match argument {
        Argument::Literal(text) => shown(text),
        Argument::EmptyFile => "<fd: empty file>".to_string(),
        Argument::Seccomp => "<fd: seccomp filter>".to_string(),
    }
}

fn list(items: &[String]) -> String {
    let quoted: Vec<String> = items
        .iter()
        .map(|item| format!("`{}`", escape_control(item)))
        .collect();
    format!("[{}]", quoted.join(", "))
}

/// A path or an OS string as it may be shown: lossily as text, control characters
/// escaped.
fn shown(value: impl AsRef<OsStr>) -> String {
    escape_control(&value.as_ref().to_string_lossy())
}
