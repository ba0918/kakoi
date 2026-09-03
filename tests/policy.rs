use std::path::{Path, PathBuf};

use process_wrap::diagnostic::Kind;
use process_wrap::policy::{parse_policy, EnvMode, NetworkMode, PolicyPath, Variable};

const EXAMPLE: &str = r#"
[mounts]
rw      = ["${workspace}", "${worktree}", "${git_common_dir}", "/tmp/process-wrap", "~/.cache"]
rw-file = ["~/.claude.json"]
ro      = ["~/.codex/AGENTS.md"]
hide    = ["/tmp", "/run/user", "~/.ssh", "~/.aws", "/run/WSL"]

[[mounts.scan]]
root    = "${worktree}"                # 必須
names   = [".env", ".env.*"]           # 必須、空は不可
exclude = ["*.example", "*.sample", "*.dist", "*.template"]   # 省略時は空
prune   = [".git", "node_modules"]     # 省略時は空

[[mounts.hide-mounts]]
under  = "/mnt"                        # 必須
fstype = ["9p", "drvfs"]               # 必須、空は不可

[network]
mode = "host"            # "host" | "none"。省略時 "host"

[env]
mode         = "inherit" # "inherit" | "clear"。省略時 "inherit"
pass         = []        # 合成後に mode が "clear" のときだけ意味を持つ（第 5.5 節）
set          = { }
unset        = ["SSH_AUTH_SOCK", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "*_TOKEN", "*_API_KEY"]
path-prepend = []

[secrets]
GH_TOKEN = "${config_dir}/secrets/gh-token"

[git.instead-of]
"git@github.com:" = "https://github.com/"
"#;

fn origin() -> &'static Path {
    Path::new("/policy/example.toml")
}

fn assert_policy_diagnostic(text: &str) {
    let diagnostic = parse_policy(text, origin()).expect_err(text);
    assert_eq!(diagnostic.kind(), Kind::Policy, "{text}");
}

#[test]
fn the_example_policy_file_loads() {
    let policy = parse_policy(EXAMPLE, origin()).unwrap();

    assert_eq!(
        policy.mounts.rw,
        [
            PolicyPath::Variable(Variable::Workspace, String::new()),
            PolicyPath::Variable(Variable::Worktree, String::new()),
            PolicyPath::Variable(Variable::GitCommonDir, String::new()),
            PolicyPath::Absolute(PathBuf::from("/tmp/process-wrap")),
            PolicyPath::Home("/.cache".to_string()),
        ]
    );
    assert_eq!(
        policy.mounts.rw_file,
        [PolicyPath::Home("/.claude.json".to_string())]
    );
    assert_eq!(
        policy.mounts.ro,
        [PolicyPath::Home("/.codex/AGENTS.md".to_string())]
    );
    assert_eq!(policy.mounts.hide.len(), 5);
    assert_eq!(policy.mounts.scan.len(), 1);
    assert_eq!(
        policy.mounts.scan[0].root,
        PolicyPath::Variable(Variable::Worktree, String::new())
    );
    assert_eq!(policy.mounts.scan[0].names, [".env", ".env.*"]);
    assert_eq!(policy.mounts.scan[0].exclude.len(), 4);
    assert_eq!(policy.mounts.scan[0].prune, [".git", "node_modules"]);
    assert_eq!(policy.mounts.hide_mounts.len(), 1);
    assert_eq!(
        policy.mounts.hide_mounts[0].under,
        PolicyPath::Absolute(PathBuf::from("/mnt"))
    );
    assert_eq!(policy.mounts.hide_mounts[0].fstype, ["9p", "drvfs"]);
    assert_eq!(policy.network.mode, Some(NetworkMode::Host));
    assert_eq!(policy.env.mode, Some(EnvMode::Inherit));
    assert!(policy.env.pass.is_empty());
    assert!(policy.env.set.is_empty());
    assert_eq!(policy.env.unset.len(), 6);
    assert!(policy.env.path_prepend.is_empty());
    assert_eq!(
        policy.secrets.get("GH_TOKEN"),
        Some(&PolicyPath::Variable(
            Variable::ConfigDir,
            "/secrets/gh-token".to_string()
        ))
    );
    assert_eq!(
        policy
            .git
            .instead_of
            .get("git@github.com:")
            .map(String::as_str),
        Some("https://github.com/")
    );
}

#[test]
fn an_empty_policy_file_is_valid() {
    let policy = parse_policy("", origin()).unwrap();

    assert!(policy.mounts.rw.is_empty());
    assert!(policy.mounts.scan.is_empty());
    assert_eq!(policy.network.mode, None);
    assert_eq!(policy.env.mode, None);
    assert!(policy.secrets.is_empty());
    assert!(policy.git.instead_of.is_empty());
}

#[test]
fn an_unknown_fixed_key_is_a_policy_diagnostic() {
    for text in [
        "mount = []",
        "[mounts]\nrw-dir = []",
        "[[mounts.scan]]\nroot = \"/r\"\nnames = [\"x\"]\nignore = []",
        "[[mounts.hide-mounts]]\nunder = \"/mnt\"\nfstype = [\"9p\"]\nname = \"x\"",
        "[network]\nmodes = \"host\"",
        "[env]\nappend = []",
        "[git]\nrewrite = {}",
        "[network]\nmode = \"bridge\"",
        "[env]\nmode = \"empty\"",
    ] {
        assert_policy_diagnostic(text);
    }
}

#[test]
fn unparsable_toml_is_a_policy_diagnostic() {
    assert_policy_diagnostic("[mounts\nrw = [");
}

#[test]
fn scan_without_names_is_a_policy_diagnostic() {
    for text in [
        "[[mounts.scan]]\nroot = \"/r\"",
        "[[mounts.scan]]\nroot = \"/r\"\nnames = []",
        "[[mounts.scan]]\nnames = [\".env\"]",
    ] {
        assert_policy_diagnostic(text);
    }
}

#[test]
fn hide_mounts_with_empty_fstype_is_a_policy_diagnostic() {
    for text in [
        "[[mounts.hide-mounts]]\nunder = \"/mnt\"\nfstype = []",
        "[[mounts.hide-mounts]]\nunder = \"/mnt\"",
        "[[mounts.hide-mounts]]\nfstype = [\"9p\"]",
    ] {
        assert_policy_diagnostic(text);
    }
}

#[test]
fn a_relative_path_is_a_policy_diagnostic() {
    for text in [
        "[mounts]\nrw = [\"src\"]",
        "[mounts]\nrw-file = [\"./a.json\"]",
        "[mounts]\nro = [\"../x\"]",
        "[mounts]\nhide = [\"\"]",
        "[[mounts.scan]]\nroot = \"src\"\nnames = [\".env\"]",
        "[[mounts.hide-mounts]]\nunder = \"mnt\"\nfstype = [\"9p\"]",
        "[secrets]\nTOKEN = \"secrets/token\"",
        "[env]\npath-prepend = [\"bin\"]",
    ] {
        assert_policy_diagnostic(text);
    }
}

#[test]
fn the_tilde_user_form_is_a_policy_diagnostic() {
    for text in [
        "[mounts]\nrw = [\"~root/x\"]",
        "[mounts]\nhide = [\"~root\"]",
        "[secrets]\nTOKEN = \"~alice/.token\"",
    ] {
        assert_policy_diagnostic(text);
    }
}

#[test]
fn an_unknown_variable_is_a_policy_diagnostic() {
    for text in [
        "[mounts]\nrw = [\"${home}/x\"]",
        "[mounts]\nrw = [\"${worktree\"]",
        "[secrets]\nTOKEN = \"${secrets_dir}/token\"",
        "[env]\npath-prepend = [\"${WORKSPACE}/bin\"]",
    ] {
        assert_policy_diagnostic(text);
    }
}

#[test]
fn env_set_secrets_and_instead_of_accept_any_key_name() {
    let policy = parse_policy(
        "[env.set]\n\"my weird key\" = \"1\"\nlower-case = \"2\"\n\
         [secrets]\n\"x.y\" = \"/s/x\"\n\
         [git.instead-of]\n\"ssh://git@example.com/\" = \"https://example.com/\"",
        origin(),
    )
    .unwrap();

    assert_eq!(
        policy.env.set.get("my weird key").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        policy.env.set.get("lower-case").map(String::as_str),
        Some("2")
    );
    assert_eq!(
        policy.secrets.get("x.y"),
        Some(&PolicyPath::Absolute(PathBuf::from("/s/x")))
    );
    assert_eq!(
        policy
            .git
            .instead_of
            .get("ssh://git@example.com/")
            .map(String::as_str),
        Some("https://example.com/")
    );
}
