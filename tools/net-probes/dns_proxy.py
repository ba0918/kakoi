"""Private resolved proxy: opaque RR data, split routing, and configuration changes."""
import json
import os
import socket
import struct
import subprocess
import threading
import time
from pathlib import Path
import dns.message, dns.query, dns.rrset, dns.flags


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=5).stdout


def setup():
    for kind in ('user', 'net', 'pid', 'mnt'):
        original = os.environ.get('KAKOI_PROBE_HOST_' + kind.upper())
        if not original or original == os.readlink('/proc/self/ns/' + kind):
            raise SystemExit('private namespace required: ' + kind)
    run('mount', '--make-rprivate', '/')
    for path in ('/run', '/etc', '/tmp'):
        run('mount', '-t', 'tmpfs', '-o', 'size=8m,mode=0755', 'tmpfs', path)
    Path('/run/systemd').mkdir()
    # Only uid 0 is mapped in this rootless fixture; this does not test uid separation.
    Path('/etc/passwd').write_text('root:x:0:0:root:/root:/bin/sh\nsystemd-resolve:x:0:0:fixture:/run/systemd/resolve:/bin/false\n')
    Path('/etc/group').write_text('root:x:0:\nsystemd-resolve:x:0:\n')
    Path('/etc/nsswitch.conf').write_text('passwd: files\ngroup: files\nhosts: files dns\n')
    Path('/etc/hosts').write_text('127.0.0.1 localhost\n')
    Path('/etc/resolv.conf').write_text('nameserver 127.0.0.54\n')
    Path('/etc/machine-id').write_text('12345678901234567890123456789012\n')
    Path('/etc/systemd').mkdir()
    Path('/etc/systemd/resolved.conf').write_text('[Resolve]\nDNS=198.18.0.2\nFallbackDNS=\nDNSSEC=no\nDNSOverTLS=no\nLLMNR=no\nMulticastDNS=no\nDNSStubListener=yes\nCache=no\n')
    Path('/run/bus.conf').write_text('''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>system</type><listen>unix:path=/run/probe-bus</listen><policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>''')
    run('ip', 'link', 'set', 'lo', 'up')
    run('ip', 'link', 'add', 'probe0', 'type', 'dummy')
    run('ip', 'addr', 'add', '198.18.0.1/24', 'dev', 'probe0')
    run('ip', 'addr', 'add', '198.18.0.2/24', 'dev', 'probe0')
    run('ip', 'addr', 'add', '198.18.0.3/24', 'dev', 'probe0')
    run('ip', 'link', 'set', 'probe0', 'up')
    run('ip', 'route', 'add', 'default', 'dev', 'probe0')


def answer(raw, address):
    query = dns.message.from_wire(raw)
    response = dns.message.make_response(query)
    response.flags |= dns.flags.AA | dns.flags.RA
    name = query.question[0].name
    if query.question[0].rdtype == 65280:
        response.answer.append(dns.rrset.from_text(name, 60, 'IN', 'TYPE65280', r'\# 6 00ff10203040'))
    else:
        response.answer.append(dns.rrset.from_text(name, 60, 'IN', 'A', '192.0.2.2' if address.endswith('.2') else '192.0.2.3'))
        response.answer.append(dns.rrset.from_text(name, 60, 'IN', 'RRSIG', 'A 8 2 60 20300101000000 20200101000000 12345 example.test. AQIDBA=='))
    return response.to_wire()


def server(address):
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    tcp = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    udp.bind((address, 53)); tcp.bind((address, 53)); tcp.listen()
    def serve_udp():
        while True:
            raw, peer = udp.recvfrom(65535)
            udp.sendto(answer(raw, address), peer)
    def serve_tcp():
        while True:
            conn, _ = tcp.accept()
            with conn:
                conn.settimeout(2)
                def exact(count):
                    result = b''
                    while len(result) < count:
                        part = conn.recv(count-len(result))
                        if not part:
                            raise EOFError()
                        result += part
                    return result
                try:
                    size = struct.unpack('!H', exact(2))[0]
                    raw = answer(exact(size), address)
                    conn.sendall(struct.pack('!H', len(raw)) + raw)
                except (EOFError, TimeoutError):
                    pass
    for function in (serve_udp, serve_tcp):
        threading.Thread(target=function, daemon=True).start()


def probe():
    setup()
    server('198.18.0.2'); server('198.18.0.3')
    env = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'DBUS_SYSTEM_BUS_ADDRESS': 'unix:path=/run/probe-bus', 'SYSTEMD_LOG_LEVEL':'warning'}
    processes = []
    try:
        processes.append(subprocess.Popen(['dbus-daemon', '--nofork', '--config-file=/run/bus.conf'], env=env))
        until = time.monotonic() + 2
        while not Path('/run/probe-bus').exists():
            if time.monotonic() >= until:
                raise AssertionError('private bus startup')
            time.sleep(.02)
        processes.append(subprocess.Popen(['/usr/lib/systemd/systemd-resolved'], env=env))
        def query(kind, tcp=False, name='records.example.test.'):
            request = dns.message.make_query(name, kind, want_dnssec=True)
            return (dns.query.tcp if tcp else dns.query.udp)(request, '127.0.0.54', timeout=1)
        until = time.monotonic() + 4
        while True:
            try:
                result = query(65280)
                break
            except (OSError, dns.exception.Timeout):
                if time.monotonic() >= until:
                    raise AssertionError('private resolved startup')
                time.sleep(.05)
        for tcp in (False, True):
            for kind in (65280, 1):
                response = query(kind, tcp)
                expected = dns.message.from_wire(answer(dns.message.make_query('records.example.test.', kind, want_dnssec=True).to_wire(), '198.18.0.2'))
                actual = sorted((rr.rdtype, item.to_wire()) for rr in response.answer for item in rr)
                wanted = sorted((rr.rdtype, item.to_wire()) for rr in expected.answer for item in rr)
                if response.rcode() != 0 or actual != wanted:
                    raise AssertionError((tcp, kind, response.to_text(), wanted))
                print(json.dumps({'transport':'tcp' if tcp else 'udp', 'rrtype':kind, 'rdata_preserved':True}), flush=True)
        interface = str(socket.if_nametoindex('probe0'))
        def bus(*args):
            return run('busctl', '--address=unix:path=/run/probe-bus', '--timeout=2', *args)
        def method(name, signature, *args):
            return bus('call', 'org.freedesktop.resolve1', '/org/freedesktop/resolve1', 'org.freedesktop.resolve1.Manager', name, signature, interface, *args)
        def domains():
            return bus('get-property', 'org.freedesktop.resolve1', '/org/freedesktop/resolve1', 'org.freedesktop.resolve1.Manager', 'Domains')
        before = domains()
        method('SetLinkDNS', 'ia(iay)', '1', '2', '4', '198', '18', '0', '3')
        method('SetLinkDefaultRoute', 'ib', 'false')
        method('SetLinkDomains', 'ia(sb)', '1', 'routed.example.test', 'true')
        after = domains()
        if before == after or 'routed.example.test' not in after:
            raise AssertionError((before, after))
        for name, expected in (('records.example.test.', '192.0.2.2'), ('records.routed.example.test.', '192.0.2.3')):
            result = query(1, name=name)
            if result.rcode() != 0 or result.answer[0][0].address != expected:
                raise AssertionError(result.to_text())
        method('SetLinkDNS', 'ia(iay)', '1', '2', '4', '198', '18', '0', '2')
        result = query(1, name='records.routed.example.test.')
        if result.rcode() != 0 or result.answer[0][0].address != '192.0.2.2':
            raise AssertionError(result.to_text())
        print(json.dumps({'split_dns':True, 'domain_snapshot_change':True, 'upstream_change':True}), flush=True)
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait(timeout=2)

if __name__ == '__main__':
    probe()
