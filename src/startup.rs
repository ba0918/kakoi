//! The start-up, in the order of the checks of specification section 13: the `--help` and
//! `--version` forms, the grammar, the current directory, the home directory, the policy
//! files and their merge, then the workspace and the variables. The first diagnostic ends
//! the run. This is the outer layer: it reads the environment and the file system and
//! hands the facts to the pure functions.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::cli::{self, Invocation, Parsed};
use crate::diagnostic::Diagnostic;
use crate::environment::{HomeDirectory, HostEnvironment, RealEntry};
use crate::layers::{load_layers, merge, Layer, Policy};
use crate::variables::{derive_variables, Variables};
use crate::workspace_facts::{collect_workspace_facts, real_entry};

/// What the start-up ends with: text to print (the usage or the version), or everything
/// the later stages need.
#[derive(Debug)]
pub enum Outcome {
    Text(String),
    Prepared(Box<Prepared>),
}

/// The results of stages 3 to 6.
#[derive(Debug)]
pub struct Prepared {
    pub invocation: Invocation,
    pub current_dir: PathBuf,
    pub home: HomeDirectory,
    pub config_dir: PathBuf,
    pub layers: Vec<Layer>,
    pub policy: Policy,
    pub variables: Variables,
}

/// Runs stages 1 to 6 on `arguments` (without the program name).
pub fn prepare<I>(arguments: I) -> Result<Outcome, Diagnostic>
where
    I: IntoIterator<Item = OsString>,
{
    // Stages 1 and 2 need nothing from the host.
    let invocation = match cli::interpret(arguments)? {
        Parsed::Help(text) | Parsed::Version(text) => return Ok(Outcome::Text(text)),
        Parsed::Invocation(invocation) => invocation,
    };
    // The nested branch of specification section 12.1 goes here, before stage 3.
    let current_dir = std::env::current_dir().map_err(|error| {
        Diagnostic::path(format!(
            "the current directory cannot be determined: {error}"
        ))
    })?;
    let invocation = invocation.anchored(&current_dir);
    let env = HostEnvironment::from_process();
    let home = env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
    let config_dir = env.config_dir(&home);
    let layers = load_layers(&invocation, &config_dir)?;
    let policy = merge(&layers)?;
    let workspace = invocation
        .workspace
        .clone()
        .unwrap_or_else(|| current_dir.clone());
    let facts = collect_workspace_facts(&workspace);
    let variables = derive_variables(&real_entry(&config_dir), &facts)?;
    Ok(Outcome::Prepared(Box::new(Prepared {
        invocation,
        current_dir,
        home,
        config_dir,
        layers,
        policy,
        variables,
    })))
}
