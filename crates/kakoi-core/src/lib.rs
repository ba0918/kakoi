//! `kakoi-core`: everything of `kakoi` that is not the command line. The policy files and
//! their merge, the mount and seccomp plan, and the `bwrap` command line assembled from
//! it, usable from Rust without going through the CLI. The specification is
//! `docs/spec/kakoi.md` of the repository.
//!
//! Nothing here interprets arguments, reads the process's environment or current
//! directory, writes to standard output or standard error, or executes anything: those
//! values are taken as arguments and returned as values, and the exec is the caller's.

#[cfg(not(target_arch = "x86_64"))]
compile_error!("kakoi supports only x86_64 (specification section 3)");

pub mod command;
pub mod copies;
pub mod copy_facts;
pub mod diagnostic;
pub mod environment;
pub mod executables;
pub mod isolated_env;
pub mod launch;
pub mod layers;
pub mod mount_facts;
pub mod mount_list;
pub mod mounts;
pub mod placement;
pub mod plan;
pub mod planning;
pub mod policy;
pub mod regular_file;
pub mod scan;
pub mod seccomp;
pub mod secret_facts;
pub mod variables;
pub mod wildcard;
pub mod workspace_facts;
