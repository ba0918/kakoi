"""Observe old UDP peers when a publication moves to another service."""
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time

from guard import require_private_namespace
from dynamic import emit, endpoint, new_socket


def serve(family):
    require_private_namespace()
    af, address = endpoint(family)
    with new_socket(af, socket.SOCK_DGRAM) as old, new_socket(af, socket.SOCK_DGRAM) as new:
        old.bind((address, 8080))
        new.bind((address, 8082))
        readers = [old, new, sys.stdin]
        print('ready', flush=True)
        while True:
            for source in select.select(readers, [], [], 1)[0]:
                if source is sys.stdin:
                    command = sys.stdin.readline().strip()
                    if command != 'close-old' or old not in readers:
                        raise RuntimeError('unexpected fixture command')
                    readers.remove(old)
                    old.close()
                    print('old-closed', flush=True)
                else:
                    data, peer = source.recvfrom(1024)
                    source.sendto((b'A:' if source is old else b'B:') + data, peer)


def configure(pesto, path, operation, target):
    for argv in ([pesto, operation, '-u', target, path], [pesto, path]):
        result = subprocess.run(argv, capture_output=True, text=True, timeout=3)
        emit(control_argv=argv, exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
        result.check_returncode()


def sample(client):
    token = os.urandom(16)
    try:
        client.send(token)
        deadline = time.monotonic() + 0.5
        while time.monotonic() < deadline:
            client.settimeout(max(0.001, deadline - time.monotonic()))
            reply = client.recv(1024)
            if reply == b'A:' + token:
                return 'A'
            if reply == b'B:' + token:
                return 'B'
            # A delayed prior reply must not count as this request's response.
        return 'timeout'
    except TimeoutError:
        return 'timeout'
    except ConnectionRefusedError:
        return 'refused'
    finally:
        client.settimeout(0.5)


def observe(client, phase, expected):
    replies = [sample(client) for _ in range(3)]
    emit(phase=phase, source=client.getsockname(), replies=replies, expected=expected)
    if expected is None:
        return all(reply in ('timeout', 'refused') for reply in replies)
    return expected in replies and all(reply in (expected, 'timeout', 'refused') for reply in replies)


def exercise(process, pesto, control, family, expiry=False):
    af, address = endpoint(family)
    old_mapping, new_mapping = address + '/18080:8080', address + '/18080:8082'
    failures = []
    # Keeping all clients open prevents source-port reuse from hiding old flows.
    with new_socket(af, socket.SOCK_DGRAM) as dormant, new_socket(af, socket.SOCK_DGRAM) as active, new_socket(af, socket.SOCK_DGRAM) as fresh:
        for client in (dormant, active, fresh):
            client.connect((address, 18080))
        configure(pesto, control, '-A', old_mapping)
        for label, client in (('dormant', dormant), ('active', active)):
            if not observe(client, 'initial-' + label, 'A'):
                raise AssertionError('old service fixture did not answer')
        configure(pesto, control, '-D', old_mapping)
        process.stdin.write('close-old\n')
        process.stdin.flush()
        if not select.select([process.stdout], [], [], 3)[0] or process.stdout.readline().strip() != 'old-closed':
            raise AssertionError('old service close was not acknowledged')
        emit(old_service_socket_closed=True)
        if not observe(active, 'closed-and-unpublished', None):
            failures.append('reply after old service closed')
        configure(pesto, control, '-A', new_mapping)
        # Dormant peer has not provoked ICMP/flow cleanup during the closed gap.
        immediate_failures = []
        clients = (('dormant', dormant), ('active', active), ('fresh', fresh))
        for label, client in clients:
            if not observe(client, 'reassigned-' + label, 'B'):
                immediate_failures.append('new service did not receive ' + label)
        emit(immediate_failures=immediate_failures)
        if expiry:
            table = Path('/proc/net/udp' if af == socket.AF_INET else '/proc/net/udp6')
            def socket_rows():
                return [line for line in table.read_text().splitlines()[1:]
                        if line.split()[1].endswith(':46A0')]
            emit(before_idle_udp_sockets=socket_rows(), idle_seconds=12)
            started = time.monotonic()
            time.sleep(12)
            emit(after_idle_udp_sockets=socket_rows(), idle_elapsed=time.monotonic() - started)
            for label, client in clients:
                if not observe(client, 'after-idle-' + label, 'B'):
                    failures.append('new service did not receive after idle: ' + label)
        else:
            failures.extend(immediate_failures)
        configure(pesto, control, '-D', new_mapping)
        emit(family=family, failures=failures,
             scope=('UDP expiry recovery subset; immediate failures remain unresolved' if expiry else
                    'UDP service close and reassignment subset; at least one correct reply in three attempts, no old-service reply'))
        if failures:
            raise AssertionError('; '.join(failures))


if __name__ == '__main__':
    require_private_namespace()
    if len(sys.argv) != 3 or sys.argv[1] != 'serve':
        raise SystemExit('Use run.py')
    serve(sys.argv[2])
