"""Stop a real lease updater, then independently block established traffic."""
from contextlib import ExitStack
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
import time

from guard import require_private_namespace
from dynamic import echo, emit, endpoint, new_socket
from worker import run


def nft(text):
    subprocess.run(['nft', '-f', '-'], input=text, text=True, check=True, timeout=2)


def controller(table):
    require_private_namespace()
    while True:
        nft(f'flush set inet {table} allowed\nadd element inet {table} allowed {{ 8080 timeout 2s }}\n')
        print('ready', flush=True)
        if sys.stdin.readline().strip() != 'renew':
            return


def exchange(client):
    try:
        return echo(client, os.urandom(16))
    except (TimeoutError, ConnectionRefusedError, ConnectionResetError, PermissionError):
        return False


def fresh(af, address, kind):
    with new_socket(af, kind) as client:
        client.settimeout(0.2)
        try:
            client.connect((address, 8080))
            return exchange(client)
        except (TimeoutError, ConnectionRefusedError, PermissionError):
            return False


def ready(process):
    if not select.select([process.stdout], [], [], 3)[0] or process.stdout.readline().strip() != 'ready':
        raise AssertionError('fixture or controller not ready')


def stop_process(process):
    if process is not None:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=3)


def probe(fault, liveness=False):
    require_private_namespace()
    run('ip', 'link', 'set', 'lo', 'up')
    for family in ('ipv4', 'ipv6'):
        af, address = endpoint(family)
        table = 'lease_' + family
        server = updater = None
        try:
            server = subprocess.Popen([sys.executable, str(Path(__file__).with_name('dynamic.py')), 'serve', family],
                                      stdout=subprocess.PIPE, text=True)
            ready(server)
            heartbeat_guard = 'meta l4proto { tcp, udp } th dport 8080 th dport != @allowed drop' if liveness else ''
            nft(f'''table inet {table} {{
 set allowed {{ type inet_service; flags timeout; }}
 chain output {{ type filter hook output priority 0; policy accept;
  {heartbeat_guard}
  ct state established accept
  meta l4proto {{ tcp, udp }} th dport @allowed accept
  meta l4proto {{ tcp, udp }} th dport 8080 drop
 }}
}}''')
            if any(fresh(af, address, kind) for kind in (socket.SOCK_STREAM, socket.SOCK_DGRAM)):
                raise AssertionError('traffic allowed before a lease existed')
            updater = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), 'controller', table],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
            ready(updater)
            with ExitStack() as stack:
                clients = {}
                for label, kind in (('tcp', socket.SOCK_STREAM), ('udp', socket.SOCK_DGRAM)):
                    client = stack.enter_context(new_socket(af, kind))
                    client.settimeout(0.2)
                    client.connect((address, 8080))
                    clients[label] = client
                    if not exchange(client):
                        raise AssertionError('permitted traffic failed')
                for _ in range(2):
                    time.sleep(1.2)
                    updater.stdin.write('renew\n')
                    updater.stdin.flush()
                    ready(updater)
                if not all(fresh(af, address, kind) for kind in (socket.SOCK_STREAM, socket.SOCK_DGRAM)):
                    raise AssertionError('lease renewal did not preserve fresh traffic')
                if fault == 'stop':
                    updater.send_signal(signal.SIGSTOP)
                    deadline = time.monotonic() + 1
                    while '\nState:\tT' not in Path('/proc/' + str(updater.pid) + '/status').read_text():
                        if time.monotonic() >= deadline:
                            raise AssertionError('controller did not stop')
                        time.sleep(0.01)
                elif fault == 'kill':
                    updater.kill()
                    updater.wait(timeout=3)
                else:
                    raise ValueError('unknown fault')
                # ready acknowledges completed nft updates; no writer is in flight.
                time.sleep(2.3)
                active_set = subprocess.run(['nft', '-j', 'list', 'set', 'inet', table, 'allowed'],
                                            capture_output=True, text=True, check=True, timeout=2)
                old = {label: exchange(client) for label, client in clients.items()}
                new = {label: fresh(af, address, kind) for label, kind in
                       (('tcp', socket.SOCK_STREAM), ('udp', socket.SOCK_DGRAM))}
                emit(family=family, fault=fault, lease_set=json.loads(active_set.stdout),
                     existing_after_expiry=old, fresh_after_expiry=new, liveness=liveness)
                if any(value != (not liveness) for value in old.values()) or any(new.values()):
                    raise AssertionError('lease expiry did not enforce the expected flow lifetime')
                # The independent supervisor's final block precedes established acceptance.
                nft(f'''flush chain inet {table} output
add rule inet {table} output meta l4proto {{ tcp, udp }} th dport 8080 drop
''')
                blocked = {label: exchange(client) for label, client in clients.items()}
                new_blocked = {label: fresh(af, address, kind) for label, kind in
                               (('tcp', socket.SOCK_STREAM), ('udp', socket.SOCK_DGRAM))}
                emit(family=family, final_block_existing=blocked, final_block_new=new_blocked)
                if any(blocked.values()) or any(new_blocked.values()):
                    raise AssertionError('independent final block left traffic working')
        finally:
            stop_process(updater)
            stop_process(server)
            subprocess.run(['nft', 'delete', 'table', 'inet', table], capture_output=True, timeout=2)
    emit(fault=fault, liveness=liveness, scope='finite kernel liveness before established acceptance' if liveness else 'real updater failure and independent block on loopback; no pasta integration, automatic watchdog, recovery or terminal proof')


if __name__ == '__main__':
    require_private_namespace()
    if len(sys.argv) == 3 and sys.argv[1] == 'controller':
        controller(sys.argv[2])
    else:
        raise SystemExit('Use run.py')
