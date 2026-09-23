"""Refuse direct execution of probes that change network state."""
import os


def require_private_namespace():
    for kind in ("net", "user", "pid"):
        original = os.environ.get("KAKOI_PROBE_HOST_" + kind.upper())
        current = os.readlink("/proc/self/ns/" + kind)
        if not original or current == original:
            raise SystemExit("Refusing probe outside the runner's private " + kind + " namespace")
