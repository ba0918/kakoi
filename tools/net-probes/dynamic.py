"""Measure publication changes while a TCP connection remains open."""
import errno
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time

from guard import require_private_namespace
from worker import loopback_options, run


def endpoint(family):
    return (socket.AF_INET, '127.0.0.1') if family == 'ipv4' else (socket.AF_INET6, '::1')


def emit(**values):
    print(json.dumps(values), flush=True)


def new_socket(af, kind):
    sock = socket.socket(af, kind)
    sock.settimeout(0.5)
    if af == socket.AF_INET6:
        sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
    return sock


def serve(family, address=None, port=8080, announce_namespace=False):
    require_private_namespace()
    af, loopback = endpoint(family)
    address = address or loopback
    tcp, udp = new_socket(af, socket.SOCK_STREAM), new_socket(af, socket.SOCK_DGRAM)
    tcp.bind((address, port))
    udp.bind((address, port))
    tcp.listen()
    readers = [tcp, udp]
    if announce_namespace:
        emit(netns=os.readlink('/proc/self/ns/net'))
    else:
        print('ready', flush=True)
    while True:
        for sock in select.select(readers, [], [], 1)[0]:
            if sock is tcp:
                conn, _ = tcp.accept()
                conn.settimeout(0.5)
                readers.append(conn)
            elif sock is udp:
                data, peer = udp.recvfrom(1024)
                udp.sendto(data, peer)
            else:
                try:
                    data = sock.recv(1024)
                    if data:
                        sock.sendall(data)
                except (ConnectionResetError, BrokenPipeError, TimeoutError):
                    data = b''
                if not data:
                    readers.remove(sock)
                    sock.close()


def echo(sock, token):
    sock.sendall(token)
    result = b''
    while len(result) < len(token):
        data = sock.recv(1024)
        if not data:
            return False
        result += data
    return result == token


def reachable(af, address, kind, port=18080, connect_only=False):
    with new_socket(af, kind) as sock:
        try:
            sock.connect((address, port))
            if connect_only and kind == socket.SOCK_STREAM:
                return True
            return echo(sock, os.urandom(16))
        except (TimeoutError, ConnectionRefusedError, ConnectionResetError):
            return False


def check_new(af, address, expected, phase):
    observed = {name: reachable(af, address, kind, connect_only=not expected) for name, kind in
                (('tcp', socket.SOCK_STREAM), ('udp', socket.SOCK_DGRAM))}
    emit(phase=phase, new_connections=observed, expected=expected)
    if any(value != expected for value in observed.values()):
        raise AssertionError('new connection result differs from expectation: ' + phase)


def configure(pesto, path, operation, mapping):
    result = subprocess.run([pesto, operation, '-t', mapping, '-u', mapping, path],
                            capture_output=True, text=True, timeout=3)
    emit(pesto_operation=operation, exit=result.returncode,
         stdout=result.stdout, stderr=result.stderr)
    result.check_returncode()
    # pesto exits after sending; a second transaction waits for the prior apply.
    state = subprocess.run([pesto, path], capture_output=True, text=True, timeout=3)
    emit(pesto_state_exit=state.returncode, stdout=state.stdout, stderr=state.stderr)
    state.check_returncode()


def probe(pasta, family, udp_reassign=False, udp_expiry=False, udp_churn=False, control_check=False, conflict=False):
    require_private_namespace()
    original_mount = os.environ.get('KAKOI_PROBE_HOST_MNT')
    if not original_mount or original_mount == os.readlink('/proc/self/ns/mnt'):
        raise SystemExit('Refusing tmpfs outside private mount namespace')
    # A host temporary socket would survive a forced stop; private tmpfs does not.
    run('mount', '--make-rprivate', '/')
    run('mount', '-t', 'tmpfs', '-o', 'size=1m,mode=0700', 'tmpfs', '/tmp')
    control = '/tmp/kakoi-probe.sock'
    pesto = os.environ['KAKOI_PROBE_PESTO']
    if udp_expiry:
        for name in ('nf_conntrack_udp_timeout', 'nf_conntrack_udp_timeout_stream'):
            setting = Path('/proc/sys/net/netfilter') / name
            setting.write_text('10\n')
            value = int(setting.read_text())
            emit(private_udp_setting=name, seconds=value)
            if value != 10:
                raise AssertionError('private UDP timeout not applied')
    af, address = endpoint(family)
    run('ip', 'link', 'set', 'lo', 'up')
    run('ip', 'link', 'add', 'probe0', 'type', 'dummy')
    run('ip', 'link', 'set', 'probe0', 'up')
    if family == 'ipv4':
        outer, flag = '198.18.0.1', '-4'
        run('ip', 'addr', 'add', outer + '/24', 'dev', 'probe0')
        run('ip', 'route', 'add', 'default', 'dev', 'probe0')
    else:
        outer, flag = '2001:db8::1', '-6'
        run('ip', '-6', 'addr', 'add', outer + '/64', 'dev', 'probe0', 'nodad')
        run('ip', '-6', 'addr', 'add', 'fe80::1/64', 'dev', 'probe0', 'nodad')
        run('ip', '-6', 'route', 'add', 'default', 'dev', 'probe0')
    fixture = Path(__file__).with_name('udp_reassign.py') if udp_reassign else Path(__file__)
    if udp_churn or conflict:
        fixture = Path(__file__).with_name('udp_churn.py')
    if control_check:
        fixture = Path(__file__).with_name('control_pasta.py')
    argv = [pasta, '-f', flag, '--config-net', *loopback_options(), '-i', 'probe0',
            '-c', control, '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none',
            '--', sys.executable, str(fixture.resolve()), 'serve', family]
    emit(pasta_argv=argv)
    process = subprocess.Popen(argv, stdout=subprocess.PIPE,
                               stdin=subprocess.PIPE if udp_reassign or control_check else None, text=True)
    try:
        if not select.select([process.stdout], [], [], 10)[0] or process.stdout.readline().strip() != 'ready':
            raise AssertionError('pasta/inner service failed to become ready')
        deadline = time.monotonic() + 2
        while not Path(control).exists():
            if time.monotonic() >= deadline:
                raise AssertionError('configuration socket readiness timeout')
            time.sleep(0.02)
        if conflict:
            from publish_conflict import exercise
            exercise(process, pesto, control, family)
            return
        if control_check:
            from control_pasta import exercise
            exercise(process, pesto, control, family)
            return
        if udp_churn:
            from udp_churn import exercise
            exercise(pesto, control, family)
            return
        if udp_reassign:
            from udp_reassign import exercise
            exercise(process, pesto, control, family, expiry=udp_expiry)
            return
        mapping = address + '/18080:8080'
        check_new(af, address, False, 'initial')
        configure(pesto, control, '-A', mapping)
        check_new(af, address, True, 'added')
        check_new(af, outer, False, 'nonloopback')
        with new_socket(af, socket.SOCK_STREAM) as existing:
            existing.connect((address, 18080))
            if not echo(existing, b'before-removal'):
                raise AssertionError('persistent TCP fixture failed')
            configure(pesto, control, '-D', mapping)
            check_new(af, address, False, 'removed')
            survived = echo(existing, b'after-removal')
            emit(existing_tcp_after_removal=survived)
            if not survived:
                raise AssertionError('removing publication broke established TCP')
            # TCP may retain established tuples; normal listeners use SO_REUSEADDR.
            udp_rebound = False
            with new_socket(af, socket.SOCK_STREAM) as tcp, new_socket(af, socket.SOCK_DGRAM) as udp:
                tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                tcp.bind((address, 18080))
                tcp.listen()
                try:
                    udp.bind((address, 18080))
                    udp_rebound = True
                except OSError as error:
                    if error.errno != errno.EADDRINUSE:
                        raise
                    table = Path('/proc/net/udp' if af == socket.AF_INET else '/proc/net/udp6')
                    rows = [line for line in table.read_text().splitlines()[1:]
                            if line.split()[1].endswith(':46A0')]
                    emit(udp_rebind_errno=error.errno, remaining_udp_port_18080=rows)
                    # Reuse is diagnostic only: ordinary UDP bind must still pass.
                    with new_socket(af, socket.SOCK_DGRAM) as reuse:
                        reuse.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                        try:
                            reuse.bind((address, 18080))
                            emit(udp_rebind_with_reuseaddr=True)
                        except OSError as reuse_error:
                            if reuse_error.errno != errno.EADDRINUSE:
                                raise
                            emit(udp_rebind_with_reuseaddr=False)
                emit(released_port_rebound={'tcp': True, 'udp': udp_rebound})
            configure(pesto, control, '-A', mapping)
            check_new(af, address, True, 'readded')
            survived = echo(existing, b'after-readd')
            emit(existing_tcp_after_readd=survived)
            if not survived:
                raise AssertionError('readding publication broke established TCP')
        configure(pesto, control, '-D', mapping)
        check_new(af, address, False, 'final-removal')
        if not udp_rebound:
            raise AssertionError('UDP publication removed but ordinary bind still occupied')
        emit(family=family, scope='E1 dynamic publication subset; no listener detection or app control isolation')
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


if __name__ == '__main__':
    require_private_namespace()
    if len(sys.argv) == 3 and sys.argv[1] == 'serve':
        serve(sys.argv[2])
    else:
        raise SystemExit('Use run.py to execute probes')
