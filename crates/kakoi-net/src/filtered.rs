//! Starting a filtered plan: locate the tools, prepare and verify the network
//! enforcement, then start the application inside it.

use crate::{
    dns_runtime::{DnsRuntimeConfig, HostDns},
    dns_transport::TlsClient,
    filter::FilterRule,
    host::{self, HOST_LOOPBACK_V4, HOST_LOOPBACK_V6},
    host_dns, pasta,
    scope::AddressContext,
    session::Session,
    supervisor::Application,
    transport::Transport,
};
use kakoi_linux::command_location::locate_command;
use kakoi_plan::plan::Plan;
use kakoi_policy::{
    diagnostic::Diagnostic,
    layers::Policy,
    network::{Allow, Destination, DnsUpstream, FixedPublication, IpFamily, IpNetwork},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

const PASTA_STARTUP: Duration = Duration::from_secs(10);

/// The instructions for installing a pasta filtered mode can use, on the main
/// branch so that a released binary still points at the current advice.
pub const PASTA_GUIDE: &str = "https://github.com/ba0918/kakoi/blob/main/docs/pasta.md";

/// The external programs a filtered run needs, found on the host's `PATH` the
/// way `bwrap` is.
pub struct Tools {
    pub pasta: PathBuf,
    pub nft: PathBuf,
}

impl Tools {
    pub fn locate(host: &BTreeMap<OsString, OsString>) -> Result<Self, Diagnostic> {
        let find = |name: &str, advice: &str| {
            locate_command(name.as_ref(), host).map_err(|_| {
                Diagnostic::bwrap(format!(
                    "filtered network mode needs `{name}`, which is not on PATH{advice}"
                ))
            })
        };
        Ok(Self {
            pasta: find("pasta", &format!("; see {PASTA_GUIDE}"))?,
            nft: find("nft", "")?,
        })
    }
}

/// Prepares the network of `plan`, verifies that it is enforced, and only then
/// starts the application. `init` is the executable that runs as the isolation's
/// process 1 (see [`crate::init::run_if_requested`]).
pub fn start(
    plan: &Plan,
    tools: &Tools,
    init: &Path,
    stdout: Stdio,
) -> Result<(Session, Application), Diagnostic> {
    let session = prepare_session(&plan.policy, tools).map_err(|error| error.diagnostic)?;
    let application = Application::spawn(
        plan,
        session.namespace(),
        init,
        Duration::from_secs(plan.policy.shutdown_grace_seconds.into()),
        stdout,
    )?;
    Ok((session, application))
}

/// Network setup failure with the OS cause retained for structured consumers.
pub struct PreparationError {
    pub diagnostic: Diagnostic,
    pub os_error: Option<i32>,
    pub resource_conflict: bool,
    pub unsupported_environment: bool,
}
impl From<Diagnostic> for PreparationError {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostic,
            os_error: None,
            resource_conflict: false,
            unsupported_environment: true,
        }
    }
}
fn preparation_failure(what: &str, error: std::io::Error) -> PreparationError {
    PreparationError {
        os_error: error.raw_os_error(),
        resource_conflict: error.kind() == std::io::ErrorKind::AddrInUse,
        unsupported_environment: error.kind() == std::io::ErrorKind::Unsupported,
        diagnostic: failure(what, error),
    }
}

/// Checks publication conflicts for a structured library start error. Pasta
/// still owns the real bind; successful probing does not reserve a port.
pub fn check_publications(policy: &Policy) -> Result<(), PreparationError> {
    // The host-side publication boundary can report the OS bind failure directly,
    // rather than classifying pasta's diagnostic prose as a resource conflict.
    for publication in &policy.network_publish {
        let address = match publication.family {
            IpFamily::Ipv4 => {
                std::net::SocketAddr::from(([127, 0, 0, 1], publication.host_port.get()))
            }
            IpFamily::Ipv6 => std::net::SocketAddr::from((
                std::net::Ipv6Addr::LOCALHOST,
                publication.host_port.get(),
            )),
        };
        match publication.protocol {
            kakoi_policy::network::Protocol::Tcp => {
                std::net::TcpListener::bind(address)
                    .map_err(|error| preparation_failure("bind publication", error))?;
            }
            kakoi_policy::network::Protocol::Udp => {
                std::net::UdpSocket::bind(address)
                    .map_err(|error| preparation_failure("bind publication", error))?;
            }
        }
    }
    Ok(())
}

/// Establishes and verifies communication independently of application launch.
/// Both CLI and retained-descriptor library launches use this same controller.
pub fn prepare_session(policy: &Policy, tools: &Tools) -> Result<Session, PreparationError> {
    let rules = filter_rules(&policy.network_allow)?;
    let (upstreams, following) = dns_upstreams(policy)?;
    let trust = if upstreams
        .first()
        .is_some_and(|first| first.tls_name().is_some())
    {
        Some(
            TlsClient::from_host()
                .map_err(|error| preparation_failure("load the host CA certificates", error))?,
        )
    } else {
        None
    };
    let config = DnsRuntimeConfig {
        policy: policy.network_allow.clone(),
        upstreams,
        limits: policy.network_limits.clone(),
        trust,
        nft: tools.nft.clone(),
        // Read before pasta starts, while this thread is still in the host's
        // network namespace.
        scope: AddressContext {
            host_addresses: host::addresses()
                .map_err(|error| preparation_failure("read the host's addresses", error))?,
            host_loopback_v4: Some(HOST_LOOPBACK_V4),
            host_loopback_v6: Some(HOST_LOOPBACK_V6),
            ..AddressContext::default()
        },
        generation: 0,
        host_dns: following,
    };
    let transport = Transport::start_closed(
        &tools.pasta,
        &tools.nft,
        &policy.network_publish,
        PASTA_STARTUP,
    )
    .map_err(|error| match pasta::early_exit(&error) {
        Some(deadline) => match outdated(&tools.pasta, deadline) {
            Some(diagnostic) => diagnostic.into(),
            None => preparation_failure("start pasta", error),
        },
        None => preparation_failure("start pasta", error),
    })?;
    let mut session = Session::prepare(transport, config, &rules, |_| None)
        .map_err(|error| preparation_failure("prepare the network policy", error))?;
    session
        .activate()
        .map_err(|error| preparation_failure("activate the network", error))?;
    for publication in &policy.network_publish {
        session.notice(&published(publication));
    }
    Ok(session)
}

/// The diagnostic for a pasta that ended during its start because it is too
/// old: one whose `--help`, run by `deadline`, does not list every long option
/// kakoi passes. `None` when the help cannot tell. Leaves pasta's own output
/// out, which only repeats its usage text.
fn outdated(executable: &Path, deadline: Instant) -> Option<Diagnostic> {
    let help = pasta::help(executable, deadline)?;
    let missing = pasta::missing_long_options(&help);
    if missing.is_empty() {
        return None;
    }
    Some(Diagnostic::bwrap(format!(
        "{} is too old for filtered network mode: it lacks {}; see {PASTA_GUIDE}",
        executable.display(),
        missing.join(", ")
    )))
}

/// Where a fixed publication is reachable on the host, and where it leads.
fn published(publication: &FixedPublication) -> String {
    let loopback = match publication.family {
        IpFamily::Ipv4 => "127.0.0.1",
        IpFamily::Ipv6 => "[::1]",
    };
    format!(
        "network published: {} {loopback}:{} -> sandbox {loopback}:{}",
        publication.protocol, publication.host_port, publication.port
    )
}

/// The single address that stands for the host's loopback of `family`.
fn host_loopback(family: IpFamily) -> IpNetwork {
    match family {
        IpFamily::Ipv4 => format!("{HOST_LOOPBACK_V4}/32"),
        IpFamily::Ipv6 => format!("{HOST_LOOPBACK_V6}/128"),
    }
    .parse()
    .expect("the host loopback address is a valid network")
}

fn unsupported(what: &str) -> Diagnostic {
    Diagnostic::bwrap(format!("filtered network mode does not support {what} yet"))
}

/// The static rules of the allow list: addresses and the host's loopback. DNS
/// names become permissions only as they are answered.
fn filter_rules(allows: &[Allow]) -> Result<Vec<FilterRule>, Diagnostic> {
    let mut rules = Vec::new();
    for allow in allows {
        match &allow.destination {
            Destination::Address {
                network,
                host_interface: None,
            } => rules.push(FilterRule {
                network: *network,
                protocol: allow.protocol,
                ports: allow.ports.clone(),
            }),
            Destination::Dns(_) => {}
            Destination::HostLoopback(family) => rules.push(FilterRule {
                network: host_loopback(*family),
                protocol: allow.protocol,
                ports: allow.ports.clone(),
            }),
            Destination::Address { .. } => {
                return Err(unsupported("`host-interface` destinations"))
            }
        }
    }
    Ok(rules)
}

/// The upstreams DNS names are resolved through: the policy's, or the host's DNS
/// configuration, which is then followed.
fn dns_upstreams(policy: &Policy) -> Result<(Vec<DnsUpstream>, Option<HostDns>), Diagnostic> {
    let names = policy
        .network_allow
        .iter()
        .any(|allow| matches!(allow.destination, Destination::Dns(_)));
    let mut following = None;
    let upstreams = if !policy.dns_upstream.is_empty() {
        policy.dns_upstream.clone()
    } else if names {
        let text = std::fs::read_to_string(host_dns::RESOLV_CONF)
            .map_err(|error| failure("read the host DNS configuration", error))?;
        let upstreams = host_dns::upstreams_from_resolv_conf(&text).map_err(Diagnostic::bwrap)?;
        following = Some(HostDns {
            path: host_dns::RESOLV_CONF.into(),
            parse: host_dns::upstreams_from_resolv_conf,
            text: Some(text),
            wait: host_dns::wait_for,
        });
        upstreams
    } else {
        // Without DNS names the managed resolver refuses every name by itself.
        Vec::new()
    };
    Ok((upstreams, following))
}

fn failure(what: &str, error: std::io::Error) -> Diagnostic {
    Diagnostic::bwrap(format!("{what}: {error}"))
}
