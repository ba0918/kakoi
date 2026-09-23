"""E3 subset: finite nft permit and existing/new flow distinction on loopback."""
from guard import require_private_namespace
require_private_namespace()
import errno, json, select, socket, subprocess, threading, time
subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True)
results = []
for family, host, label in [(socket.AF_INET, '127.0.0.1', 'ipv4'), (socket.AF_INET6, '::1', 'ipv6')]:
    for typ, proto in [(socket.SOCK_STREAM, 'tcp'), (socket.SOCK_DGRAM, 'udp')]:
        server = socket.socket(family, typ)
        server.bind((host, 0))
        port = server.getsockname()[1]
        if proto == 'tcp':
            server.listen()
        stop = threading.Event()
        clients = []

        def echo():
            while not stop.is_set():
                for s in select.select([server] + clients, [], [], 0.05)[0]:
                    if s is server and proto == 'tcp':
                        c, _ = server.accept()
                        clients.append(c)
                    elif proto == 'udp':
                        data, addr = s.recvfrom(32)
                        s.sendto(data, addr)
                    else:
                        data = s.recv(32)
                        if data:
                            s.sendall(data)
                        else:
                            clients.remove(s)
                            s.close()
        thread = threading.Thread(target=echo)
        thread.start()
        table = 'probe_' + label + '_' + proto
        rules = f'table inet {table} {{\n set allowed {{ type inet_service; flags timeout; }}\n chain output {{ type filter hook output priority 0; policy accept;\n ct state established accept\n {proto} dport @allowed accept\n {proto} dport {port} drop\n }}\n}}'
        subprocess.run(['nft', '-f', '-'], input=rules, text=True, check=True)

        def connect():
            s = socket.socket(family, typ)
            s.settimeout(0.3)
            try:
                s.connect((host, port))
                return s
            except:
                s.close()
                raise

        def exchange(s):
            s.send(b'probe')
            return s.recv(32) == b'probe'

        def fresh():
            s = None
            try:
                s = connect()
                return exchange(s)
            except OSError as e:
                if isinstance(e, TimeoutError) or e.errno in (errno.EPERM, errno.EACCES, errno.ECONNREFUSED):
                    return False
                raise
            finally:
                if s:
                    s.close()
        try:
            before = fresh()
            subprocess.run(['nft', 'add', 'element', 'inet', table, 'allowed', f'{{ {port} timeout 1s }}'], check=True)
            old = connect()
            during = exchange(old)
            time.sleep(1.2)
            existing = exchange(old)
            old.close()
            new = fresh()
            result = dict(family=label, protocol=proto, before=before, during=during, existing_after_expiry=existing, new_after_expiry=new)
            print(json.dumps(result), flush=True)
            results.append(result)
            if not (before, during, existing, new) == (False, True, True, False):
                raise AssertionError('Unexpected probe observation: (before, during, existing, new) == (False, True, True, False)')
        finally:
            stop.set()
            thread.join()
            for c in clients:
                c.close()
            server.close()
            subprocess.run(['nft', 'delete', 'table', 'inet', table], check=True)
if not len(results) == 4:
    raise AssertionError('Unexpected probe observation: len(results) == 4')
