//! A diagnostic: one line on standard error, `process-wrap: <kind>: <description>`, and the
//! exit code that goes with it (specification section 13).

use std::fmt;

/// The kind of a diagnostic. The kind is the contract; the description is free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Usage,
    Policy,
    Path,
    Secret,
    Env,
    Bwrap,
    CommandNotFound,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Usage => "usage",
            Kind::Policy => "policy",
            Kind::Path => "path",
            Kind::Secret => "secret",
            Kind::Env => "env",
            Kind::Bwrap => "bwrap",
            Kind::CommandNotFound => "command not found",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    kind: Kind,
    description: String,
}

impl Diagnostic {
    pub fn new(kind: Kind, description: impl Into<String>) -> Self {
        Self {
            kind,
            description: description.into(),
        }
    }

    pub fn usage(description: impl Into<String>) -> Self {
        Self::new(Kind::Usage, description)
    }

    pub fn policy(description: impl Into<String>) -> Self {
        Self::new(Kind::Policy, description)
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// 127 for `command not found`, 125 for every other kind.
    pub fn exit_code(&self) -> i32 {
        match self.kind {
            Kind::CommandNotFound => 127,
            _ => 125,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "process-wrap: {}: {}",
            self.kind.as_str(),
            self.description
        )
    }
}
