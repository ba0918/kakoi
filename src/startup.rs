//! The start-up, in the order of the checks of specification section 13: the `--help` and
//! `--version` forms, the grammar, the nesting, the current directory, and then stages 4
//! to 9 through `planning`. The first diagnostic ends the run. This is the outer layer:
//! it reads the arguments, the environment, and the current directory from the process
//! and hands them on as values.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::cli::{self, Invocation, Parsed};
use kakoi_core::diagnostic::{Diagnostic, Warning};
use kakoi_core::environment::{HostEnvironment, RealEntry};
use kakoi_core::plan::{is_nested, Plan};
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
        Parsed::Init(name) => {
            let env = HostEnvironment::from_variables(&host);
            let home =
                env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
            return Ok(Outcome::Init(InitRequest {
                config_dir: env.config_dir(&home),
                name,
            }));
        }
        Parsed::Invocation(invocation) => invocation,
    };
    // Stage 2, nested without `--print-plan`: stages 3 to 8 are skipped, nothing is read
    // and nothing changed, and the command is resolved on the host's `PATH`
    // (specification sections 12.1 and 13).
    if is_nested(&host) && invocation.print_plan.is_none() {
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
    let plan = plan_for(&Request {
        layers: invocation.layer_selection(),
        workspace: invocation.workspace.clone(),
        command: invocation.command.clone(),
        current_dir: current_dir.clone(),
        host,
    })?;
    Ok(Outcome::Prepared(Box::new(Prepared {
        invocation,
        current_dir,
        plan,
    })))
}

/// The one line a nested run prints (specification section 12.1).
fn nested_warning() -> Warning {
    Warning::new(
        "KAKOI=1: already inside an isolation, so the policy is not applied and the \
         command runs under the outer boundary",
    )
}
