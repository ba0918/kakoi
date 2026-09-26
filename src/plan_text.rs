//! The plan as `--print-plan` shows it (specification section 13), in its two text forms.
//! The summary, the default, is what a person reads: the real paths of the policy files read,
//! the four variables, the network mode, each mount item applied or skipped, and how the
//! environment differs from the host's. The full form (`--print-plan=full`) adds the
//! merged policy, the origin of every item, the whole environment with the secret values
//! masked, and the bwrap argument list with the descriptors as symbols. Both name the
//! resolved command. The layout is not a contract; the values embedded are escaped so
//! that no control character reaches the terminal. The JSON form is `plan_json`. Pure.

use std::ffi::OsStr;
use std::fmt::Write;
use std::path::Path;

use crate::cli::PlanForm;
use crate::plan_json;
use kakoi_core::diagnostic::escape_control;
use kakoi_core::layers::{Directive, LayerOrigin, Policy, PolicySource};
use kakoi_core::mounts::{ItemOrigin, SkippedRole};
use kakoi_core::plan::{Argument, Plan};
use kakoi_core::policy::{NetworkMode, PolicyPath};

/// The text of `plan` in `form`.
pub fn render(plan: &Plan, form: PlanForm) -> String {
    match form {
        PlanForm::Summary => text_form(plan, render_summary),
        PlanForm::Full => text_form(plan, render_full),
        PlanForm::Json => plan_json::render(plan),
    }
}

/// A text form: the head the two share (the nested mark on the first line, the policy
/// files, the variables), then `body`.
fn text_form(plan: &Plan, body: fn(&mut String, &Plan)) -> String {
    let mut text = String::new();
    if plan.nested {
        text.push_str("nested: yes (KAKOI=1; the plan is shown but would not be applied)\n");
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
    body(&mut text, plan);
    text
}

/// The summary: the network mode, the mount items with `~` for the home directory and the
/// origin only where it is not the global scope, the changes to the environment, and the
/// command. Ends with the line that names the other two forms.
fn render_summary(text: &mut String, plan: &Plan) {
    let _ = writeln!(text, "network: {}", plan.policy.network_mode.name());
    render_network_rules(text, &plan.policy);
    let _ = writeln!(text, "mounts (~ is {}):", shown(&plan.home));
    for item in &plan.mounts.items {
        let _ = writeln!(
            text,
            "  {:<7} {}{}",
            item.directive.name(),
            shortened(&item.real, &plan.home),
            item_origin_note(&item.origin)
        );
    }
    for item in &plan.mounts.skipped {
        let _ = writeln!(
            text,
            "  skipped {} `{}`{}: {}",
            item.directive.name(),
            escape_control(&item.written),
            layer_note(&item.origin),
            escape_control(&item.reason)
        );
    }
    render_left_visible_and_skipped_paths(text, plan);
    render_guards(text, plan);
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
        plan.policy.env_mode.name()
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
         environment, and the bwrap arguments; --print-plan=json is the same as one line \
         of JSON, for LLM agents and tools)\n",
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
            item.directive.name(),
            shown(&item.real),
            item_origin(&item.origin),
            escape_control(&item.written)
        );
    }
    for item in &plan.mounts.skipped {
        let _ = writeln!(
            text,
            "  skipped {} `{}` (from {}): {}",
            item.directive.name(),
            escape_control(&item.written),
            layer(&item.origin),
            escape_control(&item.reason)
        );
    }
    render_left_visible_and_skipped_paths(text, plan);
    render_guards(text, plan);
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

/// The scan hits left visible, the entries no `rw-copy` could take, and the scan roots,
/// `hide-mounts` `under`s, and `path-prepend` entries skipped, each with the reason, then
/// the note that says what an `rw-copy` item does. The same in both forms.
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
    for entry in &plan.not_copied {
        let _ = writeln!(
            text,
            "  not copied {} into the rw-copy item {}: {}",
            shown(&entry.path),
            shown(&entry.item),
            escape_control(&entry.reason)
        );
    }
    render_copy_note(text, plan);
}

/// The command guards placed, with the guard and, for `guard-absolute-path`, where the
/// real program is placed again; then the programs skipped, with the reason. The same in
/// both forms.
fn render_guards(text: &mut String, plan: &Plan) {
    if plan.guards.placed.is_empty() && plan.guards.skipped.is_empty() {
        return;
    }
    text.push_str("command guards:\n");
    for guard in &plan.guards.placed {
        let _ = write!(
            text,
            "  {} guarded by {} (real {})",
            escape_control(&guard.program),
            shown(guard.guard()),
            shown(&guard.found)
        );
        if let Some(relocated) = &guard.relocated {
            let _ = write!(text, ", relocated to {}", shown(relocated));
        }
        text.push('\n');
    }
    for skipped in &plan.guards.skipped {
        let _ = writeln!(
            text,
            "  skipped `{}`: {}",
            escape_control(&skipped.program),
            escape_control(&skipped.reason)
        );
    }
}

/// The one line that says what `rw-copy` means, printed only when an item uses it: the
/// directive is the one whose name does not say on its own where the writing goes.
fn render_copy_note(text: &mut String, plan: &Plan) {
    let uses_copy = plan
        .mounts
        .items
        .iter()
        .any(|item| item.directive == Directive::RwCopy);
    if uses_copy {
        text.push_str(
            "  (rw-copy starts from a copy of the host\'s content and is writable inside; \
             nothing written there reaches the host, and it is gone when the command ends)\n",
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

fn render_network_rules(text: &mut String, policy: &Policy) {
    if policy.network_settings_present {
        for rule in &policy.network_allow {
            let _ = writeln!(
                text,
                "  allow {} {} ports {}",
                rule.protocol,
                escape_control(&rule.destination.to_string()),
                rule.ports
            );
        }
        for publication in &policy.network_publish {
            let _ = writeln!(
                text,
                "  planned publication: {} {} host:{} -> sandbox:{}",
                publication.protocol, publication.family, publication.host_port, publication.port
            );
        }
        for upstream in &policy.dns_upstream {
            let _ = writeln!(
                text,
                "  DNS upstream: {} port {}{}",
                upstream.address,
                upstream.port(),
                upstream
                    .tls_name()
                    .map(|name| format!(" TLS name={name}"))
                    .unwrap_or_else(|| " plain".into())
            );
        }
    }
}

fn render_policy(text: &mut String, policy: &Policy) {
    text.push_str("policy (merged):\n");
    for item in &policy.mounts {
        let _ = writeln!(
            text,
            "  mounts.{} `{}` (from {})",
            item.directive.name(),
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
    let _ = writeln!(text, "  network.mode = {}", policy.network_mode.name());
    render_network_rules(text, policy);
    if policy.network_settings_present || policy.network_mode == NetworkMode::Filtered {
        let limits = &policy.network_limits;
        for (name, value) in [
            ("udp-idle-timeout-seconds", limits.udp_idle_timeout_seconds),
            (
                "dns-zero-ttl-grace-milliseconds",
                limits.dns_zero_ttl_grace_milliseconds,
            ),
            (
                "dns-server-timeout-seconds",
                limits.dns_server_timeout_seconds,
            ),
            (
                "dns-resolution-timeout-seconds",
                limits.dns_resolution_timeout_seconds,
            ),
            ("dns-max-cname-hops", limits.dns_max_cname_hops),
            ("dns-max-upstream-queries", limits.dns_max_upstream_queries),
            (
                "dns-max-concurrent-resolutions",
                limits.dns_max_concurrent_resolutions,
            ),
            (
                "dns-max-waiters-per-resolution",
                limits.dns_max_waiters_per_resolution,
            ),
            (
                "dns-failure-cache-seconds",
                limits.dns_failure_cache_seconds,
            ),
            (
                "recovery-attempt-timeout-seconds",
                limits.recovery_attempt_timeout_seconds,
            ),
        ] {
            let _ = writeln!(text, "  network.{name} = {value}");
        }
        let _ = writeln!(
            text,
            "  process.shutdown-grace-seconds = {}",
            policy.shutdown_grace_seconds
        );
    }
    let _ = writeln!(text, "  env.mode = {}", policy.env_mode.name());
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
    for entry in &policy.guards {
        let rule = serde_json::to_string(&entry.rule).expect("a rule serializes");
        let _ = writeln!(
            text,
            "  commands.guard {} (from {})",
            escape_control(&rule),
            layer(&entry.origin)
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

/// Where a policy came from. The built-in default names `kakoi init`, the form that
/// writes it out, which is the contract of specification section 13.
fn policy_source(source: &PolicySource) -> String {
    match source {
        PolicySource::File(path) => shown(path),
        PolicySource::BuiltInDefault => {
            "the built-in default (write it out with `kakoi init`)".to_string()
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
        Argument::CopiedFile(content) => {
            format!("<fd: copied file, {} bytes>", content.bytes().len())
        }
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
