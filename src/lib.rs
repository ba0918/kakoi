//! `kakoi`: runs a command inside a bubblewrap mount namespace shaped by a layered
//! policy. The specification is `docs/spec/kakoi.md`.

#[cfg(not(target_arch = "x86_64"))]
compile_error!("kakoi supports only x86_64 (specification section 3)");

pub mod cli;
pub mod command;
pub mod copies;
pub mod copy_facts;
pub mod diagnostic;
pub mod environment;
pub mod executables;
pub mod init;
pub mod isolated_env;
pub mod launch;
pub mod layers;
pub mod mount_facts;
pub mod mount_list;
pub mod mounts;
pub mod placement;
pub mod plan;
pub mod plan_json;
pub mod plan_text;
pub mod planning;
pub mod policy;
pub mod regular_file;
pub mod scan;
pub mod seccomp;
pub mod secret_facts;
pub mod startup;
pub mod variables;
pub mod wildcard;
pub mod workspace_facts;
