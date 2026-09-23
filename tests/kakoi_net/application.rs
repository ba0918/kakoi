use crate::common::{home_with_workspace, RW_WORKSPACE};
use kakoi_core::{
    layers::LayerSelection,
    planning::{plan_for, Request},
};
use kakoi_net::{application, namespace::NetworkNamespace};
use std::collections::BTreeMap;

// @kotowari[REQ-060, REQ-150]
#[test]
fn application_shares_the_controlled_network_without_its_authority_or_descriptors() {
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}\n[network]\nmode='filtered'\n"),
    );
    let namespace = NetworkNamespace::create().unwrap();
    assert!(namespace
        .command("/usr/sbin/nft")
        .unwrap()
        .args(["add", "table", "inet", "sentinel"])
        .status()
        .unwrap()
        .success());
    let net = std::fs::read_link(format!("/proc/{}/ns/net", namespace.keeper_pid())).unwrap();
    let mut host = BTreeMap::new();
    host.insert("HOME".into(), home.path().as_os_str().to_owned());
    host.insert(
        "XDG_CONFIG_HOME".into(),
        home.path().join(".config").into_os_string(),
    );
    host.insert("PATH".into(), "/usr/bin:/bin:/usr/sbin".into());
    let script = r#"
import os, socket, subprocess, sys
assert open('/etc/resolv.conf').read() == 'nameserver 127.0.0.53\n'
try:
    open('/etc/resolv.conf', 'w')
except OSError:
    pass
else:
    raise AssertionError('managed resolver configuration is writable')
assert os.readlink('/proc/self/ns/net') == sys.argv[1]
assert os.getuid() == int(sys.argv[2])
assert os.getgid() == int(sys.argv[3])
with open('/proc/self/status') as status:
    fields = dict(line.rstrip().split(':', 1) for line in status if ':' in line)
assert int(fields['CapEff'].strip(), 16) == 0
for fd in os.listdir('/proc/self/fd'):
    try:
        target = os.readlink('/proc/self/fd/' + fd)
    except FileNotFoundError:
        continue
    assert not any(part in target for part in ['net:[', 'user:[', 'pidfd', 'kakoi-'])
for command in [
    ['/usr/sbin/nft', 'flush', 'ruleset'],
    ['/usr/bin/unshare', '--user', '--map-root-user', '/usr/sbin/nft', 'flush', 'ruleset'],
]:
    assert subprocess.run(command, capture_output=True).returncode != 0
try:
    socket.socket(socket.AF_INET, socket.SOCK_RAW, socket.IPPROTO_RAW)
except PermissionError:
    pass
else:
    raise AssertionError('raw network authority leaked')
open('app-write', 'w').write('ok')
print('isolated')
"#;
    let plan = plan_for(&Request {
        layers: LayerSelection {
            profile: "default".into(),
            policy_file: None,
            rw: vec![],
            hide: vec![],
        },
        workspace: Some(workspace.clone()),
        command: vec![
            "/usr/bin/python3".into(),
            "-c".into(),
            script.into(),
            net.into_os_string(),
            unsafe { libc::getuid() }.to_string().into(),
            unsafe { libc::getgid() }.to_string().into(),
        ],
        current_dir: workspace.clone(),
        host,
    })
    .unwrap();
    let host_resolver = std::fs::read("/etc/resolv.conf").unwrap();
    let mut launch = application::prepare(&plan, &namespace).unwrap();
    let output = launch.command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"isolated\n");
    assert_eq!(std::fs::read("/etc/resolv.conf").unwrap(), host_resolver);
    assert_eq!(
        std::fs::read_to_string(workspace.join("app-write")).unwrap(),
        "ok"
    );
    assert!(namespace
        .command("/usr/sbin/nft")
        .unwrap()
        .args(["list", "table", "inet", "sentinel"])
        .output()
        .unwrap()
        .status
        .success());
}

// @kotowari[REQ-027, REQ-060, REQ-131]
#[test]
fn ordinary_application_name_resolution_uses_the_managed_dns_runtime() {
    use kakoi_core::{
        network::{Allow, Destination, NetworkLimits, Protocol},
        policy::parse_policy,
    };
    use kakoi_net::{
        dns_runtime::{DnsRuntime, DnsRuntimeConfig},
        filter, nft,
        scope::AddressContext,
    };
    use std::{
        net::UdpSocket,
        path::Path,
        process::Stdio,
        sync::Arc,
        time::{Duration, Instant},
    };
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}\n[network]\nmode='filtered'\n"),
    );
    let ns = Arc::new(NetworkNamespace::create().unwrap());
    assert!(ns
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    nft::apply(
        &ns,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let upstream = UdpSocket::bind("127.0.0.1:0").unwrap();
    upstream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let config = parse_policy(
        &format!(
            "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}",
            upstream.local_addr().unwrap().port()
        ),
        Path::new("dns.toml"),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        let mut bytes = [0; 512];
        let (size, peer) = upstream.recv_from(&mut bytes).unwrap();
        let mut answer = bytes[..size].to_vec();
        answer[2] |= 0x80;
        answer[7] = 1;
        answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 1, 1, 1, 1]);
        upstream.send_to(&answer, peer).unwrap();
    });
    let mut runtime = DnsRuntime::new(
        Arc::clone(&ns),
        DnsRuntimeConfig {
            policy: vec![Allow {
                destination: Destination::Dns("api.example.com".parse().unwrap()),
                protocol: Protocol::Tcp,
                ports: vec!["443".into()].try_into().unwrap(),
            }],
            upstreams: config.network.dns_upstream,
            limits: NetworkLimits::default(),
            trust: None,
            nft: "/usr/sbin/nft".into(),
            scope: AddressContext::default(),
            generation: 0,
            host_dns: None,
        },
        |_| None,
    )
    .unwrap();
    let mut host = BTreeMap::new();
    host.insert("HOME".into(), home.path().as_os_str().to_owned());
    host.insert(
        "XDG_CONFIG_HOME".into(),
        home.path().join(".config").into_os_string(),
    );
    host.insert("PATH".into(), "/usr/bin:/bin".into());
    let script = r#"
import socket
assert socket.gethostbyname('api.example.com') == '1.1.1.1'
try:
    socket.gethostbyname('unlisted.example.com')
except socket.gaierror:
    pass
else:
    raise AssertionError('unlisted DNS name resolved')
print('managed-dns')
"#;
    let plan = plan_for(&Request {
        layers: LayerSelection {
            profile: "default".into(),
            policy_file: None,
            rw: vec![],
            hide: vec![],
        },
        workspace: Some(workspace.clone()),
        command: vec!["/usr/bin/python3".into(), "-c".into(), script.into()],
        current_dir: workspace,
        host,
    })
    .unwrap();
    let mut launch = application::prepare(&plan, &ns).unwrap();
    let mut child = launch
        .command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        runtime.poll(Instant::now()).unwrap();
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"managed-dns\n");
    runtime.stop();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !runtime.is_finished() {
        runtime.poll(Instant::now()).unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    server.join().unwrap();
}
