"""Measure independent blocking across pasta paths after a controller dies."""
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

from boundary import addresses, namespace_entry
from dynamic import emit, new_socket, serve
from guard import require_private_namespace
from lease_failure import exchange as lease_exchange, ready, stop_process
from worker import run, loopback_options

HERE = str(Path(__file__).resolve())
KINDS = (('tcp', socket.SOCK_STREAM), ('udp', socket.SOCK_DGRAM))


def exchange(client):
    try:
        return lease_exchange(client)
    except BrokenPipeError:
        # A peer can close an existing TCP flow while the guard is installed.
        return False


def nft(enter, rules):
    subprocess.run(enter + ['nft', '-f', '-'], input=rules, text=True, check=True, timeout=3)


def table(name, verdict):
    return f'''table inet {name} {{
 chain input {{ type filter hook input priority -10; policy accept;
  meta l4proto {{ tcp, udp }} th dport 8080 counter {verdict}
  meta l4proto {{ tcp, udp }} th sport {{ 8080, 18081 }} counter {verdict}
 }}
 chain output {{ type filter hook output priority -10; policy accept;
  meta l4proto {{ tcp, udp }} th dport {{ 8080, 18081 }} counter {verdict}
  meta l4proto {{ tcp, udp }} th sport {{ 8080, 18081 }} counter {verdict}
 }}
}}'''


class Clients:
    def __init__(self, family, routes):
        self.af = addresses(family)[0]
        self.routes = routes
        self.stack = ExitStack()
        self.old = {}

    def sample(self, keep=False):
        result = {}
        for name, (address, port) in self.routes.items():
            result[name] = {}
            for protocol, kind in KINDS:
                key = (name, protocol)
                with new_socket(self.af, kind) as fresh:
                    fresh.settimeout(.2)
                    try:
                        fresh.connect((address, port))
                        new_ok = exchange(fresh)
                    except (TimeoutError, ConnectionRefusedError, PermissionError):
                        new_ok = False
                if keep:
                    old = self.stack.enter_context(new_socket(self.af, kind))
                    old.settimeout(.2)
                    old.connect((address, port))
                    self.old[key] = old
                old_ok = exchange(self.old[key]) if key in self.old else None
                result[name][protocol] = {'fresh': new_ok, 'existing': old_ok}
        return result

    def register_push(self):
        from push_paths import REGISTER
        from dynamic import echo
        for sock in self.old.values():
            if not echo(sock, REGISTER):
                raise AssertionError('push registration failed')
        return {}

    def receive_push(self):
        from push_paths import PAYLOAD
        result = {}
        for (route, protocol), sock in self.old.items():
            try:
                data = b''
                while len(data) < len(PAYLOAD):
                    chunk = sock.recv(len(PAYLOAD) - len(data))
                    if not chunk:
                        break
                    data += chunk
                if data and data != PAYLOAD:
                    raise AssertionError('unexpected push payload')
                received = data == PAYLOAD
            except (TimeoutError, ConnectionResetError, ConnectionRefusedError):
                if data:
                    raise AssertionError('partial unsolicited delivery')
                received = False
            result.setdefault(route, {})[protocol] = received
        return result

    def close(self):
        self.stack.close()
        self.old.clear()


def client(family):
    if [os.readlink('/proc/self/ns/' + kind) for kind in ('user', 'net')] != sys.argv[3:5]:
        raise AssertionError('client entered unexpected user/network namespaces')
    _, loopback, gateway, external = addresses(family)
    bank = Clients(family, {'internal': (loopback, 8082),
                           'host_splice': (loopback, 18081),
                           'host_tap': (gateway, 8080), 'external_tap': (external, 8080)})
    caps = {line.split(':')[0]: line.split(':')[1].strip()
            for line in Path('/proc/self/status').read_text().splitlines()
            if line.startswith(('CapEff:', 'CapPrm:', 'CapBnd:', 'CapAmb:'))}
    if any(int(value, 16) for value in caps.values()):
        raise AssertionError('client retained capabilities')
    print('ready', flush=True)
    try:
        for command in sys.stdin:
            command = command.strip()
            if command == 'fresh':
                bank.close()
            if command == 'register-push':
                emit(connections=bank.register_push())
            elif command == 'receive-push':
                emit(connections=bank.receive_push())
            else:
                emit(connections=bank.sample(keep=command == 'open'))
    finally:
        bank.close()


def observe(process, command):
    process.stdin.write(command + '\n')
    process.stdin.flush()
    if not select.select([process.stdout], [], [], 6)[0]:
        raise AssertionError('client observation timeout')
    return json.loads(process.stdout.readline())['connections']


def check(observation, permitted, existing):
    for route, protocols in observation.items():
        expected = permitted[route] if isinstance(permitted, dict) else permitted or route == 'internal'
        for values in protocols.values():
            if values['fresh'] != expected or (existing and values['existing'] != expected):
                raise AssertionError('unexpected connectivity: ' + json.dumps(observation))


def probe(pasta, family, watchdog_fault=None, automatic=False, kernel_fault=None, push=False):
    require_private_namespace()
    _, loopback, gateway, external = addresses(family)
    flag = '-4' if family == 'ipv4' else '-6'
    prefix = '24' if family == 'ipv4' else '64'
    host = '198.18.0.1' if family == 'ipv4' else '2001:db8::1'
    inner = '198.18.0.2' if family == 'ipv4' else '2001:db8::2'
    tail = [] if family == 'ipv4' else ['nodad']
    run('ip', 'link', 'set', 'lo', 'up')
    run('ip', 'link', 'add', 'probe0', 'type', 'dummy')
    run('ip', 'link', 'set', 'probe0', 'up')
    run('ip', flag, 'addr', 'add', host + '/' + prefix, 'dev', 'probe0', *tail)
    run('ip', flag, 'addr', 'add', external + '/' + prefix, 'dev', 'lo', *tail)
    if family == 'ipv6':
        run('ip', '-6', 'addr', 'add', 'fe80::1/64', 'dev', 'probe0', 'nodad')
    run('ip', flag, 'route', 'add', 'default', 'via', gateway, 'dev', 'probe0')
    processes = []
    published = Clients(family, {'publication': (loopback, 18080)})
    def launch(argv, **options):
        process = subprocess.Popen(argv, stdout=subprocess.PIPE, text=True, **options)
        processes.append(process)
        return process
    try:
        push_helper = str(Path(__file__).with_name('push_paths.py'))
        host_servers = []
        for address in (loopback, external):
            server = launch([sys.executable, push_helper if push else HERE, 'server', family, address, '8080'])
            host_servers.append(server)
            ready(server)
        argv = [pasta, '-f', flag, '--config-net', *loopback_options(), '-i', 'probe0',
                '-a', inner, '-g', gateway,
                '-t', loopback + '/18080:8080', '-u', loopback + '/18080:8080',
                '-T', '18081:8080', '-U', '18081:8080',
                '--', sys.executable, push_helper if push else HERE, 'inner' if push else 'guarded-inner' if watchdog_fault or kernel_fault else 'inner', family]
        emit(pasta_argv=argv)
        pasta_process = launch(argv)
        if not select.select([pasta_process.stdout], [], [], 10)[0]:
            raise AssertionError('pasta readiness timeout')
        line = pasta_process.stdout.readline()
        if not line:
            raise AssertionError('pasta exited before inner service readiness')
        identity = json.loads(line)['netns']
        enter = namespace_entry(identity)
        user = os.readlink('/proc/' + enter[2] + '/ns/user')
        app_enter = enter.copy()
        if user != os.readlink('/proc/self/ns/user'):
            app_enter += ['--user', '--preserve-credentials']
        inner_client = launch(app_enter + ['setpriv', '--bounding-set=-all', '--inh-caps=-all',
                              '--ambient-caps=-all', '--no-new-privs', sys.executable, HERE, 'client', family, user, identity],
                              stdin=subprocess.PIPE)
        ready(inner_client)
        def measure(phase, permitted, command):
            observations = {**published.sample(keep=command == 'open'), **observe(inner_client, command)}
            emit(phase=phase, connections=observations)
            check(observations, permitted, existing=command != 'fresh')
        if kernel_fault:
            measure('startup_guard_before_health_renewer', False, 'fresh')
            helper = str(Path(__file__).with_name('health_lease.py'))
            controller = launch(enter + [sys.executable, helper])
            ready(controller)
            measure('health_prepared_under_guard', False, 'fresh')
            nft(enter, 'delete table inet recovery_guard\n')
        elif watchdog_fault:
            measure('startup_guard_before_controller', False, 'fresh')
            helper = str(Path(__file__).with_name('watchdog.py'))
            controller = launch(enter + [sys.executable, helper, 'controller'])
            ready(controller)
            fd = controller.stdout.fileno()
            watcher = launch(enter + [sys.executable, helper, 'monitor', str(fd)], pass_fds=(fd,))
            ready(watcher)
            measure('prepared_but_not_opened', False, 'fresh')
            if watcher.poll() is not None:
                raise AssertionError('watcher failed before fault injection')
            if automatic:
                selector = 'ip' if family == 'ipv4' else 'ip6'
                datatype = 'ipv4_addr' if family == 'ipv4' else 'ipv6_addr'
                nft(enter, f'''table inet retained_lease {{
 set allowed {{ type {datatype}; flags timeout; elements = {{ {external} timeout 2s }}; }}
 chain output {{ type filter hook output priority 0; policy accept;
  ct state established accept
  {selector} daddr @allowed meta l4proto {{ tcp, udp }} th dport 8080 accept
  {selector} daddr {external} meta l4proto {{ tcp, udp }} th dport 8080 drop
 }}
}}''')
                lease_applied = time.monotonic()
            nft(enter, 'delete table inet recovery_guard\n')
        else:
            controller = launch(enter + [sys.executable, HERE, 'controller', family], stdin=subprocess.PIPE)
            ready(controller)
            if push:
                nft(enter, 'delete table inet recovery_guard\n')
        measure('permitted', True, 'open')
        def push_round(phase, permitted):
            from push_paths import acknowledgement, inner_pid
            for fixture in host_servers:
                fixture.send_signal(signal.SIGUSR1)
                acknowledgement(fixture)
            os.kill(inner_pid(identity), signal.SIGUSR1)
            acknowledgement(pasta_process)
            observations = {**published.receive_push(), **observe(inner_client, 'receive-push')}
            emit(phase=phase, server_push=observations)
            for route, protocols in observations.items():
                if any(value != (permitted or route == 'internal') for value in protocols.values()):
                    raise AssertionError('unexpected unsolicited delivery: ' + json.dumps(observations))
        if push:
            published.register_push()
            observe(inner_client, 'register-push')
            push_round('unsolicited_before_fault', True)
        if kernel_fault:
            if kernel_fault == 'stop':
                controller.send_signal(signal.SIGSTOP)
                stopped_by = time.monotonic() + 1
                while '\nState:\tT' not in Path('/proc/' + str(controller.pid) + '/status').read_text():
                    if time.monotonic() >= stopped_by:
                        raise AssertionError('health renewer did not stop')
                    time.sleep(.01)
            elif kernel_fault == 'kill':
                controller.kill()
                controller.wait(timeout=3)
            else:
                raise ValueError('unknown kernel gate fault')
            time.sleep(2.3)
            if push:
                push_round('unsolicited_after_kernel_expiry', False)
            measure('kernel_expiry_without_guard_writer', False, 'sample')
            state = subprocess.run(enter + ['nft', '-j', 'list', 'set', 'inet', 'health_lease', 'live'],
                                   capture_output=True, text=True, check=True, timeout=3)
            live = next(item['set'] for item in json.loads(state.stdout)['nftables'] if 'set' in item)
            if live.get('elem'):
                raise AssertionError('kernel health lease remained live')
            if pasta_process.poll() is not None:
                raise AssertionError('pasta exited instead of kernel blocking')
            emit(fault=kernel_fault, live_set=live,
                 scope='kernel liveness expiry on TAP/splice/publication, internal traffic preserved; no health evaluation protocol, renewal races or recovery proof')
            return
        if watchdog_fault:
            injected = time.monotonic()
            if watchdog_fault == 'stop':
                controller.send_signal(signal.SIGSTOP)
            elif watchdog_fault == 'kill':
                controller.kill()
                controller.wait(timeout=3)
            else:
                raise ValueError('unknown fault')
            if not select.select([watcher.stdout], [], [], 5)[0]:
                raise AssertionError('watcher failed to report completed block within 5s')
            event = json.loads(watcher.stdout.readline())
            elapsed = event['blocked_monotonic'] - injected
            if not 0 <= elapsed <= 5:
                raise AssertionError('block was premature or exceeded probe deadline')
            expected_reason = 'heartbeat_timeout' if watchdog_fault == 'stop' else 'heartbeat_eof'
            if event['reason'] != expected_reason:
                raise AssertionError('unexpected monitor trigger')
            watcher.wait(timeout=3)
            if watcher.returncode != 0:
                raise AssertionError('watcher exited unsuccessfully')
            emit(fault=watchdog_fault, seconds_to_block=elapsed, monitor=event)
        else:
            controller.kill()
            controller.wait(timeout=3)
            emit(controller_exit=controller.returncode)
            nft(enter, table('recovery_guard', 'drop'))
        if push:
            push_round('unsolicited_after_independent_guard', False)
            measure('controller_fault_guard_installed', False, 'sample')
            if pasta_process.poll() is not None:
                raise AssertionError('pasta exited instead of guard blocking')
            emit(scope='independent guard bidirectional push on fixed probe ports; no general production policy or recovery proof')
            return
        measure('controller_fault_guard_installed', False, 'sample')
        # Replacing the working rules must not remove the independent guard.
        if automatic:
            time.sleep(max(0, lease_applied + 2.2 - time.monotonic()))
            nft(enter, 'delete table inet recovery_policy\n')
            replacement = launch(enter + [sys.executable, helper, 'controller'])
            ready(replacement)
            replacement_fd = replacement.stdout.fileno()
            replacement_watcher = launch(enter + [sys.executable, helper, 'monitor', str(replacement_fd)],
                                         pass_fds=(replacement_fd,))
            ready(replacement_watcher)
            lease_state = subprocess.run(enter + ['nft', '-j', 'list', 'set', 'inet', 'retained_lease', 'allowed'],
                                         capture_output=True, text=True, check=True, timeout=3)
            retained = next(item['set'] for item in json.loads(lease_state.stdout)['nftables'] if 'set' in item)
            emit(retained_lease=retained, replacement_pid=replacement.pid)
            if retained.get('elem'):
                raise AssertionError('expired synthetic DNS grant survived recovery')
        else:
            nft(enter, 'delete table inet recovery_policy\n' + table('recovery_policy', 'accept'))
        measure('rebuilt_under_guard', False, 'sample')
        counters = subprocess.run(enter + ['nft', '-j', 'list', 'table', 'inet', 'recovery_guard'],
                                  capture_output=True, text=True, check=True, timeout=3)
        emit(guard_ruleset=json.loads(counters.stdout))
        nft(enter, 'delete table inet recovery_guard\n')
        published.close()
        if automatic:
            expected = {route: route != 'external_tap' for route in
                        ('publication', 'internal', 'host_splice', 'host_tap', 'external_tap')}
            measure('replacement_ready_reopen_expired_ip_denied', expected, 'fresh')
            # A second failure proves monitoring was restored too, not just forwarding.
            second_injected = time.monotonic()
            replacement.kill()
            replacement.wait(timeout=3)
            if not select.select([replacement_watcher.stdout], [], [], 5)[0]:
                raise AssertionError('replacement watcher failed to block')
            second = json.loads(replacement_watcher.stdout.readline())
            replacement_watcher.wait(timeout=3)
            if second['reason'] != 'heartbeat_eof' or replacement_watcher.returncode:
                raise AssertionError('replacement watcher reported wrong failure')
            if not 0 <= second['blocked_monotonic'] - second_injected <= 5:
                raise AssertionError('replacement block was premature or late')
            emit(second_monitor=second)
            measure('replacement_failure_blocked_again', False, 'fresh')
        else:
            measure('explicit_reopen', True, 'fresh')
        if pasta_process.poll() is not None:
            raise AssertionError('pasta did not survive the experiment')
        emit(scope=('one automated controller/watcher replacement and second fault; retained synthetic DNS lease expires; no real DNS, monitor-failure, arbitrary races or complete production recovery proof' if automatic else
                    'heartbeat watchdog and pre-service guard subset; experimental 1s timeout/5s observation bound; no production readiness protocol, monitor failure or DNS expiry proof'
                    if watchdog_fault else 'controller SIGKILL, independent guard and staged rebuild; TCP/UDP TAP/splice/publication subset; no automatic watchdog, startup, DNS expiry or complete gate'))
    finally:
        published.close()
        for process in reversed(processes):
            stop_process(process)


if __name__ == '__main__':
    require_private_namespace()
    mode, family = sys.argv[1:3]
    if mode == 'server':
        serve(family, sys.argv[3], int(sys.argv[4]))
    elif mode in ('inner', 'guarded-inner'):
        if mode == 'guarded-inner':
            nft([], table('recovery_guard', 'drop'))
        _, loopback, _, _ = addresses(family)
        internal = subprocess.Popen([sys.executable, HERE, 'server', family, loopback, '8082'],
                                    stdout=subprocess.PIPE, text=True)
        try:
            ready(internal)
            serve(family, announce_namespace=True)
        finally:
            stop_process(internal)
    elif mode == 'client':
        client(family)
    elif mode == 'controller':
        nft([], table('recovery_policy', 'accept'))
        print('ready', flush=True)
        sys.stdin.read()
    else:
        raise SystemExit('unknown mode')
