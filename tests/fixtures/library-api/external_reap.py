"""Let the caller's real waitpid reap its worker before the library's wait4."""
import ctypes
import os
import signal
import subprocess
import sys

libc = ctypes.CDLL(None, use_errno=True)
libc.ptrace.argtypes = [ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p, ctypes.c_void_p]
libc.ptrace.restype = ctypes.c_long


def trace(request, pid, data=0):
    if libc.ptrace(request, pid, None, ctypes.c_void_p(data)) < 0:
        raise OSError(ctypes.get_errno(), "ptrace")


def registers(pid):
    values = (ctypes.c_ulonglong * 27)()
    if libc.ptrace(12, pid, None, ctypes.cast(values, ctypes.c_void_p)) < 0:
        raise OSError(ctypes.get_errno(), "get registers")
    return values


child = subprocess.Popen([sys.argv[1], "--self-test-external-reap"])
pid, status = os.waitpid(child.pid, 0)
assert os.WIFSTOPPED(status)
trace(0x4200, pid, 1 | 8)  # syscall-stop marker and clone threads, not worker forks
trace(24, pid)
held = None
reaped = False
exit_code = None
while True:
    try:
        pid, status = os.waitpid(-1, 0x40000000)
    except ChildProcessError:
        break
    if os.WIFEXITED(status) or os.WIFSIGNALED(status):
        if pid == child.pid:
            exit_code = os.waitstatus_to_exitcode(status)
        continue
    stopped = os.WSTOPSIG(status)
    if stopped == (signal.SIGTRAP | 0x80):
        regs = registers(pid)
        # x86_64 user_regs_struct: orig_rax=15, rax=10. wait4 is syscall 61.
        if regs[15] == 61:
            if pid != child.pid and regs[10] == (2**64 - 38) and not reaped:
                assert held is None
                held = pid
                continue
            if pid == child.pid and 0 < regs[10] < 2**31:
                reaped = True
                if held is not None:
                    trace(24, held)
                    held = None
    trace(24, pid, 0 if stopped in (signal.SIGSTOP, signal.SIGTRAP, signal.SIGTRAP | 0x80) else stopped)
assert reaped, "the caller's actual waitpid did not reap a child"
assert held is None
assert exit_code == 0, f"consumer exit: {exit_code}"
