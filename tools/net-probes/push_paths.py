"""Echo fixtures that can send data without a new request on registered flows."""
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys

from dynamic import emit, endpoint, new_socket
from guard import require_private_namespace
from lease_failure import ready, stop_process

REGISTER = b'probe-register-push'
PAYLOAD = b'probe-unsolicited-push'


def serve(family, address, port, internal=None):
    require_private_namespace()
    af, _ = endpoint(family)
    tcp, udp = new_socket(af, socket.SOCK_STREAM), new_socket(af, socket.SOCK_DGRAM)
    tcp.bind((address, port))
    udp.bind((address, port))
    tcp.listen()
    readers, registered, peers = [tcp, udp], set(), set()
    pending = False
    def request(*_):
        nonlocal pending
        pending = True
    signal.signal(signal.SIGUSR1, request)
    # pasta leaves its startup SIGUSR1 blocked across exec; a handler alone
    # does not unblock the fixture's transmission trigger.
    signal.pthread_sigmask(signal.SIG_UNBLOCK, {signal.SIGUSR1})
    if internal is None:
        print('ready', flush=True)
    else:
        emit(netns=os.readlink('/proc/self/ns/net'))
    while True:
        if pending:
            pending = False
            attempted = 0
            for conn in list(registered):
                attempted += 1
                try:
                    conn.sendall(PAYLOAD)
                except (BrokenPipeError, ConnectionResetError, TimeoutError, PermissionError):
                    pass
            for peer in peers:
                attempted += 1
                try:
                    udp.sendto(PAYLOAD, peer)
                except (ConnectionRefusedError, PermissionError):
                    pass
            if not registered or not peers:
                raise AssertionError('push has no registered TCP/UDP recipients')
            if internal is not None:
                os.kill(internal.pid, signal.SIGUSR1)
                acknowledgement(internal)
            emit(push_attempted=attempted)
        for sock in select.select(readers, [], [], .02)[0]:
            if sock is tcp:
                conn, _ = tcp.accept()
                conn.settimeout(.2)
                readers.append(conn)
            elif sock is udp:
                data, peer = udp.recvfrom(1024)
                if data == REGISTER:
                    peers.add(peer)
                try:
                    udp.sendto(data, peer)
                except (PermissionError, ConnectionRefusedError):
                    pass
            else:
                try:
                    data = sock.recv(1024)
                    if data == REGISTER:
                        registered.add(sock)
                    if data:
                        sock.sendall(data)
                except (ConnectionResetError, BrokenPipeError, TimeoutError, PermissionError):
                    data = b''
                if not data:
                    registered.discard(sock)
                    readers.remove(sock)
                    sock.close()


def acknowledgement(process):
    if not select.select([process.stdout], [], [], 3)[0]:
        raise AssertionError('push fixture did not acknowledge transmission attempts')
    result = json.loads(process.stdout.readline())
    if result.get('push_attempted', 0) < 2:
        raise AssertionError('push fixture did not attempt both protocols')


def inner_pid(identity):
    targets = []
    for path in Path('/proc').iterdir():
        if not path.name.isdecimal():
            continue
        try:
            argv = (path / 'cmdline').read_bytes().split(b'\0')
            if argv[:3] == [sys.executable.encode(), str(Path(__file__).resolve()).encode(), b'inner'] and os.readlink(path / 'ns/net') == identity:
                targets.append(int(path.name))
        except (FileNotFoundError, ProcessLookupError):
            pass
    if len(targets) != 1:
        raise AssertionError('inner push fixture not uniquely identified')
    return targets[0]


if __name__ == '__main__':
    require_private_namespace()
    mode, family = sys.argv[1:3]
    if mode == 'server':
        serve(family, sys.argv[3], int(sys.argv[4]))
    elif mode == 'inner':
        from recovery import nft, table
        nft([], table('recovery_guard', 'drop'))
        _, loopback = endpoint(family)
        internal = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), 'server', family, loopback, '8082'], stdout=subprocess.PIPE, text=True)
        try:
            ready(internal)
            serve(family, loopback, 8080, internal)
        finally:
            stop_process(internal)
    else:
        raise SystemExit('Use run.py')
