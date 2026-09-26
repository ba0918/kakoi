//! The command guard's rules (`[[commands.guard]]`): their shape and how they are
//! checked when a policy file is read.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The name a rule may not guard: the guard is kakoi itself.
const KAKOI: &str = "kakoi";

/// The `[commands]` table of a policy file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Commands {
    pub guard: Vec<GuardRule>,
}

/// One rule of `[[commands.guard]]`. The optional lists are `None` when the key is left
/// out: a key written with an empty list is a mistake in the rule's shape, not the same as
/// leaving it out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct GuardRule {
    pub program: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_with_value: Option<Vec<String>>,
    #[serde(default, rename = "for", skip_serializing_if = "Option::is_none")]
    pub for_: Option<Vec<Sequence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<Sequence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny_flags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny_option_values: Option<BTreeMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny_env: Option<Vec<String>>,
    #[serde(default)]
    pub guard_absolute_path: bool,
    #[serde(default, skip_serializing_if = "Examples::is_empty")]
    pub examples: Examples,
}

/// A sequence of words matched from the front: at each position one word, or a list of
/// alternatives.
pub type Sequence = Vec<Position>;

/// One position of a sequence.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Position {
    Word(String),
    Alternatives(Vec<String>),
}

impl Position {
    /// The words this position accepts.
    pub fn words(&self) -> &[String] {
        match self {
            Position::Word(word) => std::slice::from_ref(word),
            Position::Alternatives(words) => words,
        }
    }
}

/// The examples a rule is checked against when the policy is read.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Examples {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
}

impl Examples {
    fn is_empty(&self) -> bool {
        self.deny.is_none() && self.allow.is_none()
    }
}

impl GuardRule {
    /// The shape rules a type cannot say: the program is a name to look up on `PATH`
    /// other than kakoi, the reason is not empty, at least one way to deny is written, and
    /// no list or sequence written is empty.
    pub fn validate(&self) -> Result<(), String> {
        let program = &self.program;
        if program.is_empty() || program.contains('/') || program == KAKOI {
            return Err(format!(
                "`commands.guard` program {program:?} must be a non-empty name without `/` other than `kakoi`"
            ));
        }
        if self.reason.is_empty() {
            return Err(format!(
                "`commands.guard` for `{program}` needs a non-empty `reason`"
            ));
        }
        if self.deny.is_none()
            && self.deny_flags.is_none()
            && self.deny_option_values.is_none()
            && self.deny_env.is_none()
        {
            return Err(format!(
                "`commands.guard` for `{program}` needs one of `deny`, `deny-flags`, `deny-option-values`, `deny-env`"
            ));
        }
        let empty = |key: &str| {
            Err(format!(
                "`commands.guard` for `{program}` has an empty `{key}`"
            ))
        };
        for (key, sequences) in [("for", &self.for_), ("deny", &self.deny)] {
            if let Some(sequences) = sequences {
                if sequences.is_empty()
                    || sequences.iter().any(|sequence| {
                        sequence.is_empty()
                            || sequence.iter().any(|position| position.words().is_empty())
                    })
                {
                    return empty(key);
                }
            }
        }
        for (key, list) in [
            ("options-with-value", &self.options_with_value),
            ("deny-flags", &self.deny_flags),
            ("deny-env", &self.deny_env),
            ("examples.deny", &self.examples.deny),
            ("examples.allow", &self.examples.allow),
        ] {
            if list.as_ref().is_some_and(Vec::is_empty) {
                return empty(key);
            }
        }
        if let Some(values) = &self.deny_option_values {
            if values.is_empty() || values.values().any(Vec::is_empty) {
                return empty("deny-option-values");
            }
        }
        Ok(())
    }
}
