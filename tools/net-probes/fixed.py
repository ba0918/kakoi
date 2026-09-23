"""Initial-release fixed mappings: lifecycle and conflicts without pesto."""
from contextlib import ExitStack
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys

from dynamic import emit, endpoint, new_socket
from guard import require_private_namespace
from lease_failure import ready, stop_process
from publish_conflict import expect_listeners, host_owner
from udp_reassign import sample
from worker import run

HERE = str(Path(__file__).resolve())


def serve(family, epoch):
    af, address = endpoint(family)
    with ExitStack() as stack:
        tcp = stack.enter_context(new_socket(af, socket.SOCK_STREAM))
        udp = stack.enter_context(new_socket(af, socket.SOCK_DGRAM))
        tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        for sock in (tcp, udp):
            sock.bind((address, 8080))
        tcp.listen()
        readers = [tcp, udp]
        print('ready', flush=True)
        while True:
            for sock in select.select(readers, [], [], 1)[0]:
                if sock is tcp:
                    conn, _ = tcp.accept()
                    conn.settimeout(.5)
                    stack.enter_context(conn)
                    readers.append(conn)
                elif sock is udp:
                    data, peer = sock.recvfrom(1024)
                    sock.sendto(epoch.encode() + b':' + data, peer)
                else:
                    try:
                        data = sock.recv(1024)
                        if data:
                            sock.sendall(epoch.encode() + b':' + data)
                    except (ConnectionResetError, BrokenPipeError, TimeoutError):
                        data = b''
                    if not data:
                        readers.remove(sock)
                        sock.close()


def inner(family):
    server = None
    emit(netns=os.readlink('/proc/self/ns/net'), service_started=False)
    try:
        for line in sys.stdin:
            command = line.strip()
            if command in ('A', 'B'):
                if server is not None:
                    raise AssertionError('service already started')
                server = subprocess.Popen([sys.executable, HERE, 'serve', family, command],
                                          stdout=subprocess.PIPE, text=True)
                ready(server)
                print('started', flush=True)
            elif command == 'stop':
                stop_process(server)
                server = None
                print('stopped', flush=True)
            elif command == 'exit':
                return
            else:
                raise AssertionError('unexpected control command')
    finally:
        stop_process(server)


def command(process, text, reply):
    process.stdin.write(text + '\n')
    process.stdin.flush()
    if not select.select([process.stdout], [], [], 3)[0] or process.stdout.readline().strip() != reply:
        raise AssertionError('fixed fixture command failed: ' + text)


def setup():
    run('ip', 'link', 'set', 'lo', 'up')
    run('ip', 'link', 'add', 'probe0', 'type', 'dummy')
    run('ip', 'link', 'set', 'probe0', 'up')
    run('ip', '-4', 'addr', 'add', '198.18.0.1/24', 'dev', 'probe0')
    run('ip', '-6', 'addr', 'add', '2001:db8::1/64', 'dev', 'probe0', 'nodad')
    run('ip', '-6', 'addr', 'add', 'fe80::1/64', 'dev', 'probe0', 'nodad')
    run('ip', '-4', 'route', 'add', 'default', 'via', '198.18.0.254', 'dev', 'probe0')
    run('ip', '-6', 'route', 'add', 'default', 'via', '2001:db8::fe', 'dev', 'probe0')


def launch(pasta, host_family, target_family):
    _, host = endpoint(host_family)
    _, target = endpoint(target_family)
    mapping = host + ('/18080:8080' if host_family == target_family else '/18080:' + target + '/8080')
    argv = [pasta, '-f', '--config-net', '--host-lo-to-ns-lo', '-i', 'probe0',
            '-t', mapping, '-u', mapping, '-T', 'none', '-U', 'none',
            '--', sys.executable, HERE, 'inner', target_family]
    emit(pasta_argv=argv)
    return subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, env={**os.environ, 'LC_ALL': 'C'})


def finish(process):
    if process.poll() is None:
        try:
            process.stdin.write('exit\n')
            process.stdin.flush()
            process.wait(timeout=3)
        except (BrokenPipeError, subprocess.TimeoutExpired):
            stop_process(process)
    else:
        process.wait()
    diagnostic = process.stderr.read()
    if diagnostic:
        emit(pasta_stderr=diagnostic)
    process.stderr.close()
    process.stdin.close()
    process.stdout.close()


def initial_ready(process):
    if not select.select([process.stdout], [], [], 8)[0]:
        raise AssertionError('fixed pasta readiness timeout')
    line = process.stdout.readline()
    if not line:
        process.wait(timeout=3)
        raise AssertionError('pasta exited before fixed fixture readiness: ' + process.stderr.read())
    state = json.loads(line)
    if state['service_started'] or state['netns'] in (
            os.readlink('/proc/self/ns/net'), os.environ['KAKOI_PROBE_HOST_NET']):
        raise AssertionError('fixture must be unstarted in a separate network namespace')


def probe(pasta, host_family, target_family, conflict=False):
    require_private_namespace()
    setup()
    af, host = endpoint(host_family)
    if conflict:
        control = launch(pasta, host_family, target_family)
        try:
            initial_ready(control)
            expect_listeners(control, host_family, host, 18080, True)
        finally:
            finish(control)
        with new_socket(af, socket.SOCK_STREAM) as tcp, new_socket(af, socket.SOCK_DGRAM) as udp:
            for sock in (tcp, udp):
                sock.bind((host, 18080))
            tcp.listen()
            host_owner(tcp, udp, af, host)
            process = launch(pasta, host_family, target_family)
            try:
                if not select.select([process.stdout], [], [], 8)[0]:
                    raise AssertionError('conflict startup did not settle')
                line = process.stdout.readline()
                if line:
                    # Readiness is not proof of a successful host bind.
                    json.loads(line)
                    expect_listeners(process, host_family, host, 18080, False)
                else:
                    process.wait(timeout=3)
                    diagnostic = process.stderr.read()
                    emit(conflict_stderr=diagnostic)
                    if process.returncode == 0 or 'Address already in use' not in diagnostic:
                        raise AssertionError('startup did not fail due to the injected bind conflict')
                host_owner(tcp, udp, af, host)
                emit(conflict_detected=True, unrelated_host_owners_survived=True,
                     service_started=False, pasta_exit=process.poll())
            finally:
                finish(process)
        return
    for environment in range(2):
        process = launch(pasta, host_family, target_family)
        try:
            initial_ready(process)
            expect_listeners(process, host_family, host, 18080, True)
            # Both UDP clients retain their source port across the service restart.
            with new_socket(af, socket.SOCK_DGRAM) as dormant, new_socket(af, socket.SOCK_DGRAM) as active:
                for client in (dormant, active):
                    client.connect((host, 18080))
                command(process, 'A', 'started')
                for client in (dormant, active):
                    if sample(client) != 'A':
                        raise AssertionError('original service did not answer')
                command(process, 'stop', 'stopped')
                expect_listeners(process, host_family, host, 18080, True)
                if sample(active) not in ('timeout', 'refused'):
                    raise AssertionError('stopped service still answered')
                command(process, 'B', 'started')
                for client in (dormant, active):
                    observations = [sample(client) for _ in range(3)]
                    if 'B' not in observations or any(x not in ('B', 'timeout', 'refused') for x in observations):
                        raise AssertionError('same-peer UDP restart failed: ' + repr(observations))
                    emit(environment=environment, udp_peer=client.getsockname(), after_restart=observations)
                with new_socket(af, socket.SOCK_STREAM) as tcp:
                    tcp.connect((host, 18080))
                    token = os.urandom(16)
                    tcp.sendall(token)
                    data = b''
                    while len(data) < len(token) + 2:
                        part = tcp.recv(1024)
                        if not part:
                            break
                        data += part
                    if data != b'B:' + token:
                        raise AssertionError('TCP fixed mapping failed')
        finally:
            finish(process)
        # No SO_REUSEADDR: normal bind must be possible after environment cleanup.
        with new_socket(af, socket.SOCK_STREAM) as tcp, new_socket(af, socket.SOCK_DGRAM) as udp:
            for sock in (tcp, udp):
                sock.bind((host, 18080))
        emit(environment=environment, fixed_ports_released=True)
    emit(host_family=host_family, target_family=target_family,
         scope='fixed startup/restart/conflict mechanism; not product startup supervisor')


if __name__ == '__main__':
    require_private_namespace()
    if sys.argv[1] == 'inner':
        inner(sys.argv[2])
    elif sys.argv[1] == 'serve':
        serve(sys.argv[2], sys.argv[3])
    else:
        raise SystemExit('Use run.py')
