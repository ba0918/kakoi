use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

mod common;

use common::fixture::{layers, merged};
use common::TempDir;
use process_wrap::diagnostic::{Diagnostic, Kind};
use process_wrap::isolated_env::{assemble_environment, Assembled, SecretFile};
use process_wrap::secret_facts::read_secret_file;

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

fn names(assembled: &Assembled) -> Vec<&str> {
    assembled
        .environment
        .values()
        .keys()
        .map(|name| name.to_str().unwrap())
        .collect()
}

#[test]
fn unset_accepts_wildcards() {
    let assembled = assemble(
        "[env]\nunset = [\"*_TOKEN\", \"A?\"]",
        &host(&[("GH_TOKEN", "1"), ("AB", "1"), ("ABC", "1"), ("TOKEN", "1")]),
        &BTreeMap::new(),
        &[],
    )
    .unwrap();

    assert_eq!(names(&assembled), ["ABC", "PROCESS_WRAP", "TOKEN"]);
}

#[test]
fn unset_with_brackets_removes_only_the_literal_name() {
    let assembled = assemble(
        "[env]\nunset = [\"[abc]\"]",
        &host(&[("[abc]", "1"), ("a", "1")]),
        &BTreeMap::new(),
        &[],
    )
    .unwrap();

    assert_eq!(names(&assembled), ["PROCESS_WRAP", "a"]);
}

#[test]
fn clear_without_path_leaves_path_absent() {
    let without_prepend = assemble(
        "[env]\nmode = \"clear\"\npass = [\"KEEP\"]",
        &host(&[("KEEP", "1"), ("PATH", "/usr/bin"), ("OTHER", "1")]),
        &BTreeMap::new(),
        &[],
    )
    .unwrap();
    assert_eq!(names(&without_prepend), ["KEEP", "PROCESS_WRAP"]);
    assert!(without_prepend.warnings.is_empty());

    let with_prepend = assemble(
        "[env]\nmode = \"clear\"\npath-prepend = [\"/opt/bin\"]",
        &host(&[("PATH", "/usr/bin")]),
        &BTreeMap::new(),
        &["/opt/bin"],
    )
    .unwrap();
    assert_eq!(
        with_prepend.environment.values()[&OsString::from("PATH")],
        OsString::from("/opt/bin")
    );
}

const SECRET: &str = "[secrets]\nS = \"/home/u/tokens/s\"";

#[test]
fn a_secret_removes_the_host_value_before_injecting() {
    let injected = assemble(
        SECRET,
        &host(&[("S", "host")]),
        &secret_bytes(&[("S", b"file")]),
        &[],
    )
    .unwrap();
    assert_eq!(
        injected.environment.values()[&OsString::from("S")],
        OsString::from("file")
    );

    let absent = assemble(
        SECRET,
        &host(&[("S", "host")]),
        &[("S".to_string(), SecretFile::Absent)]
            .into_iter()
            .collect(),
        &[],
    )
    .unwrap();
    assert!(!names(&absent).contains(&"S"), "{:?}", names(&absent));
}

#[test]
fn a_secret_strips_one_trailing_lf_or_crlf() {
    // One trailing newline is removed, whether LF or CR LF (a file saved by a Windows
    // editor); a lone CR is not a newline and stays (specification section 9).
    for (bytes, expected) in [
        (&b"v\n\n"[..], "v\n"),
        (b"v\n", "v"),
        (b"v", "v"),
        (b"v\r\n", "v"),
        (b"v\r", "v\r"),
    ] {
        let assembled = assemble(SECRET, &host(&[]), &secret_bytes(&[("S", bytes)]), &[]).unwrap();
        assert_eq!(
            assembled.environment.values()[&OsString::from("S")],
            OsString::from(expected),
            "{bytes:?}"
        );
    }
}

#[test]
fn an_empty_secret_file_is_a_secret_diagnostic() {
    for bytes in [&b""[..], b"\n"] {
        let diagnostic =
            assemble(SECRET, &host(&[]), &secret_bytes(&[("S", bytes)]), &[]).unwrap_err();
        assert_eq!(diagnostic.kind(), Kind::Secret, "{bytes:?}: {diagnostic}");
    }
}

#[test]
fn a_secret_with_nul_is_a_secret_diagnostic() {
    let diagnostic =
        assemble(SECRET, &host(&[]), &secret_bytes(&[("S", b"a\0b")]), &[]).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Secret, "{diagnostic}");
}

#[test]
fn an_oversized_secret_value_is_a_secret_diagnostic() {
    let limit = 64 * 1024;
    let at_limit = vec![b'x'; limit];
    let mut at_limit_with_newline = at_limit.clone();
    at_limit_with_newline.push(b'\n');
    let over = vec![b'x'; limit + 1];

    for bytes in [&at_limit, &at_limit_with_newline] {
        let assembled = assemble(SECRET, &host(&[]), &secret_bytes(&[("S", bytes)]), &[]).unwrap();
        assert_eq!(
            assembled.environment.values()[&OsString::from("S")].len(),
            limit
        );
    }
    let diagnostic = assemble(SECRET, &host(&[]), &secret_bytes(&[("S", &over)]), &[]).unwrap_err();
    assert_eq!(diagnostic.kind(), Kind::Secret, "{diagnostic}");
}

#[test]
fn a_missing_secret_file_is_a_warning_without_the_variable() {
    let assembled = assemble(
        SECRET,
        &host(&[("S", "host")]),
        &[("S".to_string(), SecretFile::Absent)]
            .into_iter()
            .collect(),
        &[],
    )
    .unwrap();

    assert!(!names(&assembled).contains(&"S"));
    assert_eq!(assembled.warnings.len(), 1, "{:?}", assembled.warnings);
    assert!(
        assembled.warnings[0]
            .to_string()
            .starts_with("process-wrap: warning: "),
        "{:?}",
        assembled.warnings
    );
}

#[test]
fn an_unusable_secret_file_is_a_secret_diagnostic() {
    let diagnostic = assemble(
        SECRET,
        &host(&[]),
        &[(
            "S".to_string(),
            SecretFile::Unreadable("is not a regular file".to_string()),
        )]
        .into_iter()
        .collect(),
        &[],
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Secret, "{diagnostic}");
}

const INSTEAD_OF: &str =
    "[git.instead-of]\n\"git@x:\" = \"https://x/\"\n\"git@y:\" = \"https://y/\"";

#[test]
fn instead_of_entries_continue_the_git_config_count() {
    let assembled = assemble(
        INSTEAD_OF,
        &host(&[
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "user.name"),
            ("GIT_CONFIG_VALUE_0", "u"),
        ]),
        &BTreeMap::new(),
        &[],
    )
    .unwrap();

    let expected: Vec<(OsString, OsString)> = host(&[
        ("GIT_CONFIG_COUNT", "3"),
        ("GIT_CONFIG_KEY_0", "user.name"),
        ("GIT_CONFIG_KEY_1", "url.https://x/.insteadof"),
        ("GIT_CONFIG_KEY_2", "url.https://y/.insteadof"),
        ("GIT_CONFIG_VALUE_0", "u"),
        ("GIT_CONFIG_VALUE_1", "git@x:"),
        ("GIT_CONFIG_VALUE_2", "git@y:"),
        ("PROCESS_WRAP", "1"),
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
}

#[test]
fn instead_of_starts_at_zero_when_the_count_is_absent() {
    let assembled = assemble(INSTEAD_OF, &host(&[]), &BTreeMap::new(), &[]).unwrap();

    assert_eq!(
        assembled.environment.values()[&OsString::from("GIT_CONFIG_COUNT")],
        OsString::from("2")
    );
    assert_eq!(
        assembled.environment.values()[&OsString::from("GIT_CONFIG_KEY_0")],
        OsString::from("url.https://x/.insteadof")
    );
}

#[test]
fn a_non_numeric_git_config_count_is_an_env_diagnostic_only_with_entries() {
    let bogus = host(&[("GIT_CONFIG_COUNT", "abc")]);

    let diagnostic = assemble(INSTEAD_OF, &bogus, &BTreeMap::new(), &[]).unwrap_err();
    assert_eq!(diagnostic.kind(), Kind::Env, "{diagnostic}");

    let without_entries = assemble("", &bogus, &BTreeMap::new(), &[]).unwrap();
    assert_eq!(
        without_entries.environment.values()[&OsString::from("GIT_CONFIG_COUNT")],
        OsString::from("abc")
    );
}

#[test]
fn an_empty_git_config_count_is_an_env_diagnostic_with_entries() {
    // git itself reads an empty count as 0, but the host handing over a broken value is
    // the situation to stop on (specification section 10); without entries it is not read.
    let empty = host(&[("GIT_CONFIG_COUNT", "")]);

    let diagnostic = assemble(INSTEAD_OF, &empty, &BTreeMap::new(), &[]).unwrap_err();
    assert_eq!(diagnostic.kind(), Kind::Env, "{diagnostic}");

    let without_entries = assemble("", &empty, &BTreeMap::new(), &[]).unwrap();
    assert_eq!(
        without_entries.environment.values()[&OsString::from("GIT_CONFIG_COUNT")],
        OsString::from("")
    );
}

#[test]
fn a_git_config_count_too_large_to_number_the_entries_is_an_env_diagnostic() {
    let at_the_limit = host(&[("GIT_CONFIG_COUNT", "18446744073709551615")]);

    let diagnostic = assemble(INSTEAD_OF, &at_the_limit, &BTreeMap::new(), &[]).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Env, "{diagnostic}");
}

#[test]
fn a_secret_git_config_count_that_is_not_a_number_is_reported_without_its_value() {
    let diagnostic = assemble(
        "[secrets]\nGIT_CONFIG_COUNT = \"/home/u/tokens/c\"\n\
         [git.instead-of]\n\"git@x:\" = \"https://x/\"",
        &host(&[]),
        &secret_bytes(&[("GIT_CONFIG_COUNT", b"hunter2\n")]),
        &[],
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Env, "{diagnostic}");
    assert!(!diagnostic.to_string().contains("hunter2"), "{diagnostic}");
}

#[test]
fn secret_values_never_appear_in_the_plan_or_its_warnings() {
    let assembled = assemble(
        "[secrets]\nS = \"/home/u/tokens/s\"\nT = \"/home/u/tokens/t\"",
        &host(&[("KEEP", "1")]),
        &[
            ("S".to_string(), SecretFile::Bytes(b"hunter2\n".to_vec())),
            ("T".to_string(), SecretFile::Absent),
        ]
        .into_iter()
        .collect(),
        &[],
    )
    .unwrap();

    let shown = assembled.environment.shown();
    assert_eq!(shown[&OsString::from("KEEP")], Some(OsString::from("1")));
    assert_eq!(shown[&OsString::from("S")], None);
    let text = format!("{shown:?} {:?}", assembled.warnings);
    assert!(!text.contains("hunter2"), "{text}");
    let debug = format!("{:?}", assembled.environment);
    assert!(!debug.contains("hunter2"), "{debug}");

    let diagnostic = assemble(
        SECRET,
        &host(&[]),
        &secret_bytes(&[("S", b"hunter2\0")]),
        &[],
    )
    .unwrap_err();
    assert!(!diagnostic.to_string().contains("hunter2"), "{diagnostic}");
}

fn assemble_from_file(path: &std::path::Path) -> Result<Assembled, Diagnostic> {
    let secrets = [("S".to_string(), read_secret_file(path))]
        .into_iter()
        .collect();
    assemble(SECRET, &host(&[]), &secrets, &[])
}

#[test]
fn a_fifo_secret_file_is_a_secret_diagnostic() {
    let dir = TempDir::new();
    let fifo = dir.path().join("fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success());

    let diagnostic = assemble_from_file(&fifo).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Secret, "{diagnostic}");
}

#[test]
fn a_secret_file_behind_a_symlink_is_read() {
    let dir = TempDir::new();
    let target = dir.write("vault/s", "linked\n");
    let link = dir.path().join("s");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let assembled = assemble_from_file(&link).unwrap();

    assert_eq!(
        assembled.environment.values()[&OsString::from("S")],
        OsString::from("linked")
    );
}

#[test]
fn an_unreadable_secret_file_is_a_secret_diagnostic() {
    let dir = TempDir::new();
    let file = dir.write("s", "v\n");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();

    let diagnostic = assemble_from_file(&file).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Secret, "{diagnostic}");
}
