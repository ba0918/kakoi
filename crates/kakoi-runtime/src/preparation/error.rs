//! How a preparation fails: the phase, the kind, what was observed, and whether what was
//! started for it is known to be gone.

use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Input,
    Worker,
    Planning,
    Retention,
    Launch,
    Execution,
    Cleanup,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    InvalidInput,
    UnsupportedEnvironment,
    PlanChanged,
    ResourceConflict,
    HelperFailure,
    Io,
    Protocol,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cleanup {
    NotNeeded,
    Confirmed,
    Unconfirmed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRecord {
    pub detail: String,
    pub os_error: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareError {
    pub phase: Phase,
    pub kind: ErrorKind,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub cleanup: Cleanup,
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}/{:?}", self.phase, self.kind)?;
        for diagnostic in &self.diagnostics {
            write!(f, ": {}", diagnostic.detail)?;
        }
        Ok(())
    }
}
impl std::error::Error for PrepareError {}

pub(super) fn error(phase: Phase, kind: ErrorKind, detail: impl Into<String>) -> PrepareError {
    PrepareError {
        phase,
        kind,
        diagnostics: vec![DiagnosticRecord {
            detail: detail.into(),
            os_error: None,
        }],
        cleanup: Cleanup::NotNeeded,
    }
}
pub(super) fn io_error(phase: Phase, cause: io::Error) -> PrepareError {
    let mut error = error(phase, ErrorKind::Io, cause.to_string());
    error.diagnostics[0].os_error = cause.raw_os_error();
    error
}
