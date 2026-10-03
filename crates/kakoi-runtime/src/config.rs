//! Explicit configuration loading. Memory requests never enter this module.

use std::path::PathBuf;

use crate::environment::{HostEnvironment, RealEntry};
use crate::input::HostContext;
use crate::workspace_facts::real_entry;
use crate::Policy;

pub use kakoi_policy::layers::{LayerSelection as Selection, PolicySource as Source};
pub type ConfigError = kakoi_policy::PolicyError;

#[derive(Debug, Clone)]
pub struct LoadedPolicy {
    pub policy: Policy,
    pub sources: Vec<Source>,
}

pub fn load(selection: &Selection, context: &HostContext) -> Result<LoadedPolicy, ConfigError> {
    let environment = HostEnvironment::from_variables(context.environment());
    let home = environment.home_directory(
        &environment
            .home
            .as_deref()
            .map_or(RealEntry::Missing, real_entry),
    )?;
    let config_dir = environment.config_dir(&home);
    let anchor = |path: &PathBuf| {
        if path.is_absolute() {
            path.clone()
        } else {
            context.cwd().join(path)
        }
    };
    let selection = Selection {
        profile: selection.profile.clone(),
        policy_file: selection.policy_file.as_ref().map(anchor),
        rw: selection.rw.iter().map(anchor).collect(),
        hide: selection.hide.iter().map(anchor).collect(),
    };
    let layers = crate::layers::load_layers(&selection, &config_dir)?;
    let sources = layers
        .iter()
        .filter_map(|layer| match &layer.origin {
            kakoi_policy::layers::LayerOrigin::Profile(path)
            | kakoi_policy::layers::LayerOrigin::PolicyFile(path) => {
                Some(Source::File(path.clone()))
            }
            kakoi_policy::layers::LayerOrigin::BuiltInDefault => Some(Source::BuiltInDefault),
            kakoi_policy::layers::LayerOrigin::CommandLine => None,
        })
        .collect();
    Ok(LoadedPolicy {
        policy: Policy::from_layers(&layers)?,
        sources,
    })
}
