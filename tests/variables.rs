use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

use common::TempDir;
use process_wrap::diagnostic::Kind;
use process_wrap::environment::{HomeDirectory, HostEnvironment, RealEntry};
use process_wrap::variables::{
    derive_variables, Ancestor, DotGit, GitFileLinks, Reference, WorkspaceFacts,
};
use process_wrap::workspace_facts::{collect_workspace_facts, real_entry};

/// The checked home directory `/home/u`.
fn home() -> HomeDirectory {
    HostEnvironment {
        home: Some(PathBuf::from("/home/u")),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Directory(PathBuf::from("/home/u")))
    .unwrap()
}

/// The configuration directory under that home, as found on disk.
fn config_dir() -> RealEntry {
    RealEntry::Directory(PathBuf::from("/home/u/.config/process-wrap"))
}

fn ancestor(path: &str, dot_git: DotGit) -> Ancestor {
    Ancestor {
        path: PathBuf::from(path),
        dot_git,
    }
}

/// Facts for a workspace at `/home/u/proj/sub` whose `.git` entries are given per level,
/// from the workspace upward.
fn facts(dot_gits: [DotGit; 4], links: Option<GitFileLinks>) -> WorkspaceFacts {
    WorkspaceFacts {
        workspace: RealEntry::Directory(PathBuf::from("/home/u/proj/sub")),
        ancestors: vec![
            ancestor("/home/u/proj/sub", dot_gits[0]),
            ancestor("/home/u/proj", dot_gits[1]),
            ancestor("/home/u", dot_gits[2]),
            ancestor("/home", dot_gits[3]),
            ancestor("/", DotGit::Absent),
        ],
        links,
    }
}

fn linked_worktree_links() -> GitFileLinks {
    GitFileLinks {
        gitdir: Some(PathBuf::from("/home/u/main/.git/worktrees/proj")),
        commondir: Reference::Resolved(PathBuf::from("/home/u/main/.git")),
        back_link: Some(PathBuf::from("/home/u/proj/.git")),
        core_worktree: None,
    }
}

#[test]
fn a_git_directory_is_the_common_dir() {
    let variables = derive_variables(
        &home(),
        &config_dir(),
        &facts(
            [
                DotGit::Absent,
                DotGit::Directory,
                DotGit::Absent,
                DotGit::Absent,
            ],
            None,
        ),
    )
    .unwrap();

    assert_eq!(variables.workspace, Path::new("/home/u/proj/sub"));
    assert_eq!(variables.worktree, Path::new("/home/u/proj"));
    assert_eq!(
        variables.git_common_dir.as_deref(),
        Some(Path::new("/home/u/proj/.git"))
    );
}

#[test]
fn a_linked_worktree_with_a_back_link_yields_the_common_dir() {
    let variables = derive_variables(
        &home(),
        &config_dir(),
        &facts(
            [DotGit::Absent, DotGit::File, DotGit::Absent, DotGit::Absent],
            Some(linked_worktree_links()),
        ),
    )
    .unwrap();

    assert_eq!(variables.worktree, Path::new("/home/u/proj"));
    assert_eq!(
        variables.git_common_dir.as_deref(),
        Some(Path::new("/home/u/main/.git"))
    );
}

#[test]
fn a_linked_worktree_whose_back_link_points_elsewhere_is_a_path_diagnostic() {
    let elsewhere = GitFileLinks {
        back_link: Some(PathBuf::from("/home/u/other/.git")),
        ..linked_worktree_links()
    };
    let outside_worktrees = GitFileLinks {
        gitdir: Some(PathBuf::from("/home/u/main/.git/elsewhere/proj")),
        ..linked_worktree_links()
    };
    let dangling = GitFileLinks {
        back_link: None,
        ..linked_worktree_links()
    };

    for links in [elsewhere, outside_worktrees, dangling] {
        let diagnostic = derive_variables(
            &home(),
            &config_dir(),
            &facts(
                [DotGit::Absent, DotGit::File, DotGit::Absent, DotGit::Absent],
                Some(links.clone()),
            ),
        )
        .expect_err(&format!("{links:?}"));

        assert_eq!(diagnostic.kind(), Kind::Path, "{links:?}");
    }
}

#[test]
fn a_submodule_with_core_worktree_pointing_back_yields_its_gitdir() {
    let variables = derive_variables(
        &home(),
        &config_dir(),
        &facts(
            [
                DotGit::File,
                DotGit::Directory,
                DotGit::Absent,
                DotGit::Absent,
            ],
            Some(GitFileLinks {
                gitdir: Some(PathBuf::from("/home/u/proj/.git/modules/sub")),
                commondir: Reference::Absent,
                back_link: None,
                core_worktree: Some(PathBuf::from("/home/u/proj/sub")),
            }),
        ),
    )
    .unwrap();

    assert_eq!(variables.worktree, Path::new("/home/u/proj/sub"));
    assert_eq!(
        variables.git_common_dir.as_deref(),
        Some(Path::new("/home/u/proj/.git/modules/sub"))
    );
}

#[test]
fn a_git_file_without_any_back_link_is_a_path_diagnostic() {
    let no_links = GitFileLinks {
        gitdir: Some(PathBuf::from("/home/u/proj/.git/modules/sub")),
        commondir: Reference::Absent,
        back_link: None,
        core_worktree: None,
    };
    let wrong_core_worktree = GitFileLinks {
        core_worktree: Some(PathBuf::from("/home/u/proj/other")),
        ..no_links.clone()
    };
    let dangling_gitdir = GitFileLinks {
        gitdir: None,
        ..no_links.clone()
    };
    let unresolvable_commondir = GitFileLinks {
        commondir: Reference::Unresolvable,
        core_worktree: Some(PathBuf::from("/home/u/proj/sub")),
        ..no_links.clone()
    };

    for links in [
        no_links,
        wrong_core_worktree,
        dangling_gitdir,
        unresolvable_commondir,
    ] {
        let diagnostic = derive_variables(
            &home(),
            &config_dir(),
            &facts(
                [
                    DotGit::File,
                    DotGit::Directory,
                    DotGit::Absent,
                    DotGit::Absent,
                ],
                Some(links.clone()),
            ),
        )
        .expect_err(&format!("{links:?}"));

        assert_eq!(diagnostic.kind(), Kind::Path, "{links:?}");
    }
}

#[test]
fn a_symlinked_dot_git_is_not_a_worktree_marker() {
    let outer_repo = derive_variables(
        &home(),
        &config_dir(),
        &facts(
            [
                DotGit::Symlink,
                DotGit::Directory,
                DotGit::Absent,
                DotGit::Absent,
            ],
            None,
        ),
    )
    .unwrap();
    assert_eq!(outer_repo.worktree, Path::new("/home/u/proj"));
    assert_eq!(
        outer_repo.git_common_dir.as_deref(),
        Some(Path::new("/home/u/proj/.git"))
    );

    let no_repo = derive_variables(
        &home(),
        &config_dir(),
        &facts(
            [
                DotGit::Symlink,
                DotGit::Other,
                DotGit::Absent,
                DotGit::Absent,
            ],
            None,
        ),
    )
    .unwrap();
    assert_eq!(no_repo.worktree, Path::new("/home/u/proj/sub"));
    assert_eq!(no_repo.git_common_dir, None);
}

#[test]
fn a_worktree_at_home_is_a_path_diagnostic() {
    let diagnostic = derive_variables(
        &home(),
        &config_dir(),
        &facts(
            [
                DotGit::Absent,
                DotGit::Absent,
                DotGit::Directory,
                DotGit::Absent,
            ],
            None,
        ),
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path);
}

#[test]
fn a_worktree_at_an_ancestor_of_home_is_a_path_diagnostic() {
    let at_home_parent = derive_variables(
        &home(),
        &config_dir(),
        &facts(
            [
                DotGit::Absent,
                DotGit::Absent,
                DotGit::Absent,
                DotGit::Directory,
            ],
            None,
        ),
    )
    .unwrap_err();
    assert_eq!(at_home_parent.kind(), Kind::Path);

    let at_root = derive_variables(
        &home(),
        &config_dir(),
        &WorkspaceFacts {
            workspace: RealEntry::Directory(PathBuf::from("/")),
            ancestors: vec![ancestor("/", DotGit::Absent)],
            links: None,
        },
    )
    .unwrap_err();
    assert_eq!(at_root.kind(), Kind::Path);

    let workspace_at_home = derive_variables(
        &home(),
        &config_dir(),
        &WorkspaceFacts {
            workspace: RealEntry::Directory(PathBuf::from("/home/u")),
            ancestors: vec![
                ancestor("/home/u", DotGit::Absent),
                ancestor("/home", DotGit::Absent),
                ancestor("/", DotGit::Absent),
            ],
            links: None,
        },
    )
    .unwrap_err();
    assert_eq!(workspace_at_home.kind(), Kind::Path);
}

#[test]
fn a_missing_workspace_is_a_path_diagnostic() {
    let diagnostic = derive_variables(
        &home(),
        &config_dir(),
        &WorkspaceFacts {
            workspace: RealEntry::Missing,
            ancestors: vec![],
            links: None,
        },
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path);
}

#[test]
fn a_workspace_that_is_a_regular_file_is_a_path_diagnostic() {
    let diagnostic = derive_variables(
        &home(),
        &config_dir(),
        &WorkspaceFacts {
            workspace: RealEntry::NotDirectory(PathBuf::from("/home/u/proj/file")),
            ancestors: vec![],
            links: None,
        },
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path);
}

#[test]
fn a_missing_home_is_an_env_diagnostic() {
    let diagnostic = HostEnvironment {
        home: None,
        xdg_config_home: Some(PathBuf::from("/xdg")),
    }
    .home_directory(&RealEntry::Missing)
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Env);
}

#[test]
fn an_empty_home_is_an_env_diagnostic() {
    let diagnostic = HostEnvironment {
        home: Some(PathBuf::new()),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Directory(PathBuf::from("/cwd")))
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Env);
}

#[test]
fn a_relative_home_is_an_env_diagnostic() {
    let diagnostic = HostEnvironment {
        home: Some(PathBuf::from("home/u")),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Directory(PathBuf::from("/cwd/home/u")))
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Env);
}

#[test]
fn a_home_without_a_real_path_is_an_env_diagnostic() {
    let diagnostic = HostEnvironment {
        home: Some(PathBuf::from("/home/u")),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Missing)
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Env);
}

#[test]
fn a_home_that_is_a_regular_file_is_an_env_diagnostic() {
    let diagnostic = HostEnvironment {
        home: Some(PathBuf::from("/home/u")),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::NotDirectory(PathBuf::from("/home/u")))
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Env);
}

#[test]
fn a_checked_home_is_the_real_path_of_home() {
    let home = HostEnvironment {
        home: Some(PathBuf::from("/home/link")),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Directory(PathBuf::from("/home/u")))
    .unwrap();

    assert_eq!(home.path(), Path::new("/home/u"));
}

#[test]
fn a_config_dir_without_a_real_path_is_a_path_diagnostic() {
    let diagnostic = derive_variables(
        &home(),
        &RealEntry::Missing,
        &facts(
            [
                DotGit::Absent,
                DotGit::Directory,
                DotGit::Absent,
                DotGit::Absent,
            ],
            None,
        ),
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path);
}

#[test]
fn the_config_dir_follows_xdg_config_home() {
    let with_xdg = HostEnvironment {
        home: Some(PathBuf::from("/home/u")),
        xdg_config_home: Some(PathBuf::from("/xdg")),
    }
    .config_dir(&home());
    assert_eq!(with_xdg, Path::new("/xdg/process-wrap"));

    let without_xdg = HostEnvironment {
        home: Some(PathBuf::from("/home/u")),
        xdg_config_home: None,
    }
    .config_dir(&home());
    assert_eq!(without_xdg, Path::new("/home/u/.config/process-wrap"));

    let repository = facts(
        [
            DotGit::Absent,
            DotGit::Directory,
            DotGit::Absent,
            DotGit::Absent,
        ],
        None,
    );
    let variables = derive_variables(
        &home(),
        &RealEntry::Directory(PathBuf::from("/real/xdg/process-wrap")),
        &repository,
    )
    .unwrap();
    assert_eq!(variables.config_dir, Path::new("/real/xdg/process-wrap"));
}

/// Runs git in `cwd` with the developer's own configuration shut out.
fn git(cwd: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["-c", "protocol.file.allow=always"])
        .args(arguments)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn checked_home(home: &TempDir) -> HomeDirectory {
    HostEnvironment {
        home: Some(home.path().to_path_buf()),
        xdg_config_home: None,
    }
    .home_directory(&real_entry(home.path()))
    .unwrap()
}

fn real_config_dir(home: &TempDir) -> RealEntry {
    let config_dir = home.path().join(".config/process-wrap");
    std::fs::create_dir_all(&config_dir).unwrap();
    real_entry(&config_dir)
}

#[test]
fn raw_git_facts_are_read_from_a_real_linked_worktree() {
    let home = TempDir::new();
    let real_home = home.path().canonicalize().unwrap();
    let main = real_home.join("main");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q"]);
    git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
    git(&main, &["worktree", "add", "-q", "../wt", "-b", "wt"]);
    let worktree = real_home.join("wt");
    let inside = worktree.join("inside");
    std::fs::create_dir(&inside).unwrap();

    let facts = collect_workspace_facts(&inside);

    assert_eq!(facts.workspace, RealEntry::Directory(inside.clone()));
    assert_eq!(
        &facts.ancestors[..2],
        [
            Ancestor {
                path: inside.clone(),
                dot_git: DotGit::Absent
            },
            Ancestor {
                path: worktree.clone(),
                dot_git: DotGit::File
            },
        ]
    );
    assert_eq!(facts.ancestors.last().unwrap().path, Path::new("/"));
    assert_eq!(
        facts.links,
        Some(GitFileLinks {
            gitdir: Some(main.join(".git/worktrees/wt")),
            commondir: Reference::Resolved(main.join(".git")),
            back_link: Some(worktree.join(".git")),
            core_worktree: None,
        })
    );
    let variables =
        derive_variables(&checked_home(&home), &real_config_dir(&home), &facts).unwrap();
    assert_eq!(variables.worktree, worktree);
    assert_eq!(variables.git_common_dir, Some(main.join(".git")));
}

#[test]
fn raw_git_facts_are_read_from_a_real_submodule() {
    let home = TempDir::new();
    let real_home = home.path().canonicalize().unwrap();
    let lib = real_home.join("lib");
    std::fs::create_dir(&lib).unwrap();
    git(&lib, &["init", "-q"]);
    git(&lib, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let main = real_home.join("main");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q"]);
    git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
    git(&main, &["submodule", "add", "-q", "../lib", "sub"]);
    let submodule = main.join("sub");

    let facts = collect_workspace_facts(&submodule);

    assert_eq!(facts.ancestors[0].dot_git, DotGit::File);
    assert_eq!(facts.ancestors[1].dot_git, DotGit::Directory);
    assert_eq!(
        facts.links,
        Some(GitFileLinks {
            gitdir: Some(main.join(".git/modules/sub")),
            commondir: Reference::Absent,
            back_link: None,
            core_worktree: Some(submodule.clone()),
        })
    );
    let variables =
        derive_variables(&checked_home(&home), &real_config_dir(&home), &facts).unwrap();
    assert_eq!(variables.worktree, submodule);
    assert_eq!(
        variables.git_common_dir,
        Some(main.join(".git/modules/sub"))
    );
}

#[test]
fn a_fifo_under_the_named_gitdir_is_a_path_diagnostic() {
    for name in ["commondir", "gitdir", "config"] {
        let home = TempDir::new();
        let worktree = home.path().canonicalize().unwrap().join("wt");
        home.write("wt/.git", "gitdir: g\n");
        let gitdir = worktree.join("g");
        std::fs::create_dir(&gitdir).unwrap();
        let status = Command::new("mkfifo")
            .arg(gitdir.join(name))
            .status()
            .unwrap();
        assert!(status.success(), "mkfifo {name}");

        let facts = collect_workspace_facts(&worktree);
        let diagnostic = derive_variables(&checked_home(&home), &real_config_dir(&home), &facts)
            .expect_err(name);

        assert_eq!(diagnostic.kind(), Kind::Path, "{name}");
    }
}

#[test]
fn the_config_dir_variable_is_the_real_path() {
    let home = TempDir::new();
    let real_home = home.path().canonicalize().unwrap();
    std::fs::create_dir_all(real_home.join("dotfiles/config/process-wrap")).unwrap();
    std::os::unix::fs::symlink(real_home.join("dotfiles/config"), real_home.join("link")).unwrap();
    let workspace = real_home.join("ws");
    std::fs::create_dir(&workspace).unwrap();
    let env = HostEnvironment {
        home: Some(home.path().to_path_buf()),
        xdg_config_home: Some(real_home.join("link")),
    };
    let checked = env.home_directory(&real_entry(home.path())).unwrap();
    let config_dir = env.config_dir(&checked);
    assert_eq!(config_dir, real_home.join("link/process-wrap"));

    let variables = derive_variables(
        &checked,
        &real_entry(&config_dir),
        &collect_workspace_facts(&workspace),
    )
    .unwrap();

    assert_eq!(
        variables.config_dir,
        real_home.join("dotfiles/config/process-wrap")
    );
}
