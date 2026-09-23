"""Internal worker: network mutations require a private user/net/PID namespace."""
import fcntl
import json
import os
from pathlib import Path
import runpy
import select
import socket
import struct
import subprocess
import sys
import time
from guard import require_private_namespace

def run(*argv):
    subprocess.run(argv, check=True, timeout=8)

def loopback_options():
    mode = os.environ.get("KAKOI_PROBE_PASTA_LOOPBACK_MODE", "explicit")
    if mode == "explicit":
        return ["--host-lo-to-ns-lo"]
    if mode == "default":
        return []
    raise ValueError("unknown pasta loopback mode")

def tap():
    with open('/dev/net/tun', 'r+b', buffering=0) as device:
        request = struct.pack('16sH22x', b'probe-tap', 2 | 4096)
        fcntl.ioctl(device, 1074025674, request)
        run('ip', 'link', 'show', 'probe-tap')
    absent = subprocess.run(['ip', 'link', 'show', 'probe-tap'], capture_output=True).returncode != 0
    if not absent:
        raise AssertionError('TAP survived closing its descriptor')
    print(json.dumps({'tap_created': True, 'tap_removed_after_close': absent}), flush=True)

def serve(family, announce_namespace=False):
    require_private_namespace()
    af, address = (socket.AF_INET, '127.0.0.1') if family == 'ipv4' else (socket.AF_INET6, '::1')
    tcp = socket.socket(af, socket.SOCK_STREAM)
    udp = socket.socket(af, socket.SOCK_DGRAM)
    for s in (tcp, udp):
        if af == socket.AF_INET6:
            s.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
        s.bind((address, 8080))
    tcp.listen()
    print(json.dumps({'netns': os.readlink('/proc/self/ns/net')}) if announce_namespace else 'ready', flush=True)
    while True:
        for s in select.select([tcp, udp], [], [], 1)[0]:
            if s is tcp:
                conn, _ = tcp.accept()
                with conn:
                    conn.settimeout(2)
                    conn.sendall(b'tcp-ok')
            else:
                _, peer = udp.recvfrom(64)
                udp.sendto(b'udp-ok', peer)

def publication(pasta, family):
    run('ip', 'link', 'set', 'lo', 'up')
    run('ip', 'link', 'add', 'probe0', 'type', 'dummy')
    run('ip', 'link', 'set', 'probe0', 'up')
    if family == 'ipv4':
        address, outer, af, flag = ('127.0.0.1', '198.18.0.1', socket.AF_INET, '-4')
        run('ip', 'addr', 'add', outer + '/24', 'dev', 'probe0')
        run('ip', 'route', 'add', 'default', 'dev', 'probe0')
    else:
        address, outer, af, flag = ('::1', '2001:db8::1', socket.AF_INET6, '-6')
        run('ip', '-6', 'addr', 'add', outer + '/64', 'dev', 'probe0', 'nodad')
        run('ip', '-6', 'addr', 'add', 'fe80::1/64', 'dev', 'probe0', 'nodad')
        run('ip', '-6', 'route', 'add', 'default', 'dev', 'probe0')
    mapping = address + '/18080:8080'
    argv = [pasta, '-f', flag, '--config-net', *loopback_options(), '-i', 'probe0', '-t', mapping, '-u', mapping, '-T', 'none', '-U', 'none', '--', sys.executable, str(Path(__file__).resolve()), 'serve', family]
    print(json.dumps({'pasta_argv': argv}), flush=True)
    process = subprocess.Popen(argv, stdout=subprocess.PIPE, text=True)
    try:
        if not select.select([process.stdout], [], [], 10)[0]:
            raise AssertionError('inner service readiness timeout')
        if not process.stdout.readline().strip() == 'ready':
            raise AssertionError('pasta exited before inner service started')
        with socket.socket(af, socket.SOCK_STREAM) as tcp:
            tcp.settimeout(2)
            tcp.connect((address, 18080))
            if not tcp.recv(32) == b'tcp-ok':
                raise AssertionError("Unexpected probe observation: tcp.recv(32) == b'tcp-ok'")
        with socket.socket(af, socket.SOCK_DGRAM) as udp:
            udp.settimeout(2)
            udp.sendto(b'probe', (address, 18080))
            if not udp.recv(32) == b'udp-ok':
                raise AssertionError("Unexpected probe observation: udp.recv(32) == b'udp-ok'")
        for kind in (socket.SOCK_STREAM, socket.SOCK_DGRAM):
            with socket.socket(af, kind) as client:
                client.settimeout(0.3)
                try:
                    client.connect((outer, 18080))
                    if kind == socket.SOCK_DGRAM:
                        client.send(b'probe')
                    client.recv(32)
                except (TimeoutError, ConnectionRefusedError):
                    pass
                else:
                    raise AssertionError('published on a non-loopback address')
        print(json.dumps({'family': family, 'tcp': True, 'udp': True, 'nonloopback_rejected': True, 'scope': 'E1 static publication subset'}), flush=True)
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
if __name__ == '__main__':
    require_private_namespace()
    name = sys.argv[1]
    if name == 'link-local':
        from link_local import probe
        probe(sys.argv[2])
    elif name.startswith('host-loopback-'):
        from host_loopback import probe
        probe(sys.argv[2], name.removeprefix('host-loopback-'))
    elif name.startswith('dual-guard-'):
        from dual_guard import probe
        probe(sys.argv[2], name.removeprefix('dual-guard-'))
    elif name.startswith('fixed-'):
        from fixed import probe
        _, host, target = name.split('-')
        probe(sys.argv[2], target if host == 'conflict' else host, target, conflict=host == 'conflict')
    elif name == 'dns-proxy':
        from dns_proxy import probe
        probe()
    elif name == 'tap':
        tap()
    elif name == 'control-isolation':
        from control_isolation import probe
        probe()
    elif name in ('process-parent-death', 'process-main-exit'):
        from process_guard import probe
        probe(name.removeprefix('process-'))
    elif name in ('process-ctrl-c', 'process-ctrl-c-default'):
        from terminal import probe
        probe(default=name == 'process-ctrl-c-default')
    elif name in ('process-interrupt-grace', 'process-repeat-term', 'process-safety-exit', 'process-network-order'):
        from shutdown import probe
        probe(name.removeprefix('process-'))
    elif name in ('dynamic-control-ipv4', 'dynamic-control-ipv6'):
        from dynamic import probe
        probe(sys.argv[2], name.removeprefix('dynamic-control-'), control_check=True)
    elif name in ('dynamic-conflict-ipv4', 'dynamic-conflict-ipv6'):
        from dynamic import probe
        probe(sys.argv[2], name.removeprefix('dynamic-conflict-'), conflict=True)
    elif name in ('lease-health-push-ipv4', 'lease-health-push-ipv6'):
        from health_push import probe
        probe(name.removeprefix('lease-health-push-'))
    elif name in ('lease-heartbeat-stop', 'lease-heartbeat-kill'):
        from lease_failure import probe
        probe(name.removeprefix('lease-heartbeat-'), liveness=True)
    elif name in ('lease-stop', 'lease-kill'):
        from lease_failure import probe
        probe(name.removeprefix('lease-'))
    elif name in ('lease-recovery-ipv4', 'lease-recovery-ipv6'):
        from lease_recovery import probe
        probe(name.removeprefix('lease-recovery-'))
    elif name in ('lease-stage-ipv4', 'lease-stage-ipv6'):
        from lease_stage import probe
        probe(name.removeprefix('lease-stage-'))
    elif name in ('lease-preserve-ipv4', 'lease-preserve-ipv6'):
        from lease_recovery import probe
        probe(name.removeprefix('lease-preserve-'), preserve_kernel=True)
    elif name in ('recovery-ipv4', 'recovery-ipv6'):
        from recovery import probe
        probe(sys.argv[2], name.removeprefix('recovery-'))
    elif name in ('resume-ipv4', 'resume-ipv6'):
        from recovery import probe
        probe(sys.argv[2], name.removeprefix('resume-'), watchdog_fault='kill', automatic=True)
    elif name in ('health-push-ipv4', 'health-push-ipv6', 'recovery-push-ipv4', 'recovery-push-ipv6'):
        from recovery import probe
        backend, _, family = name.split('-')
        probe(sys.argv[2], family, kernel_fault='kill' if backend == 'health' else None, push=True)
    elif name.startswith('health-'):
        from recovery import probe
        _, fault, family = name.split('-')
        probe(sys.argv[2], family, kernel_fault=fault)
    elif name.startswith('watchdog-'):
        from recovery import probe
        _, fault, family = name.split('-')
        probe(sys.argv[2], family, watchdog_fault=fault)
    elif name == 'lifetime':
        runpy.run_path(str(Path(__file__).with_name('flow_lifetime.py')), run_name='__main__')
    elif name == 'serve':
        serve(sys.argv[2])
    elif name in ('dynamic-ipv4', 'dynamic-ipv6'):
        from dynamic import probe
        probe(sys.argv[2], name.removeprefix('dynamic-'))
    elif name in ('dynamic-udp-ipv4', 'dynamic-udp-ipv6'):
        from dynamic import probe
        probe(sys.argv[2], name.removeprefix('dynamic-udp-'), udp_reassign=True)
    elif name in ('dynamic-udp-expiry-ipv4', 'dynamic-udp-expiry-ipv6'):
        from dynamic import probe
        probe(sys.argv[2], name.removeprefix('dynamic-udp-expiry-'), udp_reassign=True, udp_expiry=True)
    elif name in ('dynamic-udp-churn-ipv4', 'dynamic-udp-churn-ipv6'):
        from dynamic import probe
        probe(sys.argv[2], name.removeprefix('dynamic-udp-churn-'), udp_churn=True)
    elif name in ('boundary-ipv4', 'boundary-ipv6'):
        from boundary import probe
        probe(sys.argv[2], name.removeprefix('boundary-'))
    elif name in ('hooks-ipv4', 'hooks-ipv6'):
        from hooks import probe
        probe(sys.argv[2], name.removeprefix('hooks-'))
    elif name in ('pasta-ipv4', 'pasta-ipv6'):
        publication(sys.argv[2], name.removeprefix('pasta-'))
    else:
        raise SystemExit('unknown probe')
