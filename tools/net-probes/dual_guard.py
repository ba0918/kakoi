"""Two stock pasta processes: expire only transit traffic, retain same-port loopback."""
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
import time

from dynamic import emit, endpoint, new_socket
from fixed import setup
from guard import require_private_namespace
from lease_failure import ready, stop_process
from push_paths import REGISTER, PAYLOAD, acknowledgement

HERE = str(Path(__file__).resolve())


def receive(sock, expected):
    data = sock.recv(len(expected))
    if sock.type == socket.SOCK_STREAM:
        while data and len(data) < len(expected):
            more = sock.recv(len(expected) - len(data))
            if not more:
                break
            data += more
    return data == expected


def exchange(family, port):
    af, address = endpoint(family)
    result = {}
    for name, kind in [('tcp', socket.SOCK_STREAM), ('udp', socket.SOCK_DGRAM)]:
        with new_socket(af, kind) as sock:
            sock.settimeout(.3)
            try:
                sock.connect((address, port))
                sock.sendall(b'probe-echo')
                result[name] = receive(sock, b'probe-echo')
            except (TimeoutError, ConnectionRefusedError, PermissionError):
                result[name] = False
    return result


def line(process):
    if not select.select([process.stdout], [], [], 5)[0]:
        raise AssertionError('dual pasta control timeout')
    value = process.stdout.readline()
    if not value:
        raise AssertionError('dual pasta exited before acknowledgement')
    return json.loads(value)


def ask(process, message):
    process.stdin.write(message + '\n')
    process.stdin.flush()
    return line(process)


def launch(pasta, family, inside, mode):
    _, address = endpoint(family)
    flag = '-4' if family == 'ipv4' else '-6'
    args = [pasta, '-f', flag, '--config-net', '--host-lo-to-ns-lo',
            '-I', 'middle0' if mode == 'middle' else 'app0',
            '-t', address + '/18080:' + str(inside),
            '-u', address + '/18080:' + str(inside), '-T', 'none', '-U', 'none',
            '--', sys.executable, HERE, mode, pasta, family]
    return subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)


def app(family):
    _, address = endpoint(family)
    server = subprocess.Popen([sys.executable, str(Path(HERE).with_name('push_paths.py')),
                               'server', family, address, '8080'], stdout=subprocess.PIPE, text=True)
    try:
        ready(server)
        emit(app_netns=os.readlink('/proc/self/ns/net'))
        for command in sys.stdin:
            if command.strip() == 'check':
                emit(internal=exchange(family, 8080))
            elif command.strip() == 'push':
                os.kill(server.pid, signal.SIGUSR1)
                acknowledgement(server)
                emit(push_attempted=True)
            elif command.strip() == 'exit':
                return
            else:
                raise AssertionError('unexpected app command')
    finally:
        stop_process(server)


def install_guard():
    subprocess.run(['nft', '-f', '-'], check=True, text=True, timeout=4, input='''table inet transit_health {
 set live { type ifname; flags timeout; }
 chain input { type filter hook input priority -100; policy drop; iifname @live accept; }
 chain output { type filter hook output priority -100; policy drop; oifname @live accept; }
}''')


def renew_guard():
    subprocess.run(['nft', '-f', '-'], check=True, text=True, timeout=4, input='flush set inet transit_health live\nadd element inet transit_health live { "lo" timeout 2s, "middle0" timeout 2s }\n')


def middle(pasta, family):
    install_guard()
    child = launch(pasta, family, 8080, 'app')
    try:
        state = line(child)
        assert state['app_netns'] != os.readlink('/proc/self/ns/net')
        emit(middle_netns=os.readlink('/proc/self/ns/net'), **state)
        for command in sys.stdin:
            command = command.strip()
            if command == 'renew':
                renew_guard()
                emit(renewed=True)
            elif command == 'exit':
                ask_exit(child)
                return
            else:
                print(json.dumps(ask(child, command)), flush=True)
    finally:
        stop_process(child)


def ask_exit(process):
    process.stdin.write('exit\n')
    process.stdin.flush()
    process.wait(timeout=3)


def probe(pasta, family):
    require_private_namespace()
    setup()
    af, address = endpoint(family)
    process = launch(pasta, family, 18080, 'middle')
    try:
        state = line(process)
        assert len({os.readlink('/proc/self/ns/net'), state['middle_netns'], state['app_netns']}) == 3
        before = exchange(family, 18080)
        assert not any(before.values()), before
        assert all(ask(process, 'check')['internal'].values())
        assert ask(process, 'renew')['renewed']
        with new_socket(af, socket.SOCK_STREAM) as tcp, new_socket(af, socket.SOCK_DGRAM) as udp:
            for sock in (tcp, udp):
                sock.settimeout(.5)
                sock.connect((address, 18080))
                sock.sendall(REGISTER)
                assert receive(sock, REGISTER)
            assert ask(process, 'push')['push_attempted']
            assert receive(tcp, PAYLOAD) and receive(udp, PAYLOAD)
            time.sleep(2.2)
            assert ask(process, 'push')['push_attempted']
            for sock in (tcp, udp):
                try:
                    received = sock.recv(64)
                    assert not received, 'push passed after lease expiry'
                except TimeoutError:
                    pass
            after = exchange(family, 18080)
            assert not any(after.values()), after
            internal = ask(process, 'check')['internal']
            assert all(internal.values()), internal
            emit(startup_publication=before, expired_publication=after,
                 same_port_internal=internal, unsolicited_push_blocked=True, **state)
        ask_exit(process)
    finally:
        stop_process(process)


if __name__ == '__main__':
    require_private_namespace()
    mode, pasta, family = sys.argv[1:]
    if mode == 'middle':
        middle(pasta, family)
    elif mode == 'app':
        app(family)
    else:
        raise SystemExit('Use run.py')
