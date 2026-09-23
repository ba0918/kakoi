"""Apply the tested bwrap boundary to pasta's actual control socket."""
import json
import os
from pathlib import Path
import select
import subprocess
import sys

from guard import require_private_namespace
from control_isolation import run_app
from dynamic import emit
from udp_reassign import configure


def serve():
    require_private_namespace()
    print('ready', flush=True)
    request = json.loads(sys.stdin.readline())
    parent = {kind: os.readlink('/proc/self/ns/' + kind) for kind in ('user', 'mnt', 'pid', 'net')}
    run_app(parent, request['inode'], request['control'])
    print('app-checked', flush=True)
    # Keep pasta alive for the trusted controller's post-check operation.
    sys.stdin.readline()


def exercise(process, pesto, control, family):
    rows = [line.split() for line in Path('/proc/net/unix').read_text().splitlines()[1:]]
    matches = [row[6] for row in rows if len(row) >= 8 and row[7] == control]
    if len(matches) != 1:
        raise AssertionError('could not identify the real configuration socket inode')
    address = '127.0.0.1' if family == 'ipv4' else '::1'
    mapping = address + '/18080:8080'
    configure(pesto, control, '-A', mapping)
    process.stdin.write(json.dumps({'inode': matches[0], 'control': control}) + '\n')
    process.stdin.flush()
    # TextIO may prefetch lines, so use a single bounded subprocess-side result
    # line followed by its marker, rather than repeatedly selecting buffered data.
    if not select.select([process.stdout], [], [], 15)[0]:
        raise AssertionError('app isolation check timed out')
    observation = json.loads(process.stdout.readline())
    emit(app_check=observation)
    if observation.get('bwrap_exit') != 0 or process.stdout.readline().strip() != 'app-checked':
        raise AssertionError('app isolation failed')
    configure(pesto, control, '-D', mapping)
    state = subprocess.run([pesto, control], capture_output=True, text=True, timeout=3)
    state.check_returncode()
    if '18080' in state.stdout:
        raise AssertionError('trusted rule removal did not take effect')
    emit(family=family, trusted_pesto_before_after=True,
         scope='real pasta control socket hidden by bwrap; no complete escape or supervision proof')


if __name__ == '__main__':
    require_private_namespace()
    if sys.argv[1] != 'serve':
        raise SystemExit('Use run.py')
    serve()
