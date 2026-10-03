//! Explicit file loading for the policy layers.

use std::path::Path;

pub use kakoi_policy::layers::*;

use crate::diagnostic::Diagnostic;
use crate::environment::PathState;
use crate::policy::{parse_policy, PolicyFile};
use crate::regular_file::{read_regular_file, Links};
use crate::workspace_facts::probe_path;

/// Reads the profile and the `--policy-file` named by `selection` and returns the written
/// layers, lowest first. Reads nothing else.
pub fn load_layers(
    selection: &LayerSelection,
    config_dir: &Path,
) -> Result<Vec<Layer>, Diagnostic> {
    let profile_path = config_dir
        .join("profile")
        .join(format!("{}.toml", selection.profile));
    let mut layers = vec![global_scope(selection, &profile_path)?];
    if let Some(path) = &selection.policy_file {
        layers.push(Layer {
            origin: LayerOrigin::PolicyFile(path.clone()),
            policy: read_policy_file(path, || "policy file".to_string())?,
        });
    }
    layers.push(Layer::command_line(selection));
    Ok(layers)
}

// A broken link or a regular file in the way must not select the wider default.
fn global_scope(selection: &LayerSelection, path: &Path) -> Result<Layer, Diagnostic> {
    if selection.profile == DEFAULT_PROFILE && probe_path(path) == PathState::Absent {
        return Ok(Layer {
            origin: LayerOrigin::BuiltInDefault,
            policy: parse_policy(BUILT_IN_DEFAULT, path)?,
        });
    }
    Ok(Layer {
        origin: LayerOrigin::Profile(path.to_path_buf()),
        policy: read_policy_file(path, || format!("profile `{}`", selection.profile))?,
    })
}

fn read_policy_file(path: &Path, role: impl FnOnce() -> String) -> Result<PolicyFile, Diagnostic> {
    let text = read_regular_file(path, Links::Follow)
        .map_err(|error| error.to_string())
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "is not valid UTF-8".to_string()))
        .map_err(|reason| {
            Diagnostic::policy(format!("{} at {} {reason}", role(), path.display()))
        })?;
    parse_policy(&text, path)
}
