use std::path::PathBuf;

mod common;

use common::fixture::{home, layers, merged, variables};
use process_wrap::environment::{HostEnvironment, RealEntry};
use process_wrap::mounts::{expand_policy, Expansion};

#[test]
fn tilde_expands_to_the_real_home_directory() {
    let home_behind_a_link = HostEnvironment {
        home: Some(PathBuf::from("/home/link")),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Directory(PathBuf::from("/home/u")))
    .unwrap();
    let policy = merged(&layers(
        "[mounts]\nrw = [\"~\", \"~/.cache\"]",
        None,
        &[],
        &[],
    ));

    let expanded = expand_policy(&policy, &variables(), &home_behind_a_link);

    let paths: Vec<&Expansion> = expanded.mounts.iter().map(|item| &item.path).collect();
    assert_eq!(
        paths,
        [
            &Expansion::Path(PathBuf::from("/home/u")),
            &Expansion::Path(PathBuf::from("/home/u/.cache")),
        ]
    );
}

#[test]
fn variables_expand_only_in_path_values() {
    let policy = merged(&layers(
        "[mounts]\nro = [\"${config_dir}/agents.md\"]\n\
         [[mounts.scan]]\nroot = \"${worktree}\"\nnames = [\".env\"]\n\
         [[mounts.hide-mounts]]\nunder = \"${workspace}/mnt\"\nfstype = [\"9p\"]\n\
         [env]\npath-prepend = [\"${git_common_dir}/bin\"]\nset = { X = \"${worktree}\" }\n\
         [secrets]\nT = \"${config_dir}/secrets/t\"",
        None,
        &[],
        &[],
    ));

    let expanded = expand_policy(&policy, &variables(), &home());

    let path = |text: &str| Expansion::Path(PathBuf::from(text));
    assert_eq!(
        expanded.mounts[0].path,
        path("/home/u/.config/process-wrap/agents.md")
    );
    assert_eq!(expanded.scans[0].root, path("/home/u/proj"));
    assert_eq!(expanded.hide_mounts[0].under, path("/home/u/proj/mnt"));
    assert_eq!(expanded.path_prepend, [path("/home/u/proj/.git/bin")]);
    assert_eq!(
        expanded.secrets["T"],
        path("/home/u/.config/process-wrap/secrets/t")
    );
    assert_eq!(policy.env_set["X"], "${worktree}");
}
