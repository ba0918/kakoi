use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

mod common;

use common::TempDir;
use kakoi_core::diagnostic::Kind;
use kakoi_core::environment::{HomeDirectory, HostEnvironment};
use kakoi_core::layers::{
    load_layers, merge, Directive, Layer, LayerOrigin, LayerSelection, MountItem,
};
use kakoi_core::policy::{parse_policy, EnvMode, NetworkMode, PolicyPath};
use kakoi_core::workspace_facts::real_entry;

fn profile(text: &str) -> Layer {
    Layer {
        origin: LayerOrigin::Profile(PathBuf::from("/config/profile/default.toml")),
        policy: parse_policy(text, Path::new("/config/profile/default.toml")).unwrap(),
    }
}

fn policy_file(text: &str) -> Layer {
    Layer {
        origin: LayerOrigin::PolicyFile(PathBuf::from("/project/policy.toml")),
        policy: parse_policy(text, Path::new("/project/policy.toml")).unwrap(),
    }
}

fn absolute(path: &str) -> PolicyPath {
    PolicyPath::Absolute(PathBuf::from(path))
}

fn selection(profile: &str, policy_file: Option<&str>) -> LayerSelection {
    LayerSelection {
        profile: profile.to_string(),
        policy_file: policy_file.map(PathBuf::from),
        rw: vec![PathBuf::from("/cli/rw")],
        hide: vec![],
    }
}

fn home_directory(home: &TempDir) -> HomeDirectory {
    HostEnvironment {
        home: Some(home.path().to_path_buf()),
        xdg_config_home: None,
    }
    .home_directory(&real_entry(home.path()))
    .unwrap()
}

/// A configuration directory under a temporary `XDG_CONFIG_HOME`, derived the way the
/// binary derives it.
fn config_dir(home: &TempDir) -> PathBuf {
    HostEnvironment {
        home: Some(home.path().to_path_buf()),
        xdg_config_home: Some(home.path().join(".config")),
    }
    .config_dir(&home_directory(home))
}

#[test]
fn lists_concatenate_with_the_upper_layer_appended() {
    let lower = profile(
        "[mounts]\nrw = [\"/l/rw\"]\nhide = [\"/l/hide\"]\n\
         [[mounts.scan]]\nroot = \"/l\"\nnames = [\"a\"]\n\
         [[mounts.hide-mounts]]\nunder = \"/l\"\nfstype = [\"9p\"]\n\
         [env]\nunset = [\"L\"]\npass = []",
    );
    let upper = policy_file(
        "[mounts]\nrw = [\"/u/rw\"]\nro = [\"/u/ro\"]\n\
         [[mounts.scan]]\nroot = \"/u\"\nnames = [\"b\"]\n\
         [[mounts.hide-mounts]]\nunder = \"/u\"\nfstype = [\"drvfs\"]\n\
         [env]\nunset = [\"U\"]",
    );
    let command_line = Layer::command_line(&selection("default", None));

    let policy = merge(&[lower.clone(), upper.clone(), command_line]).unwrap();

    assert_eq!(
        policy.mounts,
        [
            MountItem {
                directive: Directive::Rw,
                path: absolute("/l/rw"),
                origin: lower.origin.clone(),
            },
            MountItem {
                directive: Directive::Hide,
                path: absolute("/l/hide"),
                origin: lower.origin.clone(),
            },
            MountItem {
                directive: Directive::Rw,
                path: absolute("/u/rw"),
                origin: upper.origin.clone(),
            },
            MountItem {
                directive: Directive::Ro,
                path: absolute("/u/ro"),
                origin: upper.origin.clone(),
            },
            MountItem {
                directive: Directive::Rw,
                path: absolute("/cli/rw"),
                origin: LayerOrigin::CommandLine,
            },
        ]
    );
    assert_eq!(
        policy
            .scan
            .iter()
            .map(|scan| &scan.root)
            .collect::<Vec<_>>(),
        [&absolute("/l"), &absolute("/u")]
    );
    assert_eq!(
        policy
            .hide_mounts
            .iter()
            .map(|item| &item.under)
            .collect::<Vec<_>>(),
        [&absolute("/l"), &absolute("/u")]
    );
    assert_eq!(policy.env_unset, ["L", "U"]);
}

#[test]
fn path_prepend_puts_the_upper_layer_first() {
    let lower = profile("[env]\npath-prepend = [\"/l/bin\", \"/l/sbin\"]");
    let upper = policy_file("[env]\npath-prepend = [\"/u/bin\"]");

    let policy = merge(&[lower, upper]).unwrap();

    assert_eq!(
        policy.path_prepend,
        [absolute("/u/bin"), absolute("/l/bin"), absolute("/l/sbin")]
    );
}

#[test]
fn scalars_take_the_upper_layer() {
    let overridden = merge(&[
        profile("[network]\nmode = \"none\"\n[env]\nmode = \"inherit\""),
        policy_file("[network]\nmode = \"host\"\n[env]\nmode = \"clear\""),
    ])
    .unwrap();
    assert_eq!(overridden.network_mode, NetworkMode::Host);
    assert_eq!(overridden.env_mode, EnvMode::Clear);

    let kept = merge(&[
        profile("[network]\nmode = \"none\"\n[env]\nmode = \"clear\""),
        policy_file(""),
    ])
    .unwrap();
    assert_eq!(kept.network_mode, NetworkMode::None);
    assert_eq!(kept.env_mode, EnvMode::Clear);

    let defaults = merge(&[profile(""), policy_file("")]).unwrap();
    assert_eq!(defaults.network_mode, NetworkMode::Host);
    assert_eq!(defaults.env_mode, EnvMode::Inherit);
}

#[test]
fn tables_merge_by_key_with_the_upper_layer_winning() {
    let lower = profile(
        "[env.set]\nA = \"lower\"\nB = \"lower\"\n\
         [secrets]\nS1 = \"/l/s1\"\nS2 = \"/l/s2\"\n\
         [git.instead-of]\n\"git@a:\" = \"https://a/\"\n\"git@b:\" = \"https://lower-b/\"",
    );
    let upper = policy_file(
        "[env.set]\nB = \"upper\"\nC = \"upper\"\n\
         [secrets]\nS2 = \"/u/s2\"\nS3 = \"/u/s3\"\n\
         [git.instead-of]\n\"git@b:\" = \"https://upper-b/\"\n\"git@c:\" = \"https://c/\"",
    );

    let policy = merge(&[lower, upper]).unwrap();

    assert_eq!(
        policy.env_set,
        BTreeMap::from([
            ("A".to_string(), "lower".to_string()),
            ("B".to_string(), "upper".to_string()),
            ("C".to_string(), "upper".to_string()),
        ])
    );
    assert_eq!(
        policy.secrets,
        BTreeMap::from([
            ("S1".to_string(), absolute("/l/s1")),
            ("S2".to_string(), absolute("/u/s2")),
            ("S3".to_string(), absolute("/u/s3")),
        ])
    );
    assert_eq!(
        policy.instead_of,
        BTreeMap::from([
            ("git@a:".to_string(), "https://a/".to_string()),
            ("git@b:".to_string(), "https://upper-b/".to_string()),
            ("git@c:".to_string(), "https://c/".to_string()),
        ])
    );
}

#[test]
fn pass_under_inherit_is_a_policy_diagnostic_after_merge() {
    let single = merge(&[profile("[env]\npass = [\"X\"]")]).unwrap_err();
    assert_eq!(single.kind(), Kind::Policy);

    let reverted = merge(&[
        profile("[env]\nmode = \"clear\"\npass = [\"X\"]"),
        policy_file("[env]\nmode = \"inherit\""),
    ])
    .unwrap_err();
    assert_eq!(reverted.kind(), Kind::Policy);
}

#[test]
fn lower_inherit_with_upper_clear_and_pass_is_accepted() {
    let policy = merge(&[
        profile("[env]\nmode = \"inherit\""),
        policy_file("[env]\nmode = \"clear\"\npass = [\"X\", \"Y\"]"),
    ])
    .unwrap();

    assert_eq!(policy.env_mode, EnvMode::Clear);
    assert_eq!(policy.env_pass, ["X", "Y"]);
}

#[test]
fn the_same_key_in_env_set_and_secrets_is_a_policy_diagnostic() {
    let across_layers = merge(&[
        profile("[env.set]\nTOKEN = \"x\""),
        policy_file("[secrets]\nTOKEN = \"/s/token\""),
    ])
    .unwrap_err();
    assert_eq!(across_layers.kind(), Kind::Policy);

    let within_one = merge(&[profile(
        "[env.set]\nTOKEN = \"x\"\n[secrets]\nTOKEN = \"/s/token\"",
    )])
    .unwrap_err();
    assert_eq!(within_one.kind(), Kind::Policy);
}

#[test]
fn a_missing_profile_file_is_a_policy_diagnostic() {
    let home = TempDir::new();
    let config = config_dir(&home);
    home.write(".config/kakoi/profile/default.toml", "");

    let diagnostic = load_layers(&selection("missing", None), &config).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Policy);
}

#[test]
fn a_missing_policy_file_target_is_a_policy_diagnostic() {
    let home = TempDir::new();
    let config = config_dir(&home);
    home.write(".config/kakoi/profile/default.toml", "");
    let target = home.path().join("absent.toml");

    let diagnostic = load_layers(
        &selection("default", Some(target.to_str().unwrap())),
        &config,
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Policy);
}

#[test]
fn an_explicit_default_profile_equals_the_omitted_form() {
    let home = TempDir::new();
    let config = config_dir(&home);
    home.write(
        ".config/kakoi/profile/default.toml",
        "[mounts]\nrw = [\"/d\"]",
    );

    let explicit = load_layers(&selection("default", None), &config).unwrap();
    let omitted = load_layers(
        &kakoi::cli::interpret(["--", "true"].map(OsString::from))
            .map(|parsed| match parsed {
                kakoi::cli::Parsed::Invocation(mut invocation) => {
                    invocation.rw = vec![PathBuf::from("/cli/rw")];
                    invocation.layer_selection()
                }
                _ => panic!("not an invocation"),
            })
            .unwrap(),
        &config,
    )
    .unwrap();

    assert_eq!(explicit, omitted);
    assert_eq!(
        explicit[0].origin,
        LayerOrigin::Profile(config.join("profile/default.toml"))
    );
    assert_eq!(explicit[0].policy.mounts.rw, [absolute("/d")]);
    assert_eq!(explicit.len(), 2);
    assert_eq!(explicit[1].origin, LayerOrigin::CommandLine);
}

#[test]
fn a_named_profile_does_not_read_default_toml() {
    let home = TempDir::new();
    let config = config_dir(&home);
    home.write(".config/kakoi/profile/default.toml", "[mounts\nbroken");
    home.write(
        ".config/kakoi/profile/strict.toml",
        "[mounts]\nrw = [\"/s\"]",
    );
    let policy_file = home.write("policy.toml", "[mounts]\nro = [\"/p\"]");

    let layers = load_layers(
        &selection("strict", Some(policy_file.to_str().unwrap())),
        &config,
    )
    .unwrap();

    assert_eq!(
        layers.iter().map(|layer| &layer.origin).collect::<Vec<_>>(),
        [
            &LayerOrigin::Profile(config.join("profile/strict.toml")),
            &LayerOrigin::PolicyFile(policy_file.clone()),
            &LayerOrigin::CommandLine,
        ]
    );
    assert_eq!(layers[0].policy.mounts.rw, [absolute("/s")]);
    assert_eq!(layers[1].policy.mounts.ro, [absolute("/p")]);
}

#[test]
fn an_empty_xdg_config_home_falls_back_to_the_home_config_dir() {
    let home = TempDir::new();
    home.write(".config/kakoi/profile/default.toml", "[mounts\nbroken");

    let config = HostEnvironment {
        home: Some(home.path().to_path_buf()),
        xdg_config_home: Some(PathBuf::new()),
    }
    .config_dir(&home_directory(&home));

    assert_eq!(
        config,
        home.path().canonicalize().unwrap().join(".config/kakoi")
    );
    let diagnostic = load_layers(&selection("default", None), &config).unwrap_err();
    assert_eq!(diagnostic.kind(), Kind::Policy);
}

#[test]
fn a_relative_xdg_config_home_falls_back_to_the_home_config_dir() {
    let home = TempDir::new();
    home.write(".config/kakoi/profile/default.toml", "[mounts\nbroken");
    home.write("xdg/kakoi/profile/default.toml", "");

    let config = HostEnvironment {
        home: Some(home.path().to_path_buf()),
        xdg_config_home: Some(PathBuf::from("xdg")),
    }
    .config_dir(&home_directory(&home));

    assert_eq!(
        config,
        home.path().canonicalize().unwrap().join(".config/kakoi")
    );
    let diagnostic = load_layers(&selection("default", None), &config).unwrap_err();
    assert_eq!(diagnostic.kind(), Kind::Policy);
}

#[test]
fn a_fifo_policy_file_is_a_policy_diagnostic() {
    let home = TempDir::new();
    let config = config_dir(&home);
    home.write(".config/kakoi/profile/default.toml", "");
    let fifo = home.path().join("policy.fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success(), "mkfifo");

    let diagnostic =
        load_layers(&selection("default", Some(fifo.to_str().unwrap())), &config).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Policy);
}

#[test]
fn an_oversized_policy_file_is_a_policy_diagnostic() {
    let home = TempDir::new();
    let config = config_dir(&home);
    home.write(".config/kakoi/profile/default.toml", "");
    let line = format!("#{}\n", "x".repeat(62));
    let exactly_one_mib = line.repeat((1 << 20) / line.len());
    assert_eq!(exactly_one_mib.len(), 1 << 20);
    let one_byte_over = format!("{exactly_one_mib}\n");
    let fits = home.write("fits.toml", &exactly_one_mib);
    let oversized = home.write("oversized.toml", &one_byte_over);

    load_layers(&selection("default", Some(fits.to_str().unwrap())), &config).unwrap();
    let diagnostic = load_layers(
        &selection("default", Some(oversized.to_str().unwrap())),
        &config,
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Policy);
}

#[test]
fn a_policy_file_behind_a_symlink_is_read() {
    let home = TempDir::new();
    let config = config_dir(&home);
    home.write(".config/kakoi/profile/default.toml", "");
    let target = home.write("dotfiles/policy.toml", "[mounts]\nro = [\"/p\"]");
    let link = home.path().join("policy.toml");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let layers = load_layers(&selection("default", Some(link.to_str().unwrap())), &config).unwrap();

    assert_eq!(layers[1].policy.mounts.ro, [absolute("/p")]);
}
