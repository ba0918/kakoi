//! Network execution for kakoi, usable by Rust callers independently of the CLI.
//! Configuration validation and planning belong to `kakoi-core`.

pub mod application;
pub mod dns;
pub mod dns_adoption;
pub mod dns_front;
pub mod dns_runtime;
pub mod dns_service;
pub mod dns_transport;
pub mod dns_workers;
pub mod dynamic;
pub mod filter;
pub mod health;
pub mod leases;
pub mod namespace;
pub mod nft;
pub mod notification;
pub mod pasta;
pub mod recovery;
pub mod resolution;
pub mod scope;
pub mod session;
pub mod transport;
