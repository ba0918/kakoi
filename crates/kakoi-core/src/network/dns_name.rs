use std::str::FromStr;

use serde::{Deserialize, Serialize};

const HOST_NAMES: [&str; 2] = ["host-v4.kakoi.internal", "host-v6.kakoi.internal"];

/// An absolute DNS name or its descendants, normalized once using UTS #46.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String")]
pub struct DnsPattern {
    ascii: String,
    descendants: bool,
}

pub(super) fn normalize(name: &str) -> Result<String, String> {
    if name.is_empty() || name.contains(['*', '＊']) {
        return Err(format!("invalid DNS name {name:?}"));
    }
    let name = name.strip_suffix('.').unwrap_or(name);
    let ascii =
        idna::domain_to_ascii_strict(name).map_err(|_| format!("invalid DNS name {name:?}"))?;
    if ascii.is_empty() || ascii.ends_with('.') {
        return Err(format!("invalid DNS name {name:?}"));
    }
    Ok(ascii)
}

impl DnsPattern {
    pub fn ascii_name(&self) -> &str {
        &self.ascii
    }

    pub fn matches(&self, name: &str) -> bool {
        let Ok(name) = normalize(name) else {
            return false;
        };
        if HOST_NAMES.contains(&name.as_str()) {
            return false;
        }
        if self.descendants {
            name.strip_suffix(&self.ascii)
                .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
        } else {
            name == self.ascii
        }
    }
}

impl FromStr for DnsPattern {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (name, descendants) = input
            .strip_prefix("*.")
            .map_or((input, false), |name| (name, true));
        let ascii = normalize(name)?;
        if !descendants && HOST_NAMES.contains(&ascii.as_str()) {
            return Err("use a host-loopback destination for the reserved host name".into());
        }
        Ok(Self { ascii, descendants })
    }
}

impl TryFrom<String> for DnsPattern {
    type Error = String;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        input.parse()
    }
}

impl std::fmt::Display for DnsPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.descendants {
            f.write_str("*.")?;
        }
        f.write_str(&self.ascii)
    }
}
