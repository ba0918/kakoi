"""Keep unrelated UDP and TCP traffic alive during explicit publication changes."""
from contextlib import ExitStack
import os
import select
import socket
import sys
import threading
import time

from guard import require_private_namespace
from dynamic import configure as configure_both, echo, emit, endpoint, new_socket
from udp_reassign import configure as configure_udp


def serve(family):
    require_private_namespace()
    af, address = endpoint(family)
    with ExitStack() as stack:
        services = {}
        for port, label in ((8080, b'A:'), (8082, b'B:'), (8084, b'C:')):
            sock = stack.enter_context(new_socket(af, socket.SOCK_DGRAM))
            sock.bind((address, port))
            services[sock] = label
        listener = stack.enter_context(new_socket(af, socket.SOCK_STREAM))
        listener.bind((address, 8084))
        listener.listen()
        readers = [*services, listener]
        print('ready', flush=True)
        while True:
            for source in select.select(readers, [], [], 1)[0]:
                if source is listener:
                    conn, _ = listener.accept()
                    conn.settimeout(0.5)
                    stack.enter_context(conn)
                    readers.append(conn)
                elif source in services:
                    data, peer = source.recvfrom(1024)
                    source.sendto(services[source] + data, peer)
                else:
                    data = source.recv(1024)
                    if data:
                        source.sendall(data)
                    else:
                        readers.remove(source)
                        source.close()


def sample(client):
    token = os.urandom(16)
    deadline = time.monotonic() + 0.2
    try:
        client.send(token)
        while time.monotonic() < deadline:
            client.settimeout(max(0.001, deadline - time.monotonic()))
            data = client.recv(1024)
            if len(data) == 18 and data[1:2] == b':' and data[2:] == token:
                return data[:1].decode('ascii')
        return 'timeout'
    except TimeoutError:
        return 'timeout'
    except ConnectionRefusedError:
        return 'refused'


def background(client, stop, observations, errors):
    try:
        while not stop.is_set():
            observations.append((time.monotonic(), sample(client)))
            stop.wait(0.005)
    except Exception as error:
        errors.append(repr(error))


def exercise(pesto, control, family):
    af, address = endpoint(family)
    stable = address + '/18084:8084'
    mapping = address + '/18080:8080'
    configure_both(pesto, control, '-A', stable)
    configure_udp(pesto, control, '-A', mapping)
    stop = threading.Event()
    observations, errors, threads, intervals = {'stable': [], 'changing': []}, [], [], []
    failures = []
    with ExitStack() as stack:
        clients = {}
        for label, port in (('stable', 18084), ('changing', 18080), ('probe', 18080)):
            client = stack.enter_context(new_socket(af, socket.SOCK_DGRAM))
            client.connect((address, port))
            clients[label] = client
        tcp = stack.enter_context(new_socket(af, socket.SOCK_STREAM))
        tcp.connect((address, 18084))
        if sample(clients['stable']) != 'C' or sample(clients['probe']) != 'A' or not echo(tcp, b'initial'):
            raise AssertionError('initial services did not answer')
        try:
            for label in ('stable', 'changing'):
                thread = threading.Thread(target=background,
                                          args=(clients[label], stop, observations[label], errors), daemon=True)
                threads.append(thread)
                thread.start()
            for index in range(8):
                started = time.monotonic()
                configure_udp(pesto, control, '-D', mapping)
                removed = [sample(clients['probe']) for _ in range(3)]
                if any(reply not in ('timeout', 'refused') for reply in removed):
                    failures.append('publication still replied after delete: ' + str(index))
                if not echo(tcp, b'deleted-' + str(index).encode()):
                    failures.append('TCP failed after delete: ' + str(index))
                port, expected = (8082, 'B') if index % 2 == 0 else (8080, 'A')
                mapping = address + '/18080:' + str(port)
                configure_udp(pesto, control, '-A', mapping)
                replies = [sample(clients['probe']) for _ in range(3)]
                if expected not in replies or any(reply not in (expected, 'timeout', 'refused') for reply in replies):
                    failures.append('wrong or missing reassigned response: ' + str(index))
                if not echo(tcp, b'added-' + str(index).encode()):
                    failures.append('TCP failed after add: ' + str(index))
                time.sleep(0.1)
                ended = time.monotonic()
                intervals.append((started, ended))
                emit(round=index, removed=removed, replies=replies, expected=expected)
        finally:
            stop.set()
            for thread in threads:
                thread.join(timeout=2)
            if any(thread.is_alive() for thread in threads):
                raise AssertionError('traffic worker failed to stop')
        for label, values in observations.items():
            counts = {reply: sum(value == reply for _, value in values) for _, reply in values}
            emit(background=label, counts=counts, source=clients[label].getsockname())
        stable_values = observations['stable']
        coverage = [sum(start <= when <= end and reply == 'C' for when, reply in stable_values)
                    for start, end in intervals]
        emit(stable_udp_successes_per_round=coverage, worker_errors=errors)
        if errors or len(intervals) != 8 or any(count == 0 for count in coverage):
            failures.append('unrelated UDP missing in a round or traffic worker failed')
        if any(reply not in ('C', 'timeout', 'refused') for _, reply in stable_values):
            failures.append('unrelated UDP received another service response')
        if not any(reply in ('A', 'B') for _, reply in observations['changing']):
            failures.append('background traffic to changed publication never succeeded')
        if sample(clients['stable']) != 'C' or not echo(tcp, b'final'):
            failures.append('unrelated services did not survive')
        configure_udp(pesto, control, '-D', mapping)
        configure_both(pesto, control, '-D', stable)
        emit(family=family, failures=failures,
             scope='8 explicit delete/add cycles with concurrent UDP and persistent TCP; no exhaustive race proof or UDP zero-loss guarantee')
        if failures:
            raise AssertionError('; '.join(failures))


if __name__ == '__main__':
    require_private_namespace()
    if len(sys.argv) != 3 or sys.argv[1] != 'serve':
        raise SystemExit('Use run.py')
    serve(sys.argv[2])
