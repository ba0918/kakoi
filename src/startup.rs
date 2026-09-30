//! The start-up, in the order of the checks of specification section 13: the `--help` and
//! `--version` forms, the grammar, the nesting, the current directory, and then stages 4
//! to 9 through `planning`. The first diagnostic ends the run. This is the outer layer:
//! it reads the arguments, the environment, and the current directory from the process
//! and hands them on as values.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::cli::{self, Example, Invocation, Nesting, Parsed};
use kakoi_core::diagnostic::{Diagnostic, Warning};
use kakoi_core::environment::{HostEnvironment, RealEntry};
use kakoi_core::guard_placement::{GUARD_ROOT, GUARD_TABLE};
use kakoi_core::plan::{Plan, NESTING_MARK};
use kakoi_core::planning::{locate_command, plan_for, Request};
use kakoi_core::workspace_facts::real_entry;

/// What the start-up ends with: text to print (the usage or the version), a nested run
/// that goes straight to the command, or everything the start needs.
#[derive(Debug)]
pub enum Outcome {
    Text(String),
    Init(InitRequest),
    Nested(Nested),
    Prepared(Box<Prepared>),
}

/// The `init` form (specification section 4.1): where to write and under what name. It is
/// the one form that writes, and the only check before it is the home directory.
#[derive(Debug)]
pub struct InitRequest {
    pub config_dir: PathBuf,
    pub name: String,
    /// The example written out.
    pub example: Example,
}

/// A nested run (specification section 12.1): the warning to print first, then the
/// command resolved on the host's `PATH` or the `command not found` diagnostic, the
/// `COMMAND` string as given (the process's argv[0]), and the arguments as given. The
/// environment is left as it is.
#[derive(Debug)]
pub struct Nested {
    pub warning: Warning,
    pub command: Result<PathBuf, Diagnostic>,
    pub given: OsString,
    pub arguments: Vec<OsString>,
}

/// The results of stages 3 to 9.
#[derive(Debug)]
pub struct Prepared {
    pub invocation: Invocation,
    pub current_dir: PathBuf,
    pub plan: Plan,
}

/// Runs stages 1 to 9 on `arguments` (without the program name).
pub fn prepare<I>(arguments: I) -> Result<Outcome, Diagnostic>
where
    I: IntoIterator<Item = OsString>,
{
    // Stages 1 and 2 need nothing from the host.
    let parsed = cli::interpret(arguments)?;
    let host: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    let invocation = match parsed {
        Parsed::Help(text) | Parsed::Version(text) => return Ok(Outcome::Text(text)),
        // The `init` form looks at neither nesting nor the current directory nor `bwrap`:
        // the home directory is the only check it passes (specification section 13,
        // stage 2), because a machine without `bwrap`, and a shell inside an isolation,
        // can still put the configuration in place.
        Parsed::Init(name, example) => {
            let env = HostEnvironment::from_variables(&host);
            let home =
                env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
            return Ok(Outcome::Init(InitRequest {
                config_dir: env.config_dir(&home),
                name,
                example,
            }));
        }
        Parsed::Invocation(invocation) => invocation,
    };
    // Stage 2, nested with `--nested=exec` and without `--print-plan`: stages 3 to 8 are
    // skipped, nothing is read and nothing changed, and the command is resolved on the
    // host's `PATH` (specification REQ-284). With `--nested=isolate` a nested run goes
    // through every stage as any other run does (REQ-456).
    let nested = inside_an_isolation();
    let applied = !nested || invocation.nested == Nesting::Isolate;
    if !applied && invocation.print_plan.is_none() {
        return Ok(Outcome::Nested(Nested {
            warning: nested_warning(),
            command: locate_command(&invocation.command[0], &host),
            given: invocation.command[0].clone(),
            arguments: invocation.command[1..].to_vec(),
        }));
    }
    // Stage 3.
    let current_dir = std::env::current_dir().map_err(|error| {
        Diagnostic::path(format!(
            "the current directory cannot be determined: {error}"
        ))
    })?;
    let invocation = invocation.anchored(&current_dir);
    // Only kakoi makes `/dev`, so what is there was placed by the run around this one
    // (specification REQ-465).
    let outer_guard = nested && applied && Path::new(GUARD_ROOT).symlink_metadata().is_ok();
    let plan = plan_for(&Request {
        layers: invocation.layer_selection(),
        workspace: invocation.workspace.clone(),
        command: invocation.command.clone(),
        current_dir: current_dir.clone(),
        host,
        executable: std::env::current_exe().ok(),
        nested,
        applied,
        outer_guard,
        outer_table: outer_guard
            .then(|| std::fs::read(GUARD_TABLE).ok())
            .flatten(),
        landlock_abi: kakoi_core::landlock::abi_version(),
    })?;
    Ok(Outcome::Prepared(Box::new(Prepared {
        invocation,
        current_dir,
        plan,
    })))
}

/// Whether the nesting mark is there: only an isolation of kakoi puts it, read-only, and
/// the environment, which the isolation's processes can change, is not asked
/// (specification REQ-455).
fn inside_an_isolation() -> bool {
    Path::new(NESTING_MARK).symlink_metadata().is_ok()
}

/// The one line a nested run prints (specification section 12.1).
fn nested_warning() -> Warning {
    Warning::new(
        "already inside an isolation, so the policy is not applied and the \
         command runs under the outer boundary",
    )
}
