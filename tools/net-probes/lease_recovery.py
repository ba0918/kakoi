"""Restore synthetic validated IP grants using their original monotonic deadlines."""
import json
import math
import select
import subprocess
import sys
import time
from pathlib import Path

from dynamic import emit
from guard import require_private_namespace
from lease_failure import ready, stop_process
from recovery import Clients, nft, table
from worker import run

HERE = str(Path(__file__).resolve())


def restore(grants):
    started = time.monotonic()
    reserve = .5
    # nft timeouts start at application, not at this calculation. Deduct a
    # measured application budget and refuse readiness if it was exceeded.
    remaining = [(address, math.floor((deadline - started - reserve) * 1000))
                 for address, deadline in grants.items()]
    elements = ', '.join(f'{address} timeout {ms}ms' for address, ms in remaining if ms > 0)
    rules = 'flush set inet lease_restore allowed\n'
    if elements:
        rules += 'add element inet lease_restore allowed { ' + elements + ' }\n'
    nft([], rules)
    elapsed = time.monotonic() - started
    if elapsed > reserve:
        raise AssertionError('nft apply exceeded reserved time; keep recovery guard closed')
    emit(restored=[address for address, ms in remaining if ms > 0],
         remaining_ms=dict(remaining), apply_seconds=elapsed, reserved_seconds=reserve)


def snapshot():
    result = subprocess.run(['nft', '-j', 'list', 'set', 'inet', 'lease_restore', 'allowed'],
                            capture_output=True, text=True, check=True, timeout=3)
    parsed = json.loads(result.stdout)
    emit(allowed_set=parsed)
    return next(item['set'] for item in parsed['nftables'] if 'set' in item)


def preserve(family):
    selector = 'ip' if family == 'ipv4' else 'ip6'
    nft([], f'''flush chain inet lease_restore output
add rule inet lease_restore output ct state established accept
add rule inet lease_restore output {selector} daddr @allowed meta l4proto {{ tcp, udp }} th dport 8080 accept
add rule inet lease_restore output meta l4proto {{ tcp, udp }} th dport 8080 drop
''')
    result = subprocess.run(['nft', '-j', 'list', 'set', 'inet', 'lease_restore', 'allowed'],
                            capture_output=True, text=True, check=True, timeout=3)
    current = next(item['set'] for item in json.loads(result.stdout)['nftables'] if 'set' in item)
    emit(restored=[item['elem']['val'] for item in current.get('elem', [])],
         preserved_set_handle=current['handle'])


def probe(family, preserve_kernel=False):
    require_private_namespace()
    run('ip', 'link', 'set', 'lo', 'up')
    addresses = ('127.0.0.2', '127.0.0.3') if family == 'ipv4' else ('::1', '::2')
    if family == 'ipv6':
        run('ip', '-6', 'addr', 'add', '::2/128', 'dev', 'lo', 'nodad')
    selector = 'ip' if family == 'ipv4' else 'ip6'
    datatype = 'ipv4_addr' if family == 'ipv4' else 'ipv6_addr'
    children = []
    bank = Clients(family, {'short': (addresses[0], 8080), 'long': (addresses[1], 8080)})
    def launch(mode, *args, **kw):
        process = subprocess.Popen([sys.executable, HERE, mode, *args], stdout=subprocess.PIPE,
                                   text=True, **kw)
        children.append(process)
        return process
    def measure(phase, expected, keep=False, existing=None):
        observed = bank.sample(keep=keep)
        emit(phase=phase, connections=observed)
        for name, protocols in observed.items():
            for values in protocols.values():
                if values['fresh'] != expected[name]:
                    raise AssertionError('fresh grant result differs: ' + phase)
                if existing is not None and values['existing'] != existing:
                    raise AssertionError('existing flow result differs: ' + phase)
    def wait_until(deadline):
        time.sleep(max(0, deadline - time.monotonic()))
    try:
        for address in addresses:
            ready(launch('server', family, address))
        nft([], f'''table inet lease_restore {{
 set allowed {{ type {datatype}; flags timeout; }}
 chain output {{ type filter hook output priority 0; policy accept;
  ct state established accept
  {selector} daddr @allowed meta l4proto {{ tcp, udp }} th dport 8080 accept
  meta l4proto {{ tcp, udp }} th dport 8080 drop
 }}
}}''')
        measure('no_grants', {'short': False, 'long': False})
        origin = time.monotonic()
        grants = {addresses[0]: origin + 2, addresses[1]: origin + 8}
        payload = json.dumps(grants)
        controller = launch('controller', payload, stdin=subprocess.PIPE)
        ready(controller)
        emit(original_deadlines=grants)
        measure('initial_grants', {'short': True, 'long': True}, keep=True, existing=True)
        initial_set = snapshot()
        controller.kill()
        controller.wait(timeout=3)
        nft([], table('recovery_guard', 'drop'))
        wait_until(grants[addresses[0]] + .2)
        restarted = launch('preserve', family) if preserve_kernel else launch('restore', payload)
        if not select.select([restarted.stdout], [], [], 4)[0]:
            raise AssertionError('restore did not finish')
        restored = json.loads(restarted.stdout.readline())
        restarted.wait(timeout=3)
        if restarted.returncode or restored['restored'] != [addresses[1]]:
            raise AssertionError('expired grant restored or live grant lost')
        emit(restoration=restored)
        if preserve_kernel and restored['preserved_set_handle'] != initial_set['handle']:
            raise AssertionError('restoration replaced the kernel permit set')
        snapshot()
        measure('restored_under_guard', {'short': False, 'long': False}, existing=False)
        nft([], 'delete table inet recovery_guard\n')
        bank.close()
        measure('reopened', {'short': False, 'long': True})
        wait_until(grants[addresses[1]] + .2)
        snapshot()
        measure('original_long_deadline_passed', {'short': False, 'long': False})
        emit(family=family, preserved_kernel_set=preserve_kernel,
             scope='synthetic grants; kernel set preservation or conservative reconstruction; no existing-flow resumption, DNS parser, pasta integration or complete recovery proof')
    finally:
        bank.close()
        for process in reversed(children):
            stop_process(process)


if __name__ == '__main__':
    require_private_namespace()
    mode = sys.argv[1]
    if mode == 'server':
        from dynamic import serve
        serve(sys.argv[2], sys.argv[3])
    elif mode == 'preserve':
        preserve(sys.argv[2])
    elif mode in ('controller', 'restore'):
        if mode == 'controller':
            # Keep the completion line separate from the controller readiness pipe.
            import contextlib
            with contextlib.redirect_stdout(sys.stderr):
                restore(json.loads(sys.argv[2]))
            print('ready', flush=True)
            sys.stdin.read()
        else:
            restore(json.loads(sys.argv[2]))
    else:
        raise SystemExit('Use run.py')
