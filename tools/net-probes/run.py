#!/usr/bin/env python3
"""Run bounded feasibility probes, never host network configuration commands."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
PROBES = ("tap", "lifetime", "pasta-ipv4", "pasta-ipv6", "hooks-ipv4", "hooks-ipv6", "boundary-ipv4", "boundary-ipv6", "dynamic-ipv4", "dynamic-ipv6", "dynamic-udp-ipv4", "dynamic-udp-ipv6", "dynamic-udp-expiry-ipv4", "dynamic-udp-expiry-ipv6")
PROBES += ("dynamic-udp-churn-ipv4", "dynamic-udp-churn-ipv6")
PROBES += ("control-isolation", "dns-proxy")
PROBES += ("dynamic-control-ipv4", "dynamic-control-ipv6", "lease-stop", "lease-kill", "lease-heartbeat-stop", "lease-heartbeat-kill")
PROBES += ("dynamic-conflict-ipv4", "dynamic-conflict-ipv6")
PROBES += ("recovery-ipv4", "recovery-ipv6")
PROBES += ("lease-recovery-ipv4", "lease-recovery-ipv6")
PROBES += ("lease-preserve-ipv4", "lease-preserve-ipv6", "lease-stage-ipv4", "lease-stage-ipv6")
PROBES += ("process-parent-death", "process-main-exit")
PROBES += ("process-ctrl-c", "process-ctrl-c-default")
PROBES += ("process-network-order",)
PROBES += ("process-interrupt-grace", "process-repeat-term", "process-safety-exit")
PROBES += ("health-push-ipv4", "health-push-ipv6", "recovery-push-ipv4", "recovery-push-ipv6")
PROBES += ("resume-ipv4", "resume-ipv6", "lease-health-push-ipv4", "lease-health-push-ipv6")
PROBES += tuple("health-" + fault + "-" + family for fault in ("stop", "kill") for family in ("ipv4", "ipv6"))
PROBES += tuple('watchdog-' + fault + '-' + family for fault in ('stop', 'kill') for family in ('ipv4', 'ipv6'))

PROBES += tuple('fixed-' + host + '-' + target for host in ('ipv4', 'ipv6') for target in ('ipv4', 'ipv6'))
PROBES += ('fixed-conflict-ipv4', 'fixed-conflict-ipv6')
PROBES += ('dual-guard-ipv4', 'dual-guard-ipv6')

INITIAL_RUNTIME_PROBES = (
    'boundary-ipv4', 'boundary-ipv6',
    'fixed-ipv4-ipv4', 'fixed-ipv6-ipv6',
    'fixed-conflict-ipv4', 'fixed-conflict-ipv6',
    'health-push-ipv4', 'health-push-ipv6',
    'recovery-push-ipv4', 'recovery-push-ipv6',
)


def invoke(argv, timeout=10):
    result = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    return {"argv": argv, "exit": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pasta", help="pasta executable path; default: search PATH")
    parser.add_argument("--dns-python", help="Python with dnspython 2.8.0; only for dns-proxy")
    parser.add_argument("--pesto", help="pesto executable for dynamic probes; default: next to pasta")
    parser.add_argument("--pasta-loopback-mode", choices=("explicit", "default"),
                        default="explicit", help="explicit: pass --host-lo-to-ns-lo; default: measure pasta defaults")
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--only", choices=PROBES, action="append")
    selection.add_argument("--initial-runtime", action="store_true", help="fixed initial-release runtime checks; use an unmodified pasta")
    parser.add_argument("--output", type=Path, help="new result directory; must not already exist")
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error("Run as your ordinary user, without sudo")
    def interrupted(signum, frame):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, interrupted)
    names = (INITIAL_RUNTIME_PROBES if args.initial_runtime else args.only) or [name for name in PROBES if not name.startswith(('dynamic-', 'lease-', 'recovery-', 'watchdog-', 'process-', 'resume-', 'dns-', 'health-', 'fixed-', 'dual-')) and name != 'control-isolation']
    required = ["unshare", "ip", "nft"]
    if any(name.startswith('process-') for name in names):
        required.append('bwrap')
    if any(name.startswith(("hooks-", "boundary-", "recovery-", "watchdog-", "resume-", "health-")) for name in names):
        required.append("nsenter")
    if any(name.startswith(("boundary-", "recovery-", "watchdog-", "resume-", "health-")) for name in names):
        required.append("setpriv")
    dynamic = any(name.startswith("dynamic-") for name in names)
    if dynamic:
        required.append("mount")
    if 'control-isolation' in names or any(name.startswith('dynamic-control-') for name in names):
        required.extend(['bwrap', 'mount', 'umount'])
    dns_python = None
    dns_metadata = None
    if 'dns-proxy' in names:
        required.extend(['mount', 'dbus-daemon', 'busctl'])
        resolved = Path('/usr/lib/systemd/systemd-resolved')
        if not args.dns_python or not resolved.is_file():
            parser.error('dns-proxy needs --dns-python and /usr/lib/systemd/systemd-resolved')
        dns_python = os.path.abspath(os.path.expanduser(args.dns_python))
        version = invoke([dns_python, '-c', 'import dns; print(dns.__version__)'])
        if version['exit'] != 0 or version['stdout'].strip() != '2.8.0':
            parser.error('dns-proxy needs dnspython 2.8.0 in --dns-python')
        dns_metadata = {'dnspython': '2.8.0', 'python': dns_python,
                        'resolved': invoke([str(resolved), '--version']),
                        'resolved_sha256': hashlib.sha256(resolved.read_bytes()).hexdigest(),
                        'dbus': invoke(['dbus-daemon', '--version'])}
    missing = [name for name in required if not shutil.which(name)]
    if missing:
        parser.error("Missing commands: " + ", ".join(missing))
    pasta = args.pasta or shutil.which("pasta")
    if pasta:
        # Preserve the pasta symlink basename: passt selects behavior using argv[0].
        pasta = os.path.abspath(os.path.expanduser(pasta))
        if not os.path.isfile(pasta) or not os.access(pasta, os.X_OK):
            parser.error("Not an executable: " + pasta)
    pesto = None
    if dynamic:
        pesto = args.pesto or (str(Path(pasta).with_name("pesto")) if pasta else None)
        if not pesto:
            parser.error("Dynamic probes require --pasta and --pesto")
        pesto = os.path.abspath(os.path.expanduser(pesto))
        if not os.path.isfile(pesto) or not os.access(pesto, os.X_OK):
            parser.error("Not an executable: " + pesto)
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    out = (args.output or Path(".agents/tmp/kakoi-net-proof") / stamp).absolute()
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env["KAKOI_PROBE_PASTA_LOOPBACK_MODE"] = args.pasta_loopback_mode
    env["KAKOI_PROBE_HOST_MNT"] = os.readlink("/proc/self/ns/mnt")
    if pesto:
        env["KAKOI_PROBE_PESTO"] = pesto
    original = {kind: os.readlink("/proc/self/ns/" + kind) for kind in ("net", "user", "pid")}
    env.update({"KAKOI_PROBE_HOST_" + kind.upper(): value for kind, value in original.items()})
    report = {
        "format": 1, "scope": "selected mechanism probes only; not E1/E2/E3 completion",
        "pasta_loopback_mode": args.pasta_loopback_mode,
        "started_utc": stamp, "kernel": os.uname().release,
        "os_release": Path("/etc/os-release").read_text(),
        "host_namespaces": original, "tun_present": Path("/dev/net/tun").exists(),
        "versions": [invoke([tool, "--version"]) for tool in ("unshare", "nft")],
        "source_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in sorted(HERE.glob("*.py"))},
        "probes": [], "full_gate": "NOT_RUN",
    }
    if dns_metadata:
        report["dns"] = dns_metadata
    if pasta:
        report["pasta"] = invoke([pasta, "--version"])
        report["pasta"]["sha256"] = hashlib.sha256(Path(pasta).read_bytes()).hexdigest()
    if pesto:
        report["pesto"] = {"path": pesto, "sha256": hashlib.sha256(Path(pesto).read_bytes()).hexdigest()}
    report_path = out / "report.json"

    def save():
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")

    save()
    print("Results:", out, flush=True)
    try:
        for name in names:
            if name.startswith(("pasta-", "hooks-", "boundary-", "dynamic-", "recovery-", "watchdog-", "resume-", "health-", "fixed-", "dual-")) and not pasta:
                report["probes"].append({"name": name, "status": "BLOCKED", "reason": "pasta not specified/found"})
                print(name + ": BLOCKED (pasta not found)", flush=True)
                save()
                continue
            argv = ["unshare", "--user", "--map-root-user", "--net", "--pid",
                    "--fork", "--kill-child=KILL", "--mount-proc", dns_python if name == "dns-proxy" else sys.executable,
                    str(HERE / "worker.py"), name]
            if pasta:
                argv += [pasta]
            started = time.monotonic()
            process = None
            with (out / (name + ".stdout")).open("w") as stdout, (out / (name + ".stderr")).open("w") as stderr:
                try:
                    process = subprocess.Popen(argv, env=env, stdout=stdout, stderr=stderr,
                                               start_new_session=True)
                    code = process.wait(timeout=35)
                    status = "PASS" if code == 0 else "FAIL"
                except subprocess.TimeoutExpired:
                    status, code = "TIMEOUT", None
                finally:
                    # A fresh session limits cancellation to this probe. PID namespace
                    # teardown also kills descendants if a candidate daemonizes.
                    if process is not None:
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        process.wait()
            entry = {"name": name, "status": status, "exit": code,
                     "seconds": round(time.monotonic() - started, 3), "argv": argv,
                     "launcher_reaped": process.returncode is not None}
            report["probes"].append(entry)
            save()
            print(name + ": " + status, flush=True)
    except KeyboardInterrupt:
        report["interrupted"] = True
        print("Interrupted; active probe terminated", file=sys.stderr)
    finally:
        report["host_namespaces_unchanged"] = all(
            os.readlink("/proc/self/ns/" + kind) == value for kind, value in original.items())
        report["batch_status"] = "PASS" if (
            len(report["probes"]) == len(names)
            and all(p["status"] == "PASS" for p in report["probes"])
            and not report.get("interrupted")) else "INCOMPLETE"
        save()
    print("Batch:", report["batch_status"], "| Full feasibility gate: NOT_RUN", flush=True)
    return 0 if report["batch_status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
