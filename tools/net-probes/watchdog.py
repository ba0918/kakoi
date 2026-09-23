"""Experimental heartbeat observer; timer values are probe settings, not product defaults."""
import json
import os
import select
import subprocess
import sys
import time

from guard import require_private_namespace
from recovery import nft, table

TIMEOUT = 1.0


def controller():
    nft([], table('recovery_policy', 'accept'))
    print('ready', flush=True)
    while True:
        os.write(sys.stdout.fileno(), b'.')
        time.sleep(.1)


def monitor(fd):
    # Announce readiness only after observing a real heartbeat from the controller.
    deadline = time.monotonic() + TIMEOUT
    announced = False
    while True:
        remaining = max(0, deadline - time.monotonic())
        if not select.select([fd], [], [], remaining)[0]:
            reason = 'heartbeat_timeout'
            break
        data = os.read(fd, 4096)
        if not data:
            reason = 'heartbeat_eof'
            break
        deadline = time.monotonic() + TIMEOUT
        if not announced:
            print('ready', flush=True)
            announced = True
    detected = time.monotonic()
    nft([], table('recovery_guard', 'drop'))
    print(json.dumps({'reason': reason, 'detected_monotonic': detected,
                      'blocked_monotonic': time.monotonic(), 'heartbeat_timeout': TIMEOUT}), flush=True)


if __name__ == '__main__':
    require_private_namespace()
    if sys.argv[1] == 'controller':
        controller()
    elif sys.argv[1] == 'monitor':
        monitor(int(sys.argv[2]))
    else:
        raise SystemExit('unknown mode')
