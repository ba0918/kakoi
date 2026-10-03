//! Same-binary policy transport. Decoding always returns to common validation.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use kakoi_policy::layers::{Layer, LayerOrigin};
use kakoi_policy::policy::{Env, Git, Mounts, Network, PolicyFile, Process};
use kakoi_policy::{Commands, GuardRule, HideMounts, Policy, PolicyPath, Scan};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize)]
pub(crate) enum PathInput {
    Absolute(Vec<u8>),
    Expression(String),
}

impl PathInput {
    fn encode(path: &PolicyPath) -> Self {
        match path {
            PolicyPath::Absolute(path) => Self::Absolute(path.as_os_str().as_bytes().to_vec()),
            _ => Self::Expression(path.to_string()),
        }
    }
    fn decode(self) -> Result<PolicyPath, String> {
        match self {
            Self::Absolute(bytes) => {
                if bytes.contains(&0) {
                    return Err("NUL in policy path".into());
                }
                Ok(PolicyPath::Absolute(PathBuf::from(OsString::from_vec(
                    bytes,
                ))))
            }
            Self::Expression(text) => text.try_into(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) enum Origin {
    Profile(Vec<u8>),
    PolicyFile(Vec<u8>),
    BuiltInDefault,
    CommandLine,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ScanInput {
    root: PathInput,
    names: Vec<String>,
    exclude: Vec<String>,
    prune: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct HideInput {
    under: PathInput,
    fstype: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct LayerInput {
    origin: Origin,
    mounts_mode: Option<String>,
    system: Option<bool>,
    rw: Vec<PathInput>,
    rw_file: Vec<PathInput>,
    rw_copy: Vec<PathInput>,
    ro: Vec<PathInput>,
    hide: Vec<PathInput>,
    scan: Vec<ScanInput>,
    hide_mounts: Vec<HideInput>,
    network: Value,
    shutdown_grace_seconds: Option<u32>,
    env_mode: Option<String>,
    pass: Vec<String>,
    set: BTreeMap<String, String>,
    unset: Vec<String>,
    path_prepend: Vec<PathInput>,
    secrets: BTreeMap<String, PathInput>,
    instead_of: BTreeMap<String, String>,
    commands_mode: Option<String>,
    commands_allow: Vec<PathInput>,
    guards: Vec<GuardRule>,
}

fn encode_paths(paths: &[PolicyPath]) -> Vec<PathInput> {
    paths.iter().map(PathInput::encode).collect()
}
fn decode_paths(paths: Vec<PathInput>) -> Result<Vec<PolicyPath>, String> {
    paths.into_iter().map(PathInput::decode).collect()
}

pub(crate) fn encode(policy: &Policy) -> Vec<LayerInput> {
    policy
        .source_layers()
        .iter()
        .map(|layer| {
            let file = &layer.policy;
            let mounts = &file.mounts;
            let env = &file.env;
            LayerInput {
                origin: match &layer.origin {
                    LayerOrigin::Profile(path) => {
                        Origin::Profile(path.as_os_str().as_bytes().to_vec())
                    }
                    LayerOrigin::PolicyFile(path) => {
                        Origin::PolicyFile(path.as_os_str().as_bytes().to_vec())
                    }
                    LayerOrigin::BuiltInDefault => Origin::BuiltInDefault,
                    LayerOrigin::CommandLine => Origin::CommandLine,
                },
                mounts_mode: mounts.mode.map(|mode| mode.name().into()),
                system: mounts.system,
                rw: encode_paths(&mounts.rw),
                rw_file: encode_paths(&mounts.rw_file),
                rw_copy: encode_paths(&mounts.rw_copy),
                ro: encode_paths(&mounts.ro),
                hide: encode_paths(&mounts.hide),
                scan: mounts
                    .scan
                    .iter()
                    .map(|scan| ScanInput {
                        root: PathInput::encode(&scan.root),
                        names: scan.names.clone(),
                        exclude: scan.exclude.clone(),
                        prune: scan.prune.clone(),
                    })
                    .collect(),
                hide_mounts: mounts
                    .hide_mounts
                    .iter()
                    .map(|hide| HideInput {
                        under: PathInput::encode(&hide.under),
                        fstype: hide.fstype.clone(),
                    })
                    .collect(),
                network: encode_network(&file.network),
                shutdown_grace_seconds: file.process.shutdown_grace_seconds,
                env_mode: env.mode.map(|mode| mode.name().into()),
                pass: env.pass.clone(),
                set: env.set.clone(),
                unset: env.unset.clone(),
                path_prepend: encode_paths(&env.path_prepend),
                secrets: file
                    .secrets
                    .iter()
                    .map(|(name, path)| (name.clone(), PathInput::encode(path)))
                    .collect(),
                instead_of: file.git.instead_of.clone(),
                commands_mode: file.commands.mode.map(|mode| mode.name().into()),
                commands_allow: encode_paths(&file.commands.allow),
                guards: file.commands.guard.clone(),
            }
        })
        .collect()
}

fn mode<T: serde::de::DeserializeOwned>(value: Option<String>) -> Result<Option<T>, String> {
    value
        .map(|value| {
            serde_json::from_value(Value::String(value)).map_err(|error| error.to_string())
        })
        .transpose()
}

pub(crate) fn decode(layers: Vec<LayerInput>) -> Result<Policy, String> {
    let layers = layers
        .into_iter()
        .map(|input| -> Result<Layer, String> {
            let origin_path = |bytes: Vec<u8>| -> Result<PathBuf, String> {
                if bytes.contains(&0) {
                    return Err("NUL in policy source".into());
                }
                Ok(PathBuf::from(OsString::from_vec(bytes)))
            };
            let origin = match input.origin {
                Origin::Profile(path) => LayerOrigin::Profile(origin_path(path)?),
                Origin::PolicyFile(path) => LayerOrigin::PolicyFile(origin_path(path)?),
                Origin::BuiltInDefault => LayerOrigin::BuiltInDefault,
                Origin::CommandLine => LayerOrigin::CommandLine,
            };
            let mounts = Mounts {
                mode: mode(input.mounts_mode)?,
                system: input.system,
                rw: decode_paths(input.rw)?,
                rw_file: decode_paths(input.rw_file)?,
                rw_copy: decode_paths(input.rw_copy)?,
                ro: decode_paths(input.ro)?,
                hide: decode_paths(input.hide)?,
                scan: input
                    .scan
                    .into_iter()
                    .map(|scan| {
                        Ok(Scan {
                            root: scan.root.decode()?,
                            names: scan.names,
                            exclude: scan.exclude,
                            prune: scan.prune,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                hide_mounts: input
                    .hide_mounts
                    .into_iter()
                    .map(|hide| {
                        Ok(HideMounts {
                            under: hide.under.decode()?,
                            fstype: hide.fstype,
                        })
                    })
                    .collect::<Result<_, String>>()?,
            };
            Ok(Layer {
                origin,
                policy: PolicyFile {
                    mounts,
                    network: serde_json::from_value(input.network)
                        .map_err(|error| error.to_string())?,
                    process: Process {
                        shutdown_grace_seconds: input.shutdown_grace_seconds,
                    },
                    env: Env {
                        mode: mode(input.env_mode)?,
                        pass: input.pass,
                        set: input.set,
                        unset: input.unset,
                        path_prepend: decode_paths(input.path_prepend)?,
                    },
                    secrets: input
                        .secrets
                        .into_iter()
                        .map(|(name, path)| Ok((name, path.decode()?)))
                        .collect::<Result<_, String>>()?,
                    git: Git {
                        instead_of: input.instead_of,
                    },
                    commands: Commands {
                        mode: mode(input.commands_mode)?,
                        allow: decode_paths(input.commands_allow)?,
                        guard: input.guards,
                    },
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Policy::from_layers(&layers).map_err(|error| error.to_string())
}

fn encode_network(network: &Network) -> Value {
    // The existing network Serialize forms describe observations, not TOML input.
    // Encode its input schema explicitly and decode with the existing validators.
    let mut value = serde_json::to_value(&network.limits).expect("integer limits serialize");
    let fields = value.as_object_mut().expect("limits are an object");
    fields.insert("mode".into(), json!(network.mode.map(|mode| mode.name())));
    fields.insert(
        "allow-nested-filtered".into(),
        json!(network.allow_nested_filtered),
    );
    if network.settings_present() {
        fields.insert("publish".into(), Value::Array(network.publish.iter().map(|entry| json!({ "mode": "fixed", "protocol": entry.protocol, "port": entry.port, "host-port": entry.host_port, "target-family": entry.family, "host-family": entry.family })).collect()));
        fields.insert("allow".into(), Value::Array(network.allow.iter().map(|entry| {
            let destination = match &entry.destination {
                kakoi_policy::Destination::Dns(pattern) => json!({"dns": pattern.to_string()}),
                kakoi_policy::Destination::Address { network, host_interface } => json!({"cidr": network.to_string(), "host-interface": host_interface}),
                kakoi_policy::Destination::HostLoopback(family) => json!({"host-loopback": family}),
            };
            json!({"destination": destination, "protocol": entry.protocol, "ports": entry.ports.to_string().split(',').map(str::trim).collect::<Vec<_>>()})
        }).collect()));
        fields.insert("dns-upstream".into(), Value::Array(network.dns_upstream.iter().map(|upstream| json!({"transport": if upstream.tls_name().is_some() { "tls" } else { "plain" }, "ip": upstream.address.to_string(), "port": upstream.port(), "tls-name": upstream.tls_name()})).collect()));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_preserves_every_policy_input_and_non_utf8_paths() {
        let policy = Policy::from_toml(
            r#"
[mounts]
mode = 'listed'
system = false
rw = ['${workspace}']
rw-file = ['~/.state']
rw-copy = ['/copy']
ro = ['/readonly']
hide = ['/hidden']
[[mounts.scan]]
root = '${worktree}'
names = ['.env']
exclude = ['*.example']
prune = ['.git']
[[mounts.hide-mounts]]
under = '/mnt'
fstype = ['9p']
[network]
mode = 'filtered'
allow-nested-filtered = true
udp-idle-timeout-seconds = 60
dns-zero-ttl-grace-milliseconds = 2000
dns-server-timeout-seconds = 3
dns-resolution-timeout-seconds = 20
dns-max-cname-hops = 8
dns-max-upstream-queries = 32
dns-max-concurrent-resolutions = 128
dns-max-waiters-per-resolution = 32
dns-failure-cache-seconds = 8
recovery-attempt-timeout-seconds = 15
[[network.allow]]
destination = { dns = '*.example.com' }
protocol = 'tcp'
ports = ['443']
[[network.allow]]
destination = { cidr = '127.0.0.1/32' }
protocol = 'udp'
ports = ['53']
[[network.allow]]
destination = { host-loopback = 'ipv6' }
protocol = 'tcp'
ports = ['8000-8010', '8443']
[[network.publish]]
mode = 'fixed'
protocol = 'tcp'
port = 8080
host-port = 18080
target-family = 'ipv4'
host-family = 'ipv4'
[[network.dns-upstream]]
transport = 'tls'
ip = '1.1.1.1'
port = 853
tls-name = 'cloudflare-dns.com'
[process]
shutdown-grace-seconds = 3
[env]
mode = 'clear'
pass = ['HOME']
set = { KEY = 'value' }
unset = ['*_TOKEN']
path-prepend = ['/tools']
[secrets]
SECRET = '${config_dir}/secrets/key'
[git.instead-of]
'git@example.com:' = 'https://example.com/'
[commands]
mode = 'listed'
allow = ['/bin/sh']
[[commands.guard]]
program = 'git'
deny = [['push']]
deny-flags = ['--force']
reason = 'human operation'
"#,
        )
        .unwrap();
        let mut layers = policy.source_layers().to_vec();
        layers[0]
            .policy
            .mounts
            .ro
            .push(PolicyPath::Absolute(PathBuf::from(OsString::from_vec(
                b"/non-utf8-\xff".to_vec(),
            ))));
        let policy = Policy::from_layers(&layers).unwrap();
        let message = serde_json::to_vec(&encode(&policy)).unwrap();
        let decoded = decode(serde_json::from_slice(&message).unwrap()).unwrap();
        assert_eq!(decoded, policy);
        assert_eq!(decoded.source_layers(), policy.source_layers());
    }
}
