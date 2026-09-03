//! `process-wrap`: runs a command inside a bubblewrap mount namespace shaped by a layered
//! policy. The specification is `docs/spec/process-wrap.md`.

#[cfg(not(target_arch = "x86_64"))]
compile_error!("process-wrap supports only x86_64 (specification section 3)");

pub mod cli;
pub mod diagnostic;
pub mod environment;
pub mod layers;
pub mod policy;
pub mod variables;
pub mod workspace_facts;
