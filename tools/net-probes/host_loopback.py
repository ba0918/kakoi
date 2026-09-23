"""Two stock pasta processes: which endpoint the gateway address reaches from the
application namespace, with and without the inner stage's gateway mapping."""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import threading

from boundary import addresses
from fixed import setup
from guard import require_private_namespace
from hooks import host_servers
from worker import run

HERE = str(Path(__file__).resolve())
PORT = 8081
# A dedicated alias instead of the gateway, so that the real gateway stays itself.
DEDICATED = {'ipv4': '169.254.1.2', 'ipv6': 'fd6b:616b:6f69::2'}


def reply(family, address, protocol):
    """The first bytes an endpoint answers with, or None when nothing answers."""
    kind = socket.SOCK_STREAM if protocol == 'tcp' else socket.SOCK_DGRAM
    with socket.socket(family, kind) as client:
        client.settimeout(.5)
        try:
            client.connect((address, PORT))
            if protocol == 'udp':
                client.send(b'probe')
            return client.recv(32).decode(errors='replace')
        except OSError:
            return None


def middle_servers(family, address):
    """Loopback services of the middle namespace answer differently from the host's."""
    stop = threading.Event()
    sockets = []
    for kind in (socket.SOCK_STREAM, socket.SOCK_DGRAM):
        server = socket.socket(family, kind)
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind((address, PORT))
        if kind == socket.SOCK_STREAM:
            server.listen()
        server.settimeout(.2)
        sockets.append(server)

    def serve():
        while not stop.is_set():
            for server in sockets:
                try:
                    if server.type == socket.SOCK_STREAM:
                        connection, _ = server.accept()
                        with connection:
                            connection.sendall(b'middle-tcp')
                    else:
                        _, peer = server.recvfrom(64)
                        server.sendto(b'middle-udp', peer)
                except (TimeoutError, OSError):
                    continue

    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    return stop, thread, sockets


def app(label):
    family, _, gateway, external = addresses(label)
    routes = subprocess.run(['ip', '-j', '-4' if label == 'ipv4' else '-6', 'route'],
                            capture_output=True, text=True, check=True, timeout=3)
    print(json.dumps({
        'app_netns': os.readlink('/proc/self/ns/net'),
        'app_routes': json.loads(routes.stdout),
        'replies': {name: {protocol: reply(family, address, protocol) for protocol in ('tcp', 'udp')}
                    for name, address in (('gateway', gateway), ('dedicated', DEDICATED[label]),
                                          ('host_nonloopback', external))},
    }), flush=True)


def middle(pasta, label, mapping):
    family, loopback, _, _ = addresses(label)
    run('ip', 'link', 'set', 'lo', 'up')
    stop, thread, sockets = middle_servers(family, loopback)
    flag = '-4' if label == 'ipv4' else '-6'
    argv = [pasta, '-f', flag, '--config-net', '-I', 'app0',
            '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none']
    if mapping in ('no-map-gw', 'dedicated'):
        argv.append('--no-map-gw')
    argv += ['--', sys.executable, HERE, 'app', pasta, label, mapping]
    try:
        result = subprocess.run(argv, capture_output=True, text=True, timeout=15,
                                env={**os.environ, 'LC_ALL': 'C'})
        print(json.dumps({'inner_argv': argv, 'middle_netns': os.readlink('/proc/self/ns/net'),
                          'inner_exit': result.returncode, 'inner_stderr': result.stderr,
                          'app': json.loads(result.stdout) if result.returncode == 0 else None}),
              flush=True)
    finally:
        stop.set()
        thread.join(timeout=1)
        for server in sockets:
            server.close()


def probe(pasta, label):
    require_private_namespace()
    setup()
    family, loopback, _, external = addresses(label)
    prefix = '24' if label == 'ipv4' else '64'
    tail = [] if label == 'ipv4' else ['nodad']
    run('ip', '-4' if label == 'ipv4' else '-6', 'addr', 'add', external + '/' + prefix, 'dev', 'lo', *tail)
    fixtures = [host_servers(family, address) for address in (loopback, external)]
    flag = '-4' if label == 'ipv4' else '-6'
    outcomes = {}
    try:
        for mapping in ('map-gw', 'no-map-gw', 'dedicated'):
            outer = (['--no-map-gw', '--map-host-loopback', DEDICATED[label]]
                     if mapping == 'dedicated' else [])
            argv = [pasta, '-f', flag, '--config-net', '-i', 'probe0', '-I', 'middle0',
                    '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none', *outer,
                    '--', sys.executable, HERE, 'middle', pasta, label, mapping]
            process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                       env={**os.environ, 'LC_ALL': 'C'})
            try:
                if not select.select([process.stdout], [], [], 25)[0]:
                    raise RuntimeError('middle stage timeout')
                line = process.stdout.readline()
                if not line:
                    raise RuntimeError('middle stage ended early: ' + process.stderr.read())
                outcome = json.loads(line)
            finally:
                process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            print(json.dumps({'mapping': mapping, 'outer_argv': argv, **outcome}), flush=True)
            if outcome['app'] is None:
                raise RuntimeError('inner stage failed: ' + outcome['inner_stderr'])
            outcomes[mapping] = outcome['app']['replies']
        for _, _, _, errors in fixtures:
            if errors:
                raise RuntimeError('host fixture failed: ' + str(errors))
        # The initial release needs the host loopback through both stages.
        reached = outcomes['no-map-gw']['gateway']
        if reached != {'tcp': 'tcp-ok', 'udp': 'udp-ok'}:
            raise RuntimeError('gateway did not reach the host loopback without inner mapping: '
                               + json.dumps(outcomes))
        dedicated = outcomes['dedicated']
        if dedicated['dedicated'] != {'tcp': 'tcp-ok', 'udp': 'udp-ok'}:
            raise RuntimeError('the dedicated alias did not reach the host loopback: ' + json.dumps(outcomes))
        if dedicated['gateway'] in ({'tcp': 'tcp-ok', 'udp': 'udp-ok'}, {'tcp': 'middle-tcp', 'udp': 'middle-udp'}):
            raise RuntimeError('the gateway was still translated with a dedicated alias: ' + json.dumps(outcomes))
        print(json.dumps({'scope': 'two-stage host loopback reachability, no policy', 'outcomes': outcomes}),
              flush=True)
    finally:
        for servers, stop, thread, _ in fixtures:
            stop.set()
            thread.join(timeout=1)
            for server in servers:
                server.close()


if __name__ == '__main__':
    require_private_namespace()
    mode = sys.argv[1]
    if mode == 'middle':
        middle(sys.argv[2], sys.argv[3], sys.argv[4])
    elif mode == 'app':
        app(sys.argv[3])
    else:
        raise SystemExit('Use run.py')
