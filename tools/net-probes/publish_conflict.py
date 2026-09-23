"""Measure unrelated publications and host owners during port conflicts."""
from contextlib import ExitStack
import os
from pathlib import Path
import socket
import subprocess
import sys

from dynamic import configure, echo, emit, endpoint, new_socket
from udp_churn import sample


def host_owner(tcp, udp, af, address):
    token = os.urandom(16)
    with new_socket(af, socket.SOCK_STREAM) as client:
        client.connect((address, 18080))
        accepted, _ = tcp.accept()
        with accepted:
            accepted.settimeout(0.5)
            accepted.sendall(token)
            if client.recv(32) != token:
                raise AssertionError('original host TCP owner did not answer')
    with new_socket(af, socket.SOCK_DGRAM) as client:
        client.connect((address, 18080))
        client.send(token)
        request, peer = udp.recvfrom(32)
        if request != token:
            raise AssertionError('original host UDP owner did not receive request')
        udp.sendto(token, peer)
        if client.recv(32) != token:
            raise AssertionError('original host UDP owner did not answer')


def service(af, address, port):
    result = {}
    for label, kind in (('tcp', socket.SOCK_STREAM), ('udp', socket.SOCK_DGRAM)):
        with new_socket(af, kind) as client:
            try:
                client.connect((address, port))
                result[label] = echo(client, os.urandom(16)) if label == 'tcp' else sample(client) == 'C'
            except (TimeoutError, ConnectionRefusedError, ConnectionResetError, PermissionError):
                result[label] = False
    return result


def listeners(pid, family, address, port):
    """Read a snapshot only; never send application data to identify an owner."""
    inodes = set()
    for fd in Path(f'/proc/{pid}/fd').iterdir():
        try:
            target = os.readlink(fd)
        except FileNotFoundError:
            continue
        if target.startswith('socket:['):
            inodes.add(target[8:-1])
    af, _ = endpoint(family)
    packed = socket.inet_pton(af, address)
    encoded = ''.join(f'{int.from_bytes(packed[i:i + 4], sys.byteorder):08X}'
                      for i in range(0, len(packed), 4))
    expected = f'{encoded}:{port:04X}'
    result = {}
    for protocol, state in (('tcp', '0A'), ('udp', '07')):
        table = protocol + ('6' if family == 'ipv6' else '')
        rows = Path('/proc/net/' + table).read_text().splitlines()[1:]
        result[protocol] = any(
            fields[1] == expected and fields[3] == state
            and not int(fields[2].replace(':', ''), 16) and fields[9] in inodes
            for fields in (row.split() for row in rows))
    return result


def expect_listeners(process, family, address, port, present):
    if process.poll() is not None:
        raise AssertionError('pasta exited before ownership observation')
    observed = listeners(process.pid, family, address, port)
    if process.poll() is not None:
        raise AssertionError('pasta exited during ownership observation')
    emit(listener_owner_pid=process.pid, port=port, pasta_owned=observed)
    if any(value != present for value in observed.values()):
        raise AssertionError('actual listener ownership did not match expected publication')


def exercise(process, pesto, control, family):
    af, address = endpoint(family)
    stable = address + '/18084:8084'
    occupied = address + '/18080:8084'
    fallback = address + '/18082:8084'
    configure(pesto, control, '-A', stable)
    expect_listeners(process, family, address, 18084, True)
    failures = []
    with new_socket(af, socket.SOCK_STREAM) as existing:
        existing.connect((address, 18084))
        if not echo(existing, b'initial') or not all(service(af, address, 18084).values()):
            raise AssertionError('unrelated publication did not start')
        with ExitStack() as stack:
            tcp = stack.enter_context(new_socket(af, socket.SOCK_STREAM))
            udp = stack.enter_context(new_socket(af, socket.SOCK_DGRAM))
            tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            for owner in (tcp, udp):
                owner.bind((address, 18080))
            tcp.listen()
            host_owner(tcp, udp, af, address)
            attempt = subprocess.run([pesto, '-A', '-t', occupied, '-u', occupied, control],
                                     capture_output=True, text=True, timeout=3)
            state = subprocess.run([pesto, control], capture_output=True, text=True, timeout=3)
            state.check_returncode()
            emit(conflicting_update_exit=attempt.returncode, stdout=attempt.stdout,
                 stderr=attempt.stderr, configuration_after_conflict=state.stdout)
            attempt.check_returncode()
            if '18080' not in state.stdout:
                raise AssertionError('conflicting rule was not received; conflict path unverified')
            host_owner(tcp, udp, af, address)
            stable_after = service(af, address, 18084)
            expect_listeners(process, family, address, 18080, False)
            expect_listeners(process, family, address, 18084, True)
            old_tcp = echo(existing, b'after-conflict')
            emit(host_owners_survived=True, unrelated_new=stable_after, unrelated_existing_tcp=old_tcp)
            if not all(stable_after.values()) or not old_tcp:
                failures.append('conflict disrupted unrelated publication')
            # Drop the conflicting rule before trying another explicitly allowed port.
            configure(pesto, control, '-D', occupied)
            configure(pesto, control, '-A', fallback)
            expect_listeners(process, family, address, 18082, True)
            alternative = service(af, address, 18082)
            emit(fallback=alternative)
            if not all(alternative.values()):
                failures.append('publication on alternative port failed')
            host_owner(tcp, udp, af, address)
            configure(pesto, control, '-D', fallback)
            expect_listeners(process, family, address, 18082, False)
        configure(pesto, control, '-A', occupied)
        expect_listeners(process, family, address, 18080, True)
        reused = service(af, address, 18080)
        stable_final = service(af, address, 18084)
        old_final = echo(existing, b'after-release')
        emit(released_host_port=reused, unrelated_final=stable_final, existing_tcp_final=old_final)
        if not all(reused.values()) or not all(stable_final.values()) or not old_final:
            failures.append('retry after host release or unrelated traffic failed')
        configure(pesto, control, '-D', occupied)
        configure(pesto, control, '-D', stable)
        expect_listeners(process, family, address, 18080, False)
        expect_listeners(process, family, address, 18084, False)
    emit(family=family, failures=failures,
         scope='explicit conflict/fallback/retry subset; no automatic allocator or exhaustive races')
    if failures:
        raise AssertionError('; '.join(failures))
