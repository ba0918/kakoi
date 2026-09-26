//! The command guard's rules (`[[commands.guard]]`): their shape and how they are
//! checked when a policy file is read.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::wildcard;

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

/// What a rule denied: the rule's program, the words of the run that matched (the
/// argument side), and the rule's reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    pub program: String,
    pub matched: String,
    pub reason: String,
}

/// Whether `rules` deny a run of their program with `arguments` (the words after the
/// program name) while the variables named `environment` are set. Each rule is tried in
/// order, and within a rule `deny-env`, `deny`, `deny-flags`, then `deny-option-values`;
/// the first match is the denial.
pub fn evaluate<'a>(
    rules: impl IntoIterator<Item = &'a GuardRule>,
    arguments: &[OsString],
    environment: &[OsString],
) -> Option<Denial> {
    let words: Vec<Option<&str>> = arguments.iter().map(|word| word.to_str()).collect();
    rules.into_iter().find_map(|rule| {
        rule.denial(arguments, &words, environment)
            .map(|matched| Denial {
                program: rule.program.clone(),
                matched,
                reason: rule.reason.clone(),
            })
    })
}

impl GuardRule {
    fn denial(
        &self,
        arguments: &[OsString],
        words: &[Option<&str>],
        environment: &[OsString],
    ) -> Option<String> {
        let rest = &words[self.skipped(arguments)..];
        let applies = self.for_.as_ref().is_none_or(|sequences| {
            sequences
                .iter()
                .any(|sequence| prefix(sequence, rest).is_some())
        });
        let before_separator = &words[..arguments
            .iter()
            .position(|word| word == "--")
            .unwrap_or(arguments.len())];
        let env = || {
            self.deny_env.as_ref().and_then(|patterns| {
                environment.iter().find_map(|name| {
                    patterns
                        .iter()
                        .any(|pattern| wildcard::matches(pattern, name.as_bytes()))
                        .then(|| name.to_string_lossy().into_owned())
                })
            })
        };
        let deny = || {
            self.deny
                .as_ref()
                .and_then(|sequences| sequences.iter().find_map(|sequence| prefix(sequence, rest)))
        };
        let flags = || {
            self.deny_flags
                .as_ref()
                .and_then(|flags| {
                    before_separator
                        .iter()
                        .flatten()
                        .find(|word| flags.iter().any(|flag| flag_matches(flag, word)))
                })
                .map(|word| word.to_string())
        };
        let option_values = || {
            self.deny_option_values
                .as_ref()
                .and_then(|values| option_value(values, before_separator))
        };
        if applies {
            env().or_else(deny).or_else(flags).or_else(option_values)
        } else {
            deny()
        }
    }

    /// How many leading words are global options (and their values) to skip before the
    /// prefix is matched.
    fn skipped(&self, arguments: &[OsString]) -> usize {
        let takes_value = |word: &OsStr| {
            self.options_with_value
                .iter()
                .flatten()
                .any(|name| word == OsStr::new(name))
        };
        let mut index = 0;
        while let Some(word) = arguments.get(index) {
            if word == "--" {
                return index + 1;
            }
            if !word.as_bytes().starts_with(b"-") {
                break;
            }
            index += if takes_value(word) { 2 } else { 1 };
        }
        index.min(arguments.len())
    }
}

/// The words of `rest` that `sequence` matches from its front, joined by spaces.
fn prefix(sequence: &Sequence, rest: &[Option<&str>]) -> Option<String> {
    if rest.len() < sequence.len() {
        return None;
    }
    let mut matched = Vec::with_capacity(sequence.len());
    for (position, word) in sequence.iter().zip(rest) {
        let word = (*word)?;
        if !position
            .words()
            .iter()
            .any(|pattern| word_matches(pattern, word))
        {
            return None;
        }
        matched.push(word);
    }
    Some(matched.join(" "))
}

/// Whether `word` matches `flag`: the part before the first `=` is the flag, and a
/// one-letter flag also matches that letter among the letters of a bundle.
fn flag_matches(flag: &str, word: &str) -> bool {
    let head = word.split_once('=').map_or(word, |(head, _)| head);
    if head == flag {
        return true;
    }
    let mut letters = flag.chars();
    match (letters.next(), letters.next(), letters.next()) {
        (Some('-'), Some(letter), None) if letter != '-' => {
            word.starts_with('-') && !word.starts_with("--") && word[1..].contains(letter)
        }
        _ => false,
    }
}

/// The first option and value among `words` (the words before the first `--`) whose
/// value `values` denies for that option, as the run wrote them.
fn option_value(values: &BTreeMap<String, Vec<String>>, words: &[Option<&str>]) -> Option<String> {
    let denied = |name: &str, value: &str| {
        values
            .get(name)
            .is_some_and(|patterns| patterns.iter().any(|pattern| word_matches(pattern, value)))
    };
    for (index, word) in words.iter().enumerate() {
        let Some(word) = *word else { continue };
        if let Some((name, value)) = word.split_once('=') {
            if denied(name, value) {
                return Some(word.to_string());
            }
        }
        if let Some(Some(value)) = words.get(index + 1) {
            if denied(word, value) {
                return Some(format!("{word} {value}"));
            }
        }
    }
    None
}

/// Whether `word` matches `pattern`: a text between two `/` is a regular expression
/// over the whole word, anything else the word itself.
pub fn word_matches(pattern: &str, word: &str) -> bool {
    match regular_expression(pattern) {
        Some(Ok(regex)) => regex.is_match(word),
        Some(Err(_)) => false,
        None => pattern == word,
    }
}

/// The regular expression `pattern` stands for, anchored to the whole word; `None` when
/// the pattern is a plain word. The inner text is compiled alone first so that an
/// unbalanced `)` cannot escape the anchors.
pub fn regular_expression(pattern: &str) -> Option<Result<Regex, regex::Error>> {
    let inner = pattern
        .strip_prefix('/')
        .and_then(|rest| rest.strip_suffix('/'))
        .filter(|_| pattern.len() >= 2)?;
    Some(Regex::new(inner).and_then(|_| Regex::new(&format!(r"\A(?:{inner})\z"))))
}
