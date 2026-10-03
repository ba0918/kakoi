//! Reads the secret files the policy names, the way specification section 14 requires
//! for them: following a link at the path (a user keeps them in dotfiles behind links),
//! only a regular file, without waiting, up to the reading limit. The outer layer; what
//! the content means is decided in `isolated_env`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::isolated_env::SecretFile;
use crate::mounts::Expansion;
use crate::regular_file::{read_regular_file, Links, ReadError};

/// Reads the file of each secret. A secret whose path has no value is as if its file did
/// not exist (specification section 5.2).
pub fn read_secret_files(secrets: &BTreeMap<String, Expansion>) -> BTreeMap<String, SecretFile> {
    secrets
        .iter()
        .map(|(name, path)| {
            let file = match path.path() {
                Some(path) => read_secret_file(path),
                None => SecretFile::Absent,
            };
            (name.clone(), file)
        })
        .collect()
}

/// Reads one secret file.
pub fn read_secret_file(path: &Path) -> SecretFile {
    match read_regular_file(path, Links::Follow) {
        Ok(bytes) => SecretFile::Bytes(bytes),
        Err(ReadError::Absent) => SecretFile::Absent,
        Err(error) => SecretFile::Unreadable(error.to_string()),
    }
}
