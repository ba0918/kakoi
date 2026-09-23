"""E2 mechanism probe: nft hooks on pasta local socket forwarding in both directions."""
import errno
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import threading
from guard import require_private_namespace


def exchange(family, address, port, protocol):
    kind = socket.SOCK_STREAM if protocol == 'tcp' else socket.SOCK_DGRAM
    with socket.socket(family, kind) as client:
        client.settimeout(.5)
        try:
            client.connect((address, port))
            if protocol == 'udp':
                client.send(b'probe')
            expected = b'tcp-ok' if protocol == 'tcp' else b'udp-ok'
            return client.recv(32) == expected
        except OSError as error:
            if isinstance(error, TimeoutError) or error.errno in (errno.ECONNREFUSED, errno.EPERM, errno.EACCES):
                return False
            raise


def host_servers(family, address):
    servers = []
    stop = threading.Event()
    errors = []
    for kind in (socket.SOCK_STREAM, socket.SOCK_DGRAM):
        server = socket.socket(family, kind)
        server.bind((address, 8081))
        if kind == socket.SOCK_STREAM:
            server.listen()
        servers.append(server)

    def serve():
        try:
            while not stop.is_set():
                for server in select.select(servers, [], [], .1)[0]:
                    if server.type == socket.SOCK_STREAM:
                        connection, _ = server.accept()
                        with connection:
                            connection.settimeout(.5)
                            connection.sendall(b'tcp-ok')
                    else:
                        _, peer = server.recvfrom(64)
                        server.sendto(b'udp-ok', peer)
        except Exception as error:
            errors.append(str(error))
    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    return servers, stop, thread, errors


def probe(pasta, label):
    require_private_namespace()
    from worker import run, loopback_options
    run('ip', 'link', 'set', 'lo', 'up')
    run('ip', 'link', 'add', 'probe0', 'type', 'dummy')
    run('ip', 'link', 'set', 'probe0', 'up')
    if label == 'ipv4':
        family, address, flag = socket.AF_INET, '127.0.0.1', '-4'
        run('ip', 'addr', 'add', '198.18.0.1/24', 'dev', 'probe0')
        run('ip', 'route', 'add', 'default', 'dev', 'probe0')
    else:
        family, address, flag = socket.AF_INET6, '::1', '-6'
        run('ip', '-6', 'addr', 'add', '2001:db8::1/64', 'dev', 'probe0', 'nodad')
        run('ip', '-6', 'addr', 'add', 'fe80::1/64', 'dev', 'probe0', 'nodad')
        run('ip', '-6', 'route', 'add', 'default', 'dev', 'probe0')
    servers, stop, thread, errors = host_servers(family, address)
    argv = [pasta, '-f', flag, '--config-net', *loopback_options(), '-i', 'probe0',
            '-t', address + '/18080:8080', '-u', address + '/18080:8080',
            '-T', '18081:8081', '-U', '18081:8081',
            '--', sys.executable, str(Path(__file__).resolve()), 'server', label]
    print(json.dumps({'pasta_argv': argv}), flush=True)
    process = subprocess.Popen(argv, stdout=subprocess.PIPE, text=True)
    try:
        if not select.select([process.stdout], [], [], 10)[0]:
            raise RuntimeError('service readiness timeout')
        line = process.stdout.readline()
        if not line:
            raise RuntimeError('pasta exited before server started')
        inner_net = json.loads(line)['netns']
        if inner_net in (os.readlink('/proc/self/ns/net'), os.environ['KAKOI_PROBE_HOST_NET']):
            raise RuntimeError('server is not in a separate network namespace')
        # pasta creates a nested PID namespace. Locate its network namespace
        # through the controller's procfs, rather than trusting an inner PID.
        targets = []
        for entry in Path('/proc').iterdir():
            if not entry.name.isdigit():
                continue
            try:
                if os.readlink(entry / 'ns/net') == inner_net:
                    targets.append(entry.name)
            except (FileNotFoundError, ProcessLookupError):
                continue
        if not targets:
            raise RuntimeError('inner network namespace is not visible to controller')
        enter = ['nsenter', '--target', targets[0], '--net']
        results = []
        for verdict in ('accept', 'drop', 'accept'):
            rules = f'''table inet hook_probe {{
 chain input {{ type filter hook input priority 0; policy accept;
 tcp dport 8080 counter {verdict}
 udp dport 8080 counter {verdict}
 }}
 chain output {{ type filter hook output priority 0; policy accept;
 tcp dport 18081 counter {verdict}
 udp dport 18081 counter {verdict}
 }}
}}'''
            # Delete/recreate is only measurement setup, not a proposed product
            # update mechanism. Test clients send only after replacement.
            if verdict == 'drop' or results:
                subprocess.run(enter + ['nft', 'delete', 'table', 'inet', 'hook_probe'], check=True, timeout=5)
            subprocess.run(enter + ['nft', '-f', '-'], input=rules, text=True, check=True, timeout=5)
            incoming = {protocol: exchange(family, address, 18080, protocol) for protocol in ('tcp', 'udp')}
            outgoing_result = subprocess.run(
                enter + [sys.executable, str(Path(__file__).resolve()), 'client', label],
                capture_output=True, text=True, timeout=5, check=True)
            outgoing = json.loads(outgoing_result.stdout)
            counters = subprocess.run(enter + ['nft', '-j', 'list', 'table', 'inet', 'hook_probe'],
                                      capture_output=True, text=True, timeout=5, check=True)
            observation = {'family': label, 'verdict': verdict, 'host_to_inner': incoming,
                           'inner_to_host': outgoing, 'ruleset': json.loads(counters.stdout)}
            print(json.dumps(observation), flush=True)
            results.append(observation)
            expected = verdict == 'accept'
            if not all(value == expected for value in [*incoming.values(), *outgoing.values()]):
                raise AssertionError('nft local forwarding observation differs from selected verdict')
        if errors:
            raise RuntimeError('host fixture errors: ' + str(errors))
        print(json.dumps({'scope': 'local socket forwarding hooks only', 'full_E2': False}), flush=True)
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        stop.set()
        thread.join(timeout=1)
        for server in servers:
            server.close()


if __name__ == '__main__':
    require_private_namespace()
    mode, label = sys.argv[1:3]
    if mode == 'server':
        # Readiness is emitted only after binding, by the shared fixture.
        from worker import serve
        serve(label, announce_namespace=True)
    elif mode == 'client':
        family, address = (socket.AF_INET, '127.0.0.1') if label == 'ipv4' else (socket.AF_INET6, '::1')
        print(json.dumps({protocol: exchange(family, address, 18081, protocol) for protocol in ('tcp', 'udp')}))
    else:
        raise SystemExit('unknown internal mode')
