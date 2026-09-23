"""Keep newly timed grants unreachable until their application delay is checked."""
import json
import math
from pathlib import Path
import subprocess
import sys
import time

from dynamic import emit
from guard import require_private_namespace
from lease_failure import ready, stop_process
from recovery import Clients, nft
from worker import run

HERE = str(Path(__file__).resolve())


def stage(address, deadline, delay):
    started = time.monotonic()
    reserve = .5
    remaining = math.floor((deadline - started - reserve) * 1000)
    if remaining <= 0:
        raise AssertionError('no finite candidate lifetime remains')
    # Delay models descheduling between calculating the timeout and sending it.
    time.sleep(delay)
    nft([], f'add element inet staged candidate {{ {address} timeout {remaining}ms }}\n')
    elapsed = time.monotonic() - started
    emit(acceptable=elapsed <= reserve, seconds=elapsed, reserve=reserve)


def activate(selector):
    # Referencing the existing set must not reinsert its elements or reset timeouts.
    nft([], f'''flush chain inet staged active
add rule inet staged active {selector} daddr @candidate meta l4proto {{ tcp, udp }} th dport 8080 accept
''')


def probe(family):
    require_private_namespace()
    run('ip', 'link', 'set', 'lo', 'up')
    addresses = ('127.0.0.2', '127.0.0.3') if family == 'ipv4' else ('::1', '::2')
    selector, datatype = ('ip', 'ipv4_addr') if family == 'ipv4' else ('ip6', 'ipv6_addr')
    if family == 'ipv6':
        run('ip', '-6', 'addr', 'add', '::2/128', 'dev', 'lo', 'nodad')
    children = []
    bank = Clients(family, {'candidate': (addresses[0], 8080), 'stable': (addresses[1], 8080)})

    def measure(phase, permitted, keep=False, existing=None):
        result = bank.sample(keep=keep)
        emit(phase=phase, connections=result)
        for label, protocols in result.items():
            for values in protocols.values():
                if values['fresh'] != (permitted if label == 'candidate' else True):
                    raise AssertionError('unexpected new flow: ' + phase)
                if label == 'candidate' and existing is not None and values['existing'] != existing:
                    raise AssertionError('unexpected existing flow: ' + phase)

    def prepare(delay):
        deadline = time.monotonic() + 3
        writer = subprocess.Popen([sys.executable, HERE, 'stage', addresses[0], str(deadline), str(delay)],
                                  stdout=subprocess.PIPE, text=True)
        children.append(writer)
        stdout, _ = writer.communicate(timeout=4)
        if writer.returncode:
            raise AssertionError('candidate writer failed')
        result = json.loads(stdout)
        emit(staging=result, deadline=deadline)
        return deadline, result['acceptable']

    def wait_until(deadline):
        time.sleep(max(0, deadline - time.monotonic()))

    try:
        for address in addresses:
            server = subprocess.Popen([sys.executable, HERE, 'server', family, address],
                                      stdout=subprocess.PIPE, text=True)
            children.append(server)
            ready(server)
        nft([], f'''table inet staged {{
 set candidate {{ type {datatype}; flags timeout; }}
 chain active {{ }}
 chain output {{ type filter hook output priority 0; policy accept;
  ct state established accept
  {selector} daddr {addresses[1]} meta l4proto {{ tcp, udp }} th dport 8080 accept
  jump active
  meta l4proto {{ tcp, udp }} th dport 8080 drop
 }}
}}''')
        measure('initial', False)
        deadline, acceptable = prepare(.7)
        if acceptable:
            raise AssertionError('delayed application was accepted')
        measure('late_writer_completed_but_candidate_not_activated', False)
        nft([], 'flush set inet staged candidate\n')

        deadline, acceptable = prepare(0)
        if not acceptable:
            raise AssertionError('normal application exceeded fixture budget')
        measure('before_activation', False)
        activate(selector)
        measure('active', True, keep=True, existing=True)
        wait_until(deadline + .1)
        measure('expired_without_writer', False, existing=True)
        bank.close()
        nft([], 'flush chain inet staged active\nflush set inet staged candidate\n')

        deadline, acceptable = prepare(0)
        if not acceptable:
            raise AssertionError('normal application exceeded fixture budget')
        wait_until(deadline + .1)
        activate(selector)
        measure('late_activation_does_not_resurrect', False)
    finally:
        bank.close()
        for child in reversed(children):
            stop_process(child)
    emit(family=family, scope='synthetic grant staging and chain activation; not a DNS parser, TTL-zero policy or complete concurrent-update design')


if __name__ == '__main__':
    require_private_namespace()
    if len(sys.argv) == 4 and sys.argv[1] == 'server':
        from dynamic import serve
        serve(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 5 and sys.argv[1] == 'stage':
        stage(sys.argv[2], float(sys.argv[3]), float(sys.argv[4]))
    else:
        raise SystemExit('Use run.py')
