"""Measure hiding a pathname UNIX control socket from a bwrap app."""
import errno
import json
import os
from pathlib import Path
import socket
import subprocess
import sys

from guard import require_private_namespace
from worker import run

CONTROL = '/tmp/kakoi-control-probe.sock'


def emit(**values):
    print(json.dumps(values), flush=True)


def check_hidden(inode, control=CONTROL):
    paths = [control]
    for entry in Path('/proc').iterdir():
        if entry.name.isdigit():
            paths.append(str(entry / 'root' / control.lstrip('/')))
    denied = {}
    for path in paths:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(0.2)
            try:
                client.connect(path)
            except OSError as error:
                if error.errno not in (errno.ENOENT, errno.EACCES, errno.EPERM, errno.ESRCH):
                    raise
                denied[path] = error.errno
            else:
                raise AssertionError('app reached control socket through ' + path)
    descriptors = []
    for fd in Path('/proc/self/fd').iterdir():
        try:
            target = os.readlink(fd)
        except FileNotFoundError:
            continue
        descriptors.append(target)
    if 'socket:[' + inode + ']' in descriptors:
        raise AssertionError('control socket descriptor inherited by app')
    emit(denied_socket_paths=denied, control_fd_absent=True)


def app(parent, inode, control=CONTROL):
    require_private_namespace()
    actual = {kind: os.readlink('/proc/self/ns/' + kind) for kind in parent}
    if any(actual[kind] == parent[kind] for kind in ('user', 'mnt', 'pid')):
        raise AssertionError('app did not get separate user/mount/PID namespaces')
    if actual['net'] != parent['net']:
        raise AssertionError('shared network condition was not exercised')
    status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
    caps = {key: status[key].strip() for key in ('CapPrm', 'CapEff', 'CapBnd', 'CapAmb')}
    if any(int(value, 16) for value in caps.values()):
        raise AssertionError('app retained capabilities')
    emit(app_namespaces=actual, capabilities=caps)
    check_hidden(inode, control)
    nested = subprocess.run(['unshare', '--user', '--map-root-user', '--mount',
                             sys.executable, str(Path(__file__).resolve()), 'nested', inode, control],
                            capture_output=True, text=True, timeout=5,
                            env={**os.environ, 'LC_ALL': 'C'})
    emit(nested_exit=nested.returncode, stdout=nested.stdout, stderr=nested.stderr)
    if nested.returncode != 0:
        if not nested.stderr.startswith('unshare:') or 'Operation not permitted' not in nested.stderr:
            raise AssertionError('nested attempt failed for an unexpected reason')
        emit(nested_scope='namespace creation denied; post-creation attack not exercised')


def run_app(parent, inode, control=CONTROL):
    argv = ['bwrap', '--ro-bind', '/', '/', '--unshare-all', '--share-net',
            '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp',
            '--cap-drop', 'ALL', '--die-with-parent', '--new-session', '--',
            sys.executable, str(Path(__file__).resolve()), 'app', json.dumps(parent), inode, control]
    result = subprocess.run(argv, capture_output=True, text=True, timeout=12, close_fds=True)
    emit(bwrap_exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
    result.check_returncode()


def probe():
    require_private_namespace()
    original = os.environ.get('KAKOI_PROBE_HOST_MNT')
    if not original or original == os.readlink('/proc/self/ns/mnt'):
        raise SystemExit('Refusing tmpfs outside private mount namespace')
    run('mount', '--make-rprivate', '/')
    run('mount', '-t', 'tmpfs', '-o', 'size=1m,mode=0700', 'tmpfs', '/tmp')
    parent = {kind: os.readlink('/proc/self/ns/' + kind) for kind in ('user', 'mnt', 'pid', 'net')}
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
        server.bind(CONTROL)
        server.listen(8)
        inode = str(os.fstat(server.fileno()).st_ino)
        def trusted_connect():
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.settimeout(1)
                client.connect(CONTROL)
                accepted, _ = server.accept()
                accepted.close()
        trusted_connect()
        run_app(parent, inode)
        trusted_connect()
        emit(trusted_control_before_after=True,
             scope='pathname UNIX socket and bwrap mount/PID/user boundary; no pasta integration or complete escape proof')


if __name__ == '__main__':
    require_private_namespace()
    if sys.argv[1] == 'app':
        app(json.loads(sys.argv[2]), sys.argv[3], sys.argv[4])
    elif sys.argv[1] == 'nested':
        result = subprocess.run(['umount', '/tmp'], capture_output=True, text=True, timeout=2)
        emit(unmount_exit=result.returncode, stderr=result.stderr)
        check_hidden(sys.argv[2], sys.argv[3])
    else:
        raise SystemExit('Use run.py')
