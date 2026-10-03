"""Kill the product's main-command child at its kernel fork trace stop."""
import ctypes
import os
import signal
import subprocess
import sys

libc = ctypes.CDLL(None, use_errno=True)
libc.ptrace.argtypes = [ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p, ctypes.c_void_p]
libc.ptrace.restype = ctypes.c_long


def trace(request, pid, data=0):
    result = libc.ptrace(request, pid, None, ctypes.c_void_p(data))
    if result < 0:
        raise OSError(ctypes.get_errno(), "ptrace")


child = subprocess.Popen([sys.argv[1], "--self-test-preexec-death"])
pid, status = os.waitpid(child.pid, 0)
assert os.WIFSTOPPED(status)
trace(0x4200, pid, 2 | 4 | 16)  # fork, vfork, exec; private descendants only
trace(7, pid)
killed = False
exit_code = None
while True:
    try:
        pid, status = os.waitpid(-1, 0x40000000)  # __WALL for traced descendants
    except ChildProcessError:
        break
    if os.WIFEXITED(status) or os.WIFSIGNALED(status):
        if pid == child.pid:
            exit_code = os.waitstatus_to_exitcode(status)
        continue
    if not os.WIFSTOPPED(status):
        continue
    event = status >> 16
    if event in (1, 2):
        born = ctypes.c_ulong()
        if libc.ptrace(0x4201, pid, None, ctypes.cast(ctypes.byref(born), ctypes.c_void_p)) < 0:
            raise OSError(ctypes.get_errno(), "get fork event")
        with open(f"/proc/{pid}/status", encoding="utf-8") as info:
            namespace_pids = next(
                line.split()[1:] for line in info if line.startswith("NSpid:")
            )
        # The product's init is PID 1 in its own namespace, independent of its
        # internal filename and the kernel's deleted-file display suffix.
        if len(namespace_pids) > 1 and namespace_pids[-1] == "1":
            assert not killed
            os.kill(born.value, signal.SIGKILL)
            killed = True
    stopped = os.WSTOPSIG(status)
    trace(7, pid, 0 if stopped in (signal.SIGSTOP, signal.SIGTRAP) else stopped)
assert killed, "the init's actual command fork was not observed"
assert exit_code == 0, f"consumer exit: {exit_code}"
