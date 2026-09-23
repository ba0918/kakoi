"""Measure unsolicited TCP/UDP delivery after the kernel health gate expires."""
import socket
import subprocess
import sys
import time
from pathlib import Path
from guard import require_private_namespace
from lease_failure import ready, stop_process

def probe(family):
    require_private_namespace()
    subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True, timeout=2)
    helper = str(Path(__file__).with_name('health_lease.py'))
    af, address = (socket.AF_INET, '127.0.0.1') if family == 'ipv4' else (socket.AF_INET6, '::1')
    children = []
    sockets = []

    def sock(kind):
        s = socket.socket(af, kind)
        s.settimeout(0.2)
        sockets.append(s)
        return s
    try:
        tcp = sock(socket.SOCK_STREAM)
        tcp.bind((address, 8080))
        tcp.listen()
        udp = sock(socket.SOCK_DGRAM)
        udp.bind((address, 8080))
        writer = subprocess.Popen([sys.executable, helper], stdout=subprocess.PIPE, text=True)
        children.append(writer)
        ready(writer)
        ct = sock(socket.SOCK_STREAM)
        ct.connect((address, 8080))
        st, _ = tcp.accept()
        st.settimeout(0.2)
        sockets.append(st)
        cu = sock(socket.SOCK_DGRAM)
        cu.connect((address, 8080))
        cu.send(b'hello')
        _, peer = udp.recvfrom(20)
        st.sendall(b'before')
        udp.sendto(b'before', peer)
        if ct.recv(20) != b'before' or cu.recv(20) != b'before':
            raise AssertionError('push positive control failed')
        writer.kill()
        writer.wait(timeout=2)
        time.sleep(2.3)
        st.sendall(b'after')
        try:
            udp.sendto(b'after', peer)
        except PermissionError:
            pass
        observed = {}
        for name, client in [('tcp', ct), ('udp', cu)]:
            try:
                observed[name] = bool(client.recv(20))
            except TimeoutError:
                observed[name] = False
        print({'push_received_after_expiry': observed}, flush=True)
        if any(observed.values()):
            raise AssertionError('unsolicited server traffic bypassed expiry')
    finally:
        for s in sockets:
            s.close()
        for child in children:
            stop_process(child)
