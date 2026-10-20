//! What a prepared run tells its caller before it starts: the plan's modes, the command,
//! the mounts, and what was skipped, carried from the worker as plain data.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::error::{error, DiagnosticRecord, ErrorKind, Phase, PrepareError};
use super::{bytes, path};
use crate::{ListMode, NetworkMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountDescription {
    pub path: PathBuf,
    pub permission: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedDescription {
    pub written: String,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDescription {
    pub mounts_mode: ListMode,
    pub network_mode: NetworkMode,
    pub environment_mode: crate::EnvMode,
    pub commands_mode: ListMode,
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub cwd: PathBuf,
    pub mounts: Vec<MountDescription>,
    pub skipped: Vec<SkippedDescription>,
    pub guard_count: usize,
    pub required_features: Vec<String>,
    pub diagnostics: Vec<DiagnosticRecord>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DescriptionInput {
    mounts_mode: String,
    network_mode: String,
    environment_mode: String,
    commands_mode: String,
    program: Vec<u8>,
    arguments: Vec<Vec<u8>>,
    cwd: Vec<u8>,
    mounts: Vec<(Vec<u8>, String)>,
    skipped: Vec<(String, String)>,
    guard_count: usize,
    required_features: Vec<String>,
    diagnostics: Vec<DiagnosticRecord>,
}

fn parse_mode<T: serde::de::DeserializeOwned>(name: String) -> Result<T, PrepareError> {
    serde_json::from_value(serde_json::Value::String(name)).map_err(|_| {
        error(
            Phase::Worker,
            ErrorKind::Protocol,
            "invalid description mode",
        )
    })
}
impl DescriptionInput {
    pub(super) fn from_plan(plan: &crate::plan::Plan, cwd: &std::path::Path) -> Self {
        let command = plan
            .command
            .as_ref()
            .expect("a library request contains a command");
        let program = if command.path.is_absolute() {
            command.path.clone()
        } else {
            cwd.join(&command.path)
        };
        Self {
            mounts_mode: plan.policy.mounts_mode.name().into(),
            network_mode: plan.policy.network_mode.name().into(),
            environment_mode: plan.policy.env_mode.name().into(),
            commands_mode: plan.policy.commands_mode.name().into(),
            program: bytes(program.as_os_str()),
            arguments: command.arguments.iter().map(|value| bytes(value)).collect(),
            cwd: bytes(cwd.as_os_str()),
            mounts: plan
                .mounts
                .items
                .iter()
                .map(|item| (bytes(item.real.as_os_str()), item.directive.name().into()))
                .collect(),
            skipped: plan
                .skipped_paths
                .iter()
                .map(|item| (item.written.clone(), format!("{:?}", item.reason)))
                .collect(),
            guard_count: plan.guards.placed.len(),
            required_features: vec!["bwrap bind-fd".into(), "bwrap ro-bind-fd".into()],
            diagnostics: plan
                .warnings
                .iter()
                .map(|warning| DiagnosticRecord {
                    detail: warning.to_string(),
                    os_error: None,
                })
                .collect(),
        }
    }
    pub(super) fn into_public(self) -> Result<PlanDescription, PrepareError> {
        Ok(PlanDescription {
            mounts_mode: parse_mode(self.mounts_mode)?,
            network_mode: parse_mode(self.network_mode)?,
            environment_mode: parse_mode(self.environment_mode)?,
            commands_mode: parse_mode(self.commands_mode)?,
            program: path(self.program),
            arguments: self.arguments.into_iter().map(OsString::from_vec).collect(),
            cwd: path(self.cwd),
            mounts: self
                .mounts
                .into_iter()
                .map(|(bytes, permission)| MountDescription {
                    path: path(bytes),
                    permission,
                })
                .collect(),
            skipped: self
                .skipped
                .into_iter()
                .map(|(written, reason)| SkippedDescription { written, reason })
                .collect(),
            guard_count: self.guard_count,
            required_features: self.required_features,
            diagnostics: self.diagnostics,
        })
    }
}
