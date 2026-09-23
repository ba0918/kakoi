use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

/// A nonempty, normalized union of transport port ranges, excluding port zero.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "Vec<String>")]
pub struct Ports(Vec<RangeInclusive<u16>>);

impl Ports {
    pub fn overlaps(&self, other: &Self) -> bool {
        let (mut left, mut right) = (0, 0);
        while left < self.0.len() && right < other.0.len() {
            let (a, b) = (&self.0[left], &other.0[right]);
            if a.end() < b.start() {
                left += 1;
            } else if b.end() < a.start() {
                right += 1;
            } else {
                return true;
            }
        }
        false
    }

    pub fn contains(&self, port: u16) -> bool {
        self.0.iter().any(|range| range.contains(&port))
    }
}

fn number(text: &str) -> Result<u16, String> {
    if text.starts_with('0') || text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "invalid port number {text:?}: expected 1..65535 without leading zeros"
        ));
    }
    text.parse::<u16>()
        .map_err(|_| format!("port {text:?} is outside 1..65535"))
}

impl TryFrom<Vec<String>> for Ports {
    type Error = String;

    fn try_from(items: Vec<String>) -> Result<Self, Self::Error> {
        if items.is_empty() {
            return Err("ports must not be empty".into());
        }
        if items.len() == 1 && items[0] == "*" {
            return Ok(Self(std::iter::once(1..=u16::MAX).collect()));
        }
        let mut ranges = Vec::with_capacity(items.len());
        for item in items {
            let (start, end) = item.split_once('-').unwrap_or((&item, &item));
            let (start, end) = (number(start)?, number(end)?);
            if start > end {
                return Err(format!("port range {item:?} is reversed"));
            }
            ranges.push((start, end));
        }
        ranges.sort_unstable();
        let mut merged: Vec<RangeInclusive<u16>> = Vec::new();
        for (start, end) in ranges {
            if let Some(last) = merged.last_mut() {
                if start <= last.end().saturating_add(1) {
                    *last = *last.start()..=end.max(*last.end());
                    continue;
                }
            }
            merged.push(start..=end);
        }
        Ok(Self(merged))
    }
}

impl std::fmt::Display for Ports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, range) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}", range.start())?;
            if range.start() != range.end() {
                write!(f, "-{}", range.end())?;
            }
        }
        Ok(())
    }
}
