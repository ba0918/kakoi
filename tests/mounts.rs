use std::path::PathBuf;

mod common;

use common::fixture::{layers, merged, variables};
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
