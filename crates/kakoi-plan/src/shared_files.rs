//! Shared-file descriptions and location selection from explicit environment values.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::copies::FileContent;
use crate::plan::Argument;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SharedFile {
    Empty,
    Resolver,
}

impl SharedFile {
    pub fn name(self) -> &'static str {
        match self {
            SharedFile::Empty => "empty",
            SharedFile::Resolver => "resolv.conf",
        }
    }

    pub fn content(self) -> Vec<u8> {
        match self {
            SharedFile::Empty => Vec::new(),
            SharedFile::Resolver => {
                format!("nameserver {}\n", crate::network::DNS_RESOLVER_ADDRESS).into_bytes()
            }
        }
    }

    pub fn from_data(self) -> Argument {
        match self {
            SharedFile::Empty => Argument::EmptyFile,
            SharedFile::Resolver => Argument::CopiedFile(FileContent::new(self.content())),
        }
    }
}

pub fn place(host: &BTreeMap<OsString, OsString>, nested: bool) -> Option<PathBuf> {
    if nested {
        return None;
    }
    host.get(OsStr::new("XDG_RUNTIME_DIR"))
        .map(Path::new)
        .filter(|runtime| runtime.is_absolute())
        .map(|runtime| runtime.join("kakoi"))
}
