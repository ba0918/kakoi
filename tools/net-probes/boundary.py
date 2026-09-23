"""Measure outbound TAP filtering independently of internal loopback traffic."""
import errno
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
from guard import require_private_namespace
from hooks import exchange, host_servers
from worker import run

HERE = Path(__file__).resolve()


def addresses(label):
    if label == 'ipv4':
        return socket.AF_INET, '127.0.0.1', '198.18.0.254', '198.19.0.1'
    if label == 'ipv6':
        return socket.AF_INET6, '::1', '2001:db8::fe', '2001:db8:1::1'
    raise ValueError('unknown family')


def namespace_entry(identity):
    if identity in (os.readlink('/proc/self/ns/net'), os.environ['KAKOI_PROBE_HOST_NET']):
        raise RuntimeError('expected a separate inner network namespace')
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if os.readlink(entry / 'ns/net') == identity:
                return ['nsenter', '--target', entry.name, '--net']
        except (FileNotFoundError, ProcessLookupError):
            continue
    raise RuntimeError('inner namespace not visible')


def client(label, expected_user, expected_net):
    namespaces = {kind: os.readlink("/proc/self/ns/" + kind) for kind in ("user", "net")}
    if namespaces != {"user": expected_user, "net": expected_net}:
        raise RuntimeError("client did not enter the service user/network namespaces")
    family, loopback, gateway, external = addresses(label)
    capabilities = {line.split(':')[0]: line.split(':')[1].strip()
                    for line in Path('/proc/self/status').read_text().splitlines()
                    if line.startswith(('CapEff:', 'CapPrm:', 'CapBnd:', 'CapAmb:', 'NoNewPrivs:'))}
    if any(int(capabilities[k], 16) != 0 for k in ('CapEff', 'CapPrm', 'CapBnd', 'CapAmb')):
        raise RuntimeError('client retained capabilities')
    if capabilities['NoNewPrivs'] != '1':
        raise RuntimeError('client can gain privileges on exec')
    attempts = []
    for argv in (['nft', 'flush', 'ruleset'],
                 ['ip', 'link', 'set', 'lo', 'down'],
                 ['unshare', '--user', '--map-root-user', 'nft', 'flush', 'ruleset']):
        result = subprocess.run(argv, capture_output=True, text=True, timeout=3,
                                env={**os.environ, "LC_ALL": "C"})
        attempts.append({'argv': argv, 'exit': result.returncode, 'stderr': result.stderr})
        if result.returncode == 0 or 'Operation not permitted' not in result.stderr:
            raise RuntimeError('privilege attempt did not fail with permission denial: ' + str(attempts[-1]))
    try:
        raw = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(3))
    except OSError as error:
        if error.errno != errno.EPERM:
            raise
    else:
        raw.close()
        raise RuntimeError('unprivileged client opened a raw packet socket')
    observations = {name: {protocol: exchange(family, address, 8081, protocol)
                           for protocol in ('tcp', 'udp')}
                    for name, address in [('internal', loopback), ('host_loopback', gateway),
                                          ('host_nonloopback', external)]}
    print(json.dumps({'namespaces': namespaces, 'capabilities': capabilities, 'denied_operations': attempts,
                      'raw_socket_denied': True, 'connections': observations}), flush=True)


def probe(pasta, label):
    require_private_namespace()
    family, loopback, gateway, external = addresses(label)
    version = '-4' if label == 'ipv4' else '-6'
    prefix = '24' if label == 'ipv4' else '64'
    host_address = '198.18.0.1' if label == 'ipv4' else '2001:db8::1'
    inner_address = '198.18.0.2' if label == 'ipv4' else '2001:db8::2'
    run('ip', 'link', 'set', 'lo', 'up')
    run('ip', 'link', 'add', 'probe0', 'type', 'dummy')
    run('ip', 'link', 'set', 'probe0', 'up')
    tail = [] if label == 'ipv4' else ['nodad']
    run('ip', version, 'addr', 'add', host_address + '/' + prefix, 'dev', 'probe0', *tail)
    run('ip', version, 'addr', 'add', external + '/' + prefix, 'dev', 'lo', *tail)
    if label == 'ipv6':
        run('ip', '-6', 'addr', 'add', 'fe80::1/64', 'dev', 'probe0', 'nodad')
    run('ip', version, 'route', 'add', 'default', 'via', gateway, 'dev', 'probe0')
    fixtures = []
    process = None
    try:
        for address in (loopback, external):
            fixtures.append(host_servers(family, address))
        # Disable both directions of socket forwarding: clients must use TAP.
        argv = [pasta, '-f', version, '--config-net', '-i', 'probe0',
                '-a', inner_address, '-g', gateway,
                '-t', 'none', '-u', 'none', '-T', 'none', '-U', 'none',
                '--', sys.executable, str(HERE), 'server', label]
        print(json.dumps({'pasta_argv': argv}), flush=True)
        process = subprocess.Popen(argv, stdout=subprocess.PIPE, text=True)
        if not select.select([process.stdout], [], [], 10)[0]:
            raise RuntimeError('inner service readiness timeout')
        line = process.stdout.readline()
        if not line:
            raise RuntimeError('pasta exited before readiness')
        inner_net = json.loads(line)['netns']
        enter = namespace_entry(inner_net)
        inner_user = os.readlink('/proc/' + enter[2] + '/ns/user')
        app_enter = enter.copy()
        if inner_user != os.readlink('/proc/self/ns/user'):
            # Dropping capability bits in the parent does not remove the
            # namespace owner's authority over child user namespaces.
            app_enter += ['--user', '--preserve-credentials']
        for query in (['ip', '-j', 'addr'], ['ip', version, '-j', 'route']):
            result = subprocess.run(enter + query, capture_output=True, text=True, check=True, timeout=3)
            print(json.dumps({'query': query, 'result': json.loads(result.stdout)}), flush=True)
        for index, permitted in enumerate((False, True, False)):
            family_keyword = 'ip' if label == 'ipv4' else 'ip6'
            allowed = '\n'.join(f'{family_keyword} daddr {address} {protocol} dport 8081 counter accept'
                                for address in (gateway, external) for protocol in ('tcp', 'udp')) if permitted else ''
            rules = ('delete table inet boundary_probe\n' if index else '') + f'''table inet boundary_probe {{
 chain output {{ type filter hook output priority 0; policy drop;
 oifname "lo" counter accept
 meta l4proto ipv6-icmp counter accept
 {allowed}
 counter drop
 }}
}}'''
            # One nft transaction avoids a delete/add gap. Startup and recovery
            # races of the future product still require separate verification.
            subprocess.run(enter + ['nft', '-f', '-'], input=rules, text=True, check=True, timeout=3)
            result = subprocess.run(app_enter + ['setpriv', '--bounding-set=-all', '--inh-caps=-all',
                                            '--ambient-caps=-all', '--no-new-privs',
                                            sys.executable, str(HERE), 'client', label, inner_user, inner_net],
                                    capture_output=True, text=True, timeout=8)
            if result.returncode:
                raise RuntimeError('client failed: ' + result.stdout + result.stderr)
            observation = json.loads(result.stdout)
            ruleset = subprocess.run(enter + ['nft', '-j', 'list', 'table', 'inet', 'boundary_probe'],
                                     capture_output=True, text=True, check=True, timeout=3)
            parsed_ruleset = json.loads(ruleset.stdout)
            print(json.dumps({'permitted': permitted, **observation,
                              'ruleset': parsed_ruleset}), flush=True)
            counters = [part['counter']['packets']
                        for item in parsed_ruleset['nftables'] if 'rule' in item
                        for part in item['rule']['expr'] if 'counter' in part]
            if not counters or counters[0] == 0:
                raise RuntimeError('internal loopback rule was not exercised')
            if permitted:
                if len(counters) != 7 or any(value == 0 for value in counters[2:6]):
                    raise RuntimeError('not every non-loopback permit rule was exercised')
            elif counters[-1] == 0:
                raise RuntimeError('non-loopback drop rule was not exercised')
            for destination, connections in observation['connections'].items():
                expected = destination == 'internal' or permitted
                if set(connections) != {'tcp', 'udp'} or any(v is not expected for v in connections.values()):
                    raise RuntimeError('unexpected connectivity for ' + destination)
        for _, _, _, errors in fixtures:
            if errors:
                raise RuntimeError('host fixture failed: ' + str(errors))
        print(json.dumps({'scope': 'outbound TAP and capability boundary subset', 'full_E2': False}), flush=True)
    finally:
        if process:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for servers, stop, thread, _ in fixtures:
            stop.set()
            thread.join(timeout=1)
            for server in servers:
                server.close()


if __name__ == '__main__':
    require_private_namespace()
    mode, label = sys.argv[1:3]
    if mode == 'server':
        family, loopback, _, _ = addresses(label)
        fixtures = host_servers(family, loopback)
        print(json.dumps({'netns': os.readlink('/proc/self/ns/net')}), flush=True)
        while True:
            signal.pause()
    elif mode == 'client':
        client(label, *sys.argv[3:5])
    else:
        raise SystemExit('unknown internal mode')
