"""Exercise terminal-generated SIGINT through a supervising process and bwrap."""
import errno
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

HERE = str(Path(__file__).resolve())


def app(default=False):
    signal.signal(signal.SIGINT, signal.SIG_DFL if default else lambda *_: print('APP_INTERRUPT', flush=True))
    print('APP_READY', flush=True)
    for line in sys.stdin:
        if line.strip() == 'exit':
            raise SystemExit(23)


def supervisor(default=False):
    interrupts = []
    # The wrapper must survive the foreground group's interrupt. The app sets
    # its own handler after exec; a general launcher must restore app defaults.
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    try:
        process = subprocess.Popen(['bwrap', '--ro-bind', '/', '/', '--unshare-all', '--share-net',
                                    '--proc', '/proc', '--dev', '/dev', '--cap-drop', 'ALL',
                                    '--die-with-parent', '--', sys.executable, HERE, 'default-app' if default else 'app'])
    finally:
        signal.signal(signal.SIGINT, lambda *_: interrupts.append(time.monotonic()))
    try:
        code = process.wait()
        print('SUPERVISOR_RESULT ' + json.dumps({'interrupts': len(interrupts), 'app_exit': code}), flush=True)
        raise SystemExit(code)
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()


def probe(default=False):
    require_private_namespace()
    pid, master = pty.fork()
    if pid == 0:
        os.execv(sys.executable, [sys.executable, HERE, 'default-supervisor' if default else 'supervisor'])
    output = b''
    reaped = False
    def until(marker):
        nonlocal output
        deadline = time.monotonic() + 5
        while marker not in output:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([master], [], [], remaining)[0]:
                raise AssertionError('terminal event timeout: ' + repr(output))
            try:
                chunk = os.read(master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    raise AssertionError('terminal closed before expected event: ' + repr(output)) from error
                raise
            if not chunk:
                raise AssertionError('terminal EOF before expected event')
            output += chunk
    try:
        until(b'APP_READY')
        os.write(master, b'\x03')
        if not default:
            until(b'APP_INTERRUPT')
            if os.waitpid(pid, os.WNOHANG)[0]:
                reaped = True
                raise AssertionError('Ctrl+C terminated the supervisor')
            os.write(master, b'exit\n')
        expected_code = 130 if default else 23
        until(b'SUPERVISOR_RESULT ')
        until(f'"app_exit": {expected_code}}}'.encode())
        deadline = time.monotonic() + 3
        while True:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                reaped = True
                break
            if time.monotonic() >= deadline:
                raise AssertionError('supervisor failed to exit')
            time.sleep(.01)
        if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != expected_code:
            raise AssertionError('main exit value changed across supervisor')
        result = json.loads(output.decode().split('SUPERVISOR_RESULT ', 1)[1].splitlines()[0])
        if result != {'interrupts': 1, 'app_exit': expected_code}:
            raise AssertionError('terminal signal/result differs: ' + repr(result))
        emit(terminal_generated_interrupt=True, app_continued=not default, **result,
             scope='interactive Ctrl+C and app result through bwrap; no shutdown grace, other signal priority or complete terminal safety proof')
    finally:
        if not reaped:
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
        os.close(master)


if __name__ == '__main__':
    require_private_namespace()
    if sys.argv[1] in ('supervisor', 'default-supervisor'):
        supervisor(sys.argv[1] == 'default-supervisor')
    elif sys.argv[1] in ('app', 'default-app'):
        app(sys.argv[1] == 'default-app')
    else:
        raise SystemExit('Use run.py')
