//! `kakoi`: runs a command inside a bubblewrap mount namespace shaped by a layered
//! policy. The specification is `docs/ir/`, read through `docs/guide/`. This crate is the command-line
//! interface; policy, planning, Linux operations, and the shared runtime are in the crates
//! under `crates/`.

pub mod cli;
pub mod first_process;
pub mod guard;
pub mod init;
pub mod plan_json;
pub mod plan_text;
pub mod startup;
