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

/// Writes the control characters of `text` (0x00 to 0x1F and 0x7F) in a visible form: `\n`,
/// `\r`, and `\t` for the usual three, `\xNN` for the rest. A value taken from the command
/// line or a policy file then cannot split a line or reach the terminal as a control
/// sequence (specification section 13). Unicode line separators are left as they are.
pub fn escape_control(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\x00'..='\x1f' | '\x7f' => {
                escaped.push_str(&format!("\\x{:02X}", character as u32));
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

impl Diagnostic {
    /// Control characters in `description` are escaped with `escape_control`, so the
    /// diagnostic stays one line without them whatever value it embeds.
    pub fn new(kind: Kind, description: impl Into<String>) -> Self {
        Self {
            kind,
            description: escape_control(&description.into()),
        }
    }

    pub fn usage(description: impl Into<String>) -> Self {
        Self::new(Kind::Usage, description)
    }

    pub fn policy(description: impl Into<String>) -> Self {
        Self::new(Kind::Policy, description)
    }

    pub fn path(description: impl Into<String>) -> Self {
        Self::new(Kind::Path, description)
    }

    pub fn env(description: impl Into<String>) -> Self {
        Self::new(Kind::Env, description)
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
