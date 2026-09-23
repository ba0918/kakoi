"""Measure bwrap parent-death cleanup including a descendant in another session."""
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time

from dynamic import emit, endpoint, new_socket
from guard import require_private_namespace
from lease_failure import ready, stop_process
from recovery import exchange
from worker import run
import socket

HERE = str(Path(__file__).resolve())


def read_event(process):
    if not select.select([process.stdout], [], [], 5)[0]:
        raise AssertionError('process readiness timeout')
    line = process.stdout.readline()
    if not line:
        raise AssertionError('process exited before readiness')
    return json.loads(line)


def members(identity):
    found = []
    for entry in Path('/proc').iterdir():
        if entry.name.isdigit():
            try:
                if os.readlink(entry / 'ns/pid') == identity:
                    found.append(int(entry.name))
            except (FileNotFoundError, ProcessLookupError):
                continue
    return found


def probe(mode):
    require_private_namespace()
    if os.getpid() != 1:
        raise AssertionError('probe must reap orphans as the private PID namespace init')
    run('ip', 'link', 'set', 'lo', 'up')
    supervisor = subprocess.Popen([sys.executable, HERE, 'supervisor', mode],
                                  stdout=subprocess.PIPE, text=True)
    try:
        event = read_event(supervisor)
        identity = event['pidns']
        if identity in (os.readlink('/proc/self/ns/pid'), os.environ['KAKOI_PROBE_HOST_PID']):
            raise AssertionError('fixture did not get a separate PID namespace')
        emit(start=event, members_before=members(identity))
        if not event['detached_session']:
            raise AssertionError('descendant did not enter a separate session')
        af, address = endpoint('ipv4')
        with new_socket(af, socket.SOCK_STREAM) as existing:
            existing.connect((address, 8080))
            if not exchange(existing):
                raise AssertionError('descendant service was not reachable')
            started = time.monotonic()
            if mode == 'parent-death':
                supervisor.kill()
                supervisor.wait(timeout=3)
            else:
                os.kill(event['fixture_outer_pid'], signal.SIGUSR1)
                supervisor.wait(timeout=3)
                if supervisor.returncode != 23:
                    raise AssertionError('main command exit status was not preserved')
            deadline = started + 3
            while members(identity):
                # bwrap's namespace init can be adopted by this private PID 1.
                # It has exited, but its zombie must still be reaped.
                try:
                    while os.waitpid(-1, os.WNOHANG)[0]:
                        pass
                except ChildProcessError:
                    pass
                if time.monotonic() >= deadline:
                    emit(survivors={pid: Path(f'/proc/{pid}/status').read_text().splitlines()[:8]
                                    for pid in members(identity)})
                    raise AssertionError('descendants survived PID namespace cleanup')
                time.sleep(.02)
            if exchange(existing):
                raise AssertionError('established service remained usable')
            with new_socket(af, socket.SOCK_STREAM) as fresh:
                try:
                    fresh.connect((address, 8080))
                except ConnectionRefusedError:
                    pass
                else:
                    raise AssertionError('descendant listener survived')
            emit(mode=mode, cleanup_seconds=time.monotonic() - started,
                 descendants_gone=True, old_and_new_service_unreachable=True,
                 supervisor_exit=supervisor.returncode,
                 scope='bwrap parent-death/main-exit PID cleanup including setsid descendant; no grace or terminal interaction proof')
    finally:
        stop_process(supervisor)


def supervisor(mode):
    argv = ['bwrap', '--ro-bind', '/', '/', '--unshare-all', '--share-net',
            '--proc', '/proc', '--dev', '/dev', '--cap-drop', 'ALL',
            '--die-with-parent', '--new-session', '--', sys.executable, HERE, 'fixture', mode]
    process = subprocess.Popen(argv, stdout=subprocess.PIPE, text=True)
    try:
        event = read_event(process)
        candidates = members(event['pidns'])
        matches = []
        for pid in candidates:
            try:
                argv = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
                if argv[0] == sys.executable.encode() and b'fixture' in argv and HERE.encode() in argv:
                    matches.append(pid)
            except FileNotFoundError:
                pass
        if len(matches) != 1:
            raise AssertionError('fixture PID was not uniquely visible')
        emit(**event, fixture_outer_pid=matches[0])
        raise SystemExit(process.wait())
    finally:
        stop_process(process)


def fixture():
    process = subprocess.Popen([sys.executable, str(Path(__file__).with_name('dynamic.py')), 'serve', 'ipv4'],
                               stdout=subprocess.PIPE, text=True, start_new_session=True)
    try:
        ready(process)
        signal.signal(signal.SIGUSR1, lambda *_: sys.exit(23))
        emit(pidns=os.readlink('/proc/self/ns/pid'),
             detached_session=os.getsid(process.pid) != os.getsid(0))
        while True:
            signal.pause()
    finally:
        # The scenario intentionally leaves the child to the namespace teardown.
        pass


if __name__ == '__main__':
    require_private_namespace()
    if sys.argv[1] == 'supervisor':
        supervisor(sys.argv[2])
    elif sys.argv[1] == 'fixture':
        fixture()
    else:
        raise SystemExit('Use run.py')
