"""Bounded PID-1 cleanup and terminal signals using a disposable bwrap sandbox."""
import json
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import sys
import time

from dynamic import emit
from guard import require_private_namespace
from process_guard import members

HERE = str(Path(__file__).resolve())
GRACE = 2.0


def linger(ready_fd=None):
    spawn_at = None
    def terminate(*_):
        nonlocal spawn_at
        if spawn_at is None:
            spawn_at = time.monotonic() + 1.0
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    signal.signal(signal.SIGTERM, terminate)
    if ready_fd is not None:
        os.write(ready_fd, b'1')
        os.close(ready_fd)
    spawned = False
    while True:
        if spawn_at is not None and not spawned and ready_fd is not None and time.monotonic() >= spawn_at:
            subprocess.Popen([sys.executable, HERE, 'late-child'], start_new_session=True)
            spawned = True
            print('LATE_CHILD_CREATED', flush=True)
        time.sleep(.01)


def app():
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    signal.signal(signal.SIGINT, lambda *_: None)
    read_fd, write_fd = os.pipe()
    child = subprocess.Popen([sys.executable, HERE, 'linger', str(write_fd)],
                             pass_fds=(write_fd,), start_new_session=True)
    os.close(write_fd)
    try:
        if not select.select([read_fd], [], [], 3)[0] or os.read(read_fd, 1) != b'1':
            raise AssertionError('linger readiness failed')
    finally:
        os.close(read_fd)
    print('MAIN_READY', flush=True)
    for line in sys.stdin:
        if line.strip() == 'exit':
            raise SystemExit(23)


def init(network_order=False):
    if os.getpid() != 1:
        raise AssertionError('cleanup init must be private PID 1')
    flags = {'term': False, 'interrupt': False, 'safety': False, 'guard_ack': None}
    signal.signal(signal.SIGTERM, lambda *_: flags.update(term=True))
    signal.signal(signal.SIGINT, lambda *_: flags.update(interrupt=True))
    signal.signal(signal.SIGUSR2, lambda *_: flags.update(safety=True))
    if network_order:
        signal.signal(signal.SIGUSR1, lambda *_: flags.update(guard_ack=time.monotonic()))
        signal.pthread_sigmask(signal.SIG_UNBLOCK, {signal.SIGUSR1})
    process = subprocess.Popen([sys.executable, HERE, 'app'])
    emit(init_pidns=os.readlink('/proc/self/ns/pid'))
    while process.poll() is None and not flags['term']:
        time.sleep(.005)
    result = process.returncode if process.returncode is not None else 143
    main_exit = process.returncode
    if network_order:
        print('NETWORK_BLOCK_REQUIRED', flush=True)
        guard_deadline = time.monotonic() + 4
        while flags['guard_ack'] is None:
            if time.monotonic() >= guard_deadline:
                os.kill(-1, signal.SIGKILL)
                raise SystemExit(125)
            time.sleep(.005)
    started = time.monotonic()
    deadline = started + GRACE
    flags['interrupt'] = False
    os.kill(-1, signal.SIGTERM)
    print('GRACE_READY', flush=True)
    while time.monotonic() < deadline:
        if flags['safety']:
            failed = subprocess.run(['nft', 'add', 'table', 'inet', 'shutdown_fault_probe'],
                                    capture_output=True, text=True, timeout=1, env={**os.environ, 'LC_ALL': 'C'})
            if failed.returncode == 0 or 'Operation not permitted' not in failed.stderr:
                raise AssertionError('expected real permission failure while installing guard')
            result = 125
            break
        if flags['interrupt']:
            break
        # Later SIGTERM and newly forked children do not reset this deadline.
        time.sleep(.005)
    os.kill(-1, signal.SIGKILL)
    while True:
        try:
            child_pid, status = os.waitpid(-1, 0)
            if child_pid == process.pid:
                main_exit = os.waitstatus_to_exitcode(status)
        except ChildProcessError:
            break
    emit(shutdown_result=result, grace_seconds=time.monotonic() - started,
         main_exit=main_exit, safety_failure=flags['safety'], interrupted=flags['interrupt'],
         grace_started=started, guard_ack=flags['guard_ack'])
    raise SystemExit(result)


def probe(mode):
    require_private_namespace()
    network_server = network_bank = None
    if mode == 'network-order':
        from recovery import Clients
        from lease_failure import ready
        subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True, timeout=2)
        network_server = subprocess.Popen([sys.executable, str(Path(__file__).with_name('dynamic.py')), 'serve', 'ipv4'], stdout=subprocess.PIPE, text=True)
        try:
            ready(network_server)
        except BaseException:
            from lease_failure import stop_process
            stop_process(network_server)
            raise
        network_bank = Clients('ipv4', {'service': ('127.0.0.1', 8080)})
    pid, master = pty.fork()
    if pid == 0:
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        os.execvp('bwrap', ['bwrap', '--ro-bind', '/', '/', '--unshare-all', '--share-net',
                           '--proc', '/proc', '--dev', '/dev', '--cap-drop', 'ALL',
                           '--die-with-parent', '--as-pid-1', '--', sys.executable, HERE, 'network-init' if mode == 'network-order' else 'init'])
    output = b''
    reaped = False
    def until(marker):
        nonlocal output
        deadline = time.monotonic() + 5
        while marker not in output:
            if not select.select([master], [], [], max(0, deadline - time.monotonic()))[0]:
                raise AssertionError('shutdown terminal timeout: ' + repr(output))
            try:
                chunk = os.read(master, 4096)
            except OSError as error:
                raise AssertionError('terminal closed early: ' + repr(output)) from error
            if not chunk:
                raise AssertionError('terminal EOF')
            output += chunk
    block_completed = None
    try:
        until(b'MAIN_READY')
        if network_bank is not None:
            baseline = network_bank.sample(keep=True)
            if not all(v['fresh'] and v['existing'] for v in baseline['service'].values()):
                raise AssertionError('network positive control failed')
        line = next(line for line in output.decode().splitlines() if line.startswith('{"init_pidns"'))
        identity = json.loads(line)['init_pidns']
        if identity in (os.readlink('/proc/self/ns/pid'), os.environ['KAKOI_PROBE_HOST_PID']):
            raise AssertionError('init is not in a nested private PID namespace')
        targets = []
        for candidate in members(identity):
            argv = Path(f'/proc/{candidate}/cmdline').read_bytes().split(b'\0')
            if argv[:3] == [sys.executable.encode(), HERE.encode(), b'network-init' if mode == 'network-order' else b'init']:
                targets.append(candidate)
        if len(targets) != 1:
            raise AssertionError('nested init not uniquely identified')
        if mode in ('interrupt-grace', 'safety-exit', 'network-order'):
            os.write(master, b'exit\n')
        else:
            os.kill(targets[0], signal.SIGTERM)
        if network_bank is not None:
            from recovery import nft, table
            until(b'NETWORK_BLOCK_REQUIRED')
            if b'GRACE_READY' in output:
                raise AssertionError('grace began before block acknowledgement')
            nft([], table('shutdown_network_guard', 'drop'))
            blocked = network_bank.sample()
            if any(v['fresh'] or v['existing'] for v in blocked['service'].values()):
                raise AssertionError('network guard left a flow open')
            block_completed = time.monotonic()
            os.kill(targets[0], signal.SIGUSR1)
        until(b'GRACE_READY')
        until(b'LATE_CHILD_CREATED')
        if mode == 'network-order':
            during = network_bank.sample()
            if any(v['fresh'] or v['existing'] for v in during['service'].values()):
                raise AssertionError('network reopened during cleanup grace')
        elif mode == 'safety-exit':
            os.kill(targets[0], signal.SIGUSR2)
        elif mode == 'interrupt-grace':
            os.write(master, b'\x03')
        else:
            os.kill(targets[0], signal.SIGTERM)
        until(b'"interrupted":')
        until(b'}\r\n')
        # Read the complete result line if the PTY split it across reads.
        while not output.decode().split('"shutdown_result":', 1)[-1].splitlines()[0].endswith('}'):
            if not select.select([master], [], [], 3)[0]:
                raise AssertionError('incomplete shutdown result')
            output += os.read(master, 4096)
        line = next(line for line in output.decode().splitlines() if '"shutdown_result"' in line)
        observed = json.loads(line[line.index('{'):])
        deadline = time.monotonic() + 3
        while True:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                reaped = True
                break
            if time.monotonic() >= deadline:
                raise AssertionError('bwrap did not finish')
            time.sleep(.01)
        expected = 125 if mode == 'safety-exit' else 23 if mode in ('interrupt-grace', 'network-order') else 143
        if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != expected or observed['shutdown_result'] != expected:
            raise AssertionError('exit priority did not survive actual signals')
        if network_bank is not None:
            if not block_completed <= observed['guard_ack'] <= observed['grace_started']:
                raise AssertionError('cleanup grace started before network block')
        elapsed = observed['grace_seconds']
        if observed['main_exit'] != (0 if mode == 'repeat-term' else 23):
            raise AssertionError('main command did not return the expected value')
        if mode == 'safety-exit':
            if not observed['safety_failure'] or not .05 < elapsed < 1.5:
                raise AssertionError('failed guard did not abort grace promptly')
        elif mode == 'interrupt-grace':
            if not observed['interrupted'] or not .05 < elapsed < 1.5:
                raise AssertionError('Ctrl+C failed to shorten grace')
        elif observed['interrupted'] or not GRACE <= elapsed < GRACE + .5:
            raise AssertionError('repeated SIGTERM changed the shared deadline')
        # Reap orphaned bwrap helpers adopted by the outer private PID 1.
        try:
            while os.waitpid(-1, os.WNOHANG)[0]:
                pass
        except ChildProcessError:
            pass
        if members(identity):
            raise AssertionError('nested children survived shared cleanup deadline')
        emit(mode=mode, **observed, late_child_removed=True,
             block_completed=block_completed,
             scope='network block then PID1 grace on shared private loopback; no pasta integration' if network_bank is not None else 'PID1 cleanup, actual PTY/signals, and injected capability-denied guard; no complete network shutdown integration')
    finally:
        if not reaped:
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
        os.close(master)
        if network_bank is not None:
            from lease_failure import stop_process
            network_bank.close()
            stop_process(network_server)


if __name__ == '__main__':
    require_private_namespace()
    mode = sys.argv[1]
    if mode in ('init', 'network-init'):
        init(network_order=mode == 'network-init')
    elif mode == 'app':
        app()
    elif mode == 'linger':
        linger(int(sys.argv[2]))
    elif mode == 'late-child':
        linger()
    else:
        raise SystemExit('Use run.py')
