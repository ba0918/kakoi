use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

mod common;

use common::fixture::{layers, merged};
use process_wrap::diagnostic::Diagnostic;
use process_wrap::isolated_env::{assemble_environment, Assembled, SecretFile};

fn host(pairs: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
    pairs
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
}

fn secret_bytes(pairs: &[(&str, &[u8])]) -> BTreeMap<String, SecretFile> {
    pairs
        .iter()
        .map(|(name, bytes)| (name.to_string(), SecretFile::Bytes(bytes.to_vec())))
        .collect()
}

/// Assembles the environment of the profile `profile` for the host environment `host`,
/// with the secret files as given and `path_prepend` already resolved.
fn assemble(
    profile: &str,
    host: &BTreeMap<OsString, OsString>,
    secrets: &BTreeMap<String, SecretFile>,
    path_prepend: &[&str],
) -> Result<Assembled, Diagnostic> {
    let policy = merged(&layers(profile, None, &[], &[]));
    let path_prepend: Vec<PathBuf> = path_prepend.iter().map(PathBuf::from).collect();
    assemble_environment(&policy, host, secrets, &path_prepend)
}

#[test]
fn the_environment_is_assembled_in_the_seven_stages() {
    let assembled = assemble(
        "[env]\nunset = [\"DROP\", \"NEW\"]\nset = { NEW = \"${worktree}\" }\n\
         path-prepend = [\"/opt/bin\"]\n\
         [secrets]\nS = \"/home/u/tokens/s\"\n\
         [git.instead-of]\n\"git@x:\" = \"https://x/\"",
        &host(&[
            ("DROP", "1"),
            ("KEEP", "1"),
            ("PATH", "/usr/bin"),
            ("S", "host"),
            ("PROCESS_WRAP", "stale"),
        ]),
        &secret_bytes(&[("S", b"from-file\n")]),
        &["/opt/bin"],
    )
    .unwrap();

    let expected: Vec<(OsString, OsString)> = host(&[
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "url.https://x/.insteadof"),
        ("GIT_CONFIG_VALUE_0", "git@x:"),
        ("KEEP", "1"),
        ("NEW", "${worktree}"),
        ("PATH", "/opt/bin:/usr/bin"),
        ("PROCESS_WRAP", "1"),
        ("S", "from-file"),
    ])
    .into_iter()
    .collect();
    let actual: Vec<(OsString, OsString)> = assembled
        .environment
        .values()
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    assert_eq!(actual, expected);
    assert!(assembled.warnings.is_empty(), "{:?}", assembled.warnings);
}
