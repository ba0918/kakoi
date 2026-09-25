//! Starting a filtered plan: locate the tools, prepare and verify the network
//! enforcement, then start the application inside it.

use crate::{
    dns_runtime::{DnsRuntimeConfig, HostDns},
    dns_transport::TlsClient,
    filter::FilterRule,
    host::{self, HOST_LOOPBACK_V4, HOST_LOOPBACK_V6},
    host_dns,
    scope::AddressContext,
    session::Session,
    supervisor::Application,
    transport::Transport,
};
use kakoi_core::{
    diagnostic::Diagnostic,
    layers::Policy,
    network::{Allow, Destination, DnsUpstream, FixedPublication, IpFamily, IpNetwork},
    plan::Plan,
    planning::locate_command,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
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
    let policy = &plan.policy;
    let rules = filter_rules(&policy.network_allow)?;
    let (upstreams, following) = dns_upstreams(policy)?;
    let trust = if upstreams
        .first()
        .is_some_and(|first| first.tls_name().is_some())
    {
        Some(
            TlsClient::from_host()
                .map_err(|error| failure("load the host CA certificates", error))?,
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
                .map_err(|error| failure("read the host's addresses", error))?,
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
    .map_err(|error| failure("start pasta", error))?;
    let mut session = Session::prepare(transport, config, &rules, |_| None)
        .map_err(|error| failure("prepare the network policy", error))?;
    session
        .activate()
        .map_err(|error| failure("activate the network", error))?;
    for publication in &policy.network_publish {
        session.notice(&published(publication));
    }
    let application = Application::spawn(
        plan,
        session.namespace(),
        init,
        Duration::from_secs(policy.shutdown_grace_seconds.into()),
        stdout,
    )?;
    Ok((session, application))
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
