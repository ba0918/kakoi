//! The plan as `--print-plan` shows it (specification section 13), in two forms. The
//! summary, the default, is what a person reads: the real paths of the policy files read,
//! the four variables, the network mode, each mount item applied or skipped, and how the
//! environment differs from the host's. The full form (`--print-plan=full`) adds the
//! merged policy, the origin of every item, the whole environment with the secret values
//! masked, and the bwrap argument list with the descriptors as symbols. Both name the
//! resolved command. The layout is not a contract; the values embedded are escaped so
//! that no control character reaches the terminal. Pure.

use std::ffi::OsStr;
use std::fmt::Write;
use std::path::Path;

use crate::cli::PlanForm;
use crate::diagnostic::escape_control;
use crate::layers::{Directive, LayerOrigin, Policy, PolicySource};
use crate::mounts::{ItemOrigin, SkippedRole};
use crate::plan::{Argument, Plan};
use crate::policy::{EnvMode, NetworkMode, PolicyPath};

/// The text of `plan` in `form`. A nested run is marked on the first line.
pub fn render(plan: &Plan, form: PlanForm) -> String {
    let mut text = String::new();
    if plan.nested {
        text.push_str("nested: yes (PROCESS_WRAP=1; the plan is shown but would not be applied)\n");
    }
    text.push_str("policy files:\n");
    for source in &plan.policy_sources {
        let _ = writeln!(text, "  {}", policy_source(source));
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
    let _ = writeln!(
        text,
        "  config_dir = {}",
        plan.variables
            .config_dir
            .as_deref()
            .map_or_else(|| "(no value)".to_string(), shown)
    );
    match form {
        PlanForm::Summary => render_summary(&mut text, plan),
        PlanForm::Full => render_full(&mut text, plan),
    }
    text
}

/// The summary: the network mode, the mount items with `~` for the home directory and the
/// origin only where it is not the global scope, the changes to the environment, and the
/// command. Ends with the line that names the full form.
fn render_summary(text: &mut String, plan: &Plan) {
    let _ = writeln!(text, "network: {}", network_mode(plan.policy.network_mode));
    let _ = writeln!(text, "mounts (~ is {}):", shown(&plan.home));
    for item in &plan.mounts.items {
        let _ = writeln!(
            text,
            "  {:<7} {}{}",
            directive(item.directive),
            shortened(&item.real, &plan.home),
            item_origin_note(&item.origin)
        );
    }
    for item in &plan.mounts.skipped {
        let _ = writeln!(
            text,
            "  skipped {} `{}`{}: {}",
            directive(item.directive),
            escape_control(&item.written),
            layer_note(&item.origin),
            escape_control(&item.reason)
        );
    }
    render_left_visible_and_skipped_paths(text, plan);
    let changes = &plan.environment_changes;
    let kept = match (changes.inherited, changes.kept) {
        (true, 1) => "1 variable as on the host".to_string(),
        (true, count) => format!("{count} variables as on the host"),
        (false, 1) => "1 variable passed from the host".to_string(),
        (false, count) => format!("{count} variables passed from the host"),
    };
    let _ = writeln!(
        text,
        "environment ({}): {kept}, and:",
        env_mode(plan.policy.env_mode)
    );
    for name in &changes.unset {
        let _ = writeln!(text, "  unset  {}", shown(name));
    }
    for (name, value) in &changes.set {
        let _ = writeln!(text, "  set    {}={}", shown(name), escape_control(value));
    }
    for name in &changes.secrets {
        let _ = writeln!(text, "  secret {} (value not shown)", shown(name));
    }
    render_command(text, plan);
    text.push_str(
        "(--print-plan=full adds the merged policy, the origin of every item, the whole \
         environment, and the bwrap arguments)\n",
    );
}

/// The full form: the merged policy, every mount item with its real path and its origin,
/// the whole environment, the command, and the bwrap argument list.
fn render_full(text: &mut String, plan: &Plan) {
    render_policy(text, &plan.policy);
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
    render_left_visible_and_skipped_paths(text, plan);
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
    render_command(text, plan);
    text.push_str("bwrap arguments:\n");
    for argument in &plan.arguments {
        let _ = writeln!(text, "  {}", argument_text(argument));
    }
}

/// The scan hits left visible and the scan roots, `hide-mounts` `under`s, and
/// `path-prepend` entries skipped, each with the reason. The same in both forms.
fn render_left_visible_and_skipped_paths(text: &mut String, plan: &Plan) {
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
}

fn render_command(text: &mut String, plan: &Plan) {
    let _ = writeln!(
        text,
        "command: {}",
        plan.command
            .as_ref()
            .map_or_else(|| "(none)".to_string(), |command| shown(&command.path))
    );
    let _ = writeln!(text, "bwrap: {}", shown(&plan.bwrap));
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
    let _ = writeln!(
        text,
        "  network.mode = {}",
        network_mode(policy.network_mode)
    );
    let _ = writeln!(text, "  env.mode = {}", env_mode(policy.env_mode));
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

fn network_mode(mode: NetworkMode) -> &'static str {
    match mode {
        NetworkMode::Host => "host",
        NetworkMode::None => "none",
    }
}

fn env_mode(mode: EnvMode) -> &'static str {
    match mode {
        EnvMode::Inherit => "inherit",
        EnvMode::Clear => "clear",
    }
}

/// Where a policy came from. The built-in default names `process-wrap init`, the form that
/// writes it out, which is the contract of specification section 13.
fn policy_source(source: &PolicySource) -> String {
    match source {
        PolicySource::File(path) => shown(path),
        PolicySource::BuiltInDefault => {
            "the built-in default (write it out with `process-wrap init`)".to_string()
        }
    }
}

fn layer(origin: &LayerOrigin) -> String {
    match origin {
        LayerOrigin::Profile(path) => format!("the profile {}", shown(path)),
        LayerOrigin::BuiltInDefault => "the built-in default".to_string(),
        LayerOrigin::PolicyFile(path) => format!("the policy file {}", shown(path)),
        LayerOrigin::CommandLine => "the command line".to_string(),
    }
}

/// The summary's note on a written item's layer: nothing for the global scope, which the
/// `policy files` lines name; a short label for the other two layers.
fn layer_note(origin: &LayerOrigin) -> &'static str {
    match origin {
        LayerOrigin::Profile(_) | LayerOrigin::BuiltInDefault => "",
        LayerOrigin::PolicyFile(_) => " (--policy-file)",
        LayerOrigin::CommandLine => " (command line)",
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

/// The summary's note on an applied item's origin: nothing for the global scope, a short
/// label otherwise.
fn item_origin_note(origin: &ItemOrigin) -> String {
    match origin {
        ItemOrigin::Written(origin) => layer_note(origin).to_string(),
        ItemOrigin::Scan => " (scan)".to_string(),
        ItemOrigin::HideMounts => " (hide-mounts)".to_string(),
        ItemOrigin::SecretFile(name) => format!(" (secret {})", escape_control(name)),
        ItemOrigin::ConfigSecrets => " (secrets/ of the configuration directory)".to_string(),
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

/// `path` with the home directory replaced by `~`: the home itself, or a path below it.
/// A path that only shares a prefix of the name (`/home/user2` for `/home/user`) is left
/// whole.
fn shortened(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", shown(rest)),
        Err(_) => shown(path),
    }
}

/// A path or an OS string as it may be shown: lossily as text, control characters
/// escaped.
fn shown(value: impl AsRef<OsStr>) -> String {
    escape_control(&value.as_ref().to_string_lossy())
}
