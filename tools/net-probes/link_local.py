"""Stock pasta: can the application reach an IPv6 link-local neighbour on a
chosen host interface, and only there? A global address of the same neighbour is
the control that the fixture carries traffic at all."""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time

from fixed import setup
from guard import require_private_namespace
from worker import run

HERE = str(Path(__file__).resolve())
PORT = 8081
NEIGHBOUR = 'fe80::2'
# The same neighbour on a global address: shows the fixture itself carries traffic.
GLOBAL = '2001:db8:5::2'


def neighbour(interface):
    """Runs in the neighbour namespace: answers TCP and UDP on its link-local address."""
    run('ip', 'link', 'set', 'lo', 'up')
    for _ in range(50):
        if Path('/sys/class/net/' + interface).exists():
            break
        time.sleep(.1)
    run('ip', '-6', 'addr', 'add', NEIGHBOUR + '/64', 'dev', interface, 'nodad')
    run('ip', '-6', 'addr', 'add', GLOBAL + '/64', 'dev', interface, 'nodad')
    run('ip', 'link', 'set', interface, 'up')
    tcp = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
    tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    tcp.bind(('::', PORT))
    tcp.listen()
    udp = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
    udp.bind(('::', PORT))
    print('ready', flush=True)
    while True:
        readable, _, _ = select.select([tcp, udp], [], [], 30)
        if not readable:
            return
        for server in readable:
            if server is tcp:
                connection, _ = tcp.accept()
                with connection:
                    connection.sendall(b'neighbour-tcp')
            else:
                _, peer = udp.recvfrom(64)
                udp.sendto(b'neighbour-udp', peer)


def reply(interface, protocol, address=NEIGHBOUR):
    kind = socket.SOCK_STREAM if protocol == 'tcp' else socket.SOCK_DGRAM
    with socket.socket(socket.AF_INET6, kind) as client:
        client.settimeout(1)
        try:
            scope = socket.if_nametoindex(interface) if address == NEIGHBOUR else 0
            client.connect((address, PORT, 0, scope))
            if protocol == 'udp':
                client.send(b'probe')
            return client.recv(32).decode(errors='replace')
        except OSError as error:
            return 'failed ' + type(error).__name__


def app():
    print(json.dumps({'replies': {protocol: reply('app0', protocol) for protocol in ('tcp', 'udp')},
                      'global': {protocol: reply('app0', protocol, GLOBAL) for protocol in ('tcp', 'udp')}}),
          flush=True)


def middle(pasta):
    argv = [pasta, '-f', '-6', '--config-net', '-I', 'app0', '--no-map-gw',
            '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none',
            '--', sys.executable, HERE, 'app', pasta]
    result = subprocess.run(argv, capture_output=True, text=True, timeout=15,
                            env={**os.environ, 'LC_ALL': 'C'})
    print(json.dumps({'inner_argv': argv, 'inner_exit': result.returncode, 'inner_stderr': result.stderr,
                      'app': json.loads(result.stdout) if result.returncode == 0 else None}), flush=True)


def probe(pasta):
    require_private_namespace()
    setup()
    # The host-side link to a neighbour, and a second link without it.
    run('ip', 'link', 'add', 'hostll0', 'type', 'veth', 'peer', 'name', 'peerll0')
    run('ip', 'link', 'set', 'hostll0', 'up')
    run('ip', '-6', 'addr', 'add', 'fe80::1/64', 'dev', 'hostll0', 'nodad')
    run('ip', '-6', 'addr', 'add', '2001:db8:5::1/64', 'dev', 'hostll0', 'nodad')
    run('ip', 'link', 'add', 'otherll0', 'type', 'dummy')
    run('ip', 'link', 'set', 'otherll0', 'up')
    peer = subprocess.Popen(['unshare', '--net', sys.executable, HERE, 'neighbour', pasta, 'peerll0'],
                            stdout=subprocess.PIPE, text=True)
    outcomes = {}
    try:
        time.sleep(.3)
        run('ip', 'link', 'set', 'peerll0', 'netns', str(peer.pid))
        if not select.select([peer.stdout], [], [], 10)[0] or peer.stdout.readline().strip() != 'ready':
            raise RuntimeError('neighbour did not start')
        time.sleep(.5)
        outcomes['host'] = {protocol: reply('hostll0', protocol) for protocol in ('tcp', 'udp')}
        # One stage first: whether pasta forwards link-local traffic at all.
        argv = [pasta, '-f', '-6', '--config-net', '-i', 'probe0', '-I', 'app0',
                '--no-map-gw', '--outbound-if6', 'hostll0',
                '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none',
                '--', sys.executable, HERE, 'app', pasta]
        result = subprocess.run(argv, capture_output=True, text=True, timeout=25,
                                env={**os.environ, 'LC_ALL': 'C'})
        single = json.loads(result.stdout.splitlines()[-1]) if result.stdout else {}
        print(json.dumps({'single_argv': argv, 'single_exit': result.returncode,
                          'single_stderr': result.stderr, **single}), flush=True)
        outcomes['single'] = single.get('replies')
        outcomes['single_global'] = single.get('global')
        for interface in ('hostll0', 'otherll0'):
            argv = [pasta, '-f', '-6', '--config-net', '-i', 'probe0', '-I', 'middle0',
                    '--no-map-gw', '--outbound-if6', interface,
                    '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none',
                    '--', sys.executable, HERE, 'middle', pasta]
            result = subprocess.run(argv, capture_output=True, text=True, timeout=25,
                                    env={**os.environ, 'LC_ALL': 'C'})
            line = result.stdout.splitlines()[-1] if result.stdout else '{}'
            outcome = json.loads(line)
            print(json.dumps({'outbound_if6': interface, 'outer_argv': argv, 'outer_exit': result.returncode,
                              'outer_stderr': result.stderr, **outcome}), flush=True)
            outcomes[interface] = (outcome.get('app') or {}).get('replies')
        print(json.dumps({'scope': 'two-stage IPv6 link-local reachability, no policy', 'outcomes': outcomes}),
              flush=True)
        if outcomes['host'] != {'tcp': 'neighbour-tcp', 'udp': 'neighbour-udp'}:
            raise RuntimeError('the fixture neighbour is not reachable from the host role')
        if outcomes['hostll0'] != {'tcp': 'neighbour-tcp', 'udp': 'neighbour-udp'}:
            raise RuntimeError('the chosen interface did not reach the neighbour')
        if any(value.startswith('neighbour') for value in (outcomes['otherll0'] or {}).values()):
            raise RuntimeError('another interface reached the neighbour')
    finally:
        peer.terminate()
        try:
            peer.wait(timeout=3)
        except subprocess.TimeoutExpired:
            peer.kill()
            peer.wait()


if __name__ == '__main__':
    require_private_namespace()
    mode = sys.argv[1]
    if mode == 'middle':
        middle(sys.argv[2])
    elif mode == 'app':
        app()
    elif mode == 'neighbour':
        neighbour(sys.argv[3])
    else:
        raise SystemExit('Use run.py')
