//! `kakoi`: runs a command inside a bubblewrap mount namespace shaped by a layered
//! policy. The specification is `docs/spec/kakoi.md`. This crate is the command-line
//! interface; everything else is `kakoi-core`.

pub mod cli;
pub mod init;
pub mod plan_json;
pub mod plan_text;
pub mod startup;
