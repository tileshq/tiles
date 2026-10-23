#!/usr/bin/env python3
"""Run a command with no network and fail on any try to reach one (ADR-058).

    python3 scripts/no-network.py <trace-file> <command> [args...]

The command runs in a network namespace of its own (`unshare --user
--map-root-user --net`), which holds only a loopback interface that is down,
so no packet leaves the machine. strace follows it and every process it
starts and writes each connect, sendto, sendmsg, sendmmsg and io_uring_setup
call to <trace-file>. A failed call is still written, so a command that
tries to connect, fails and carries on is still caught. strace answers every
io_uring_setup with ENOSYS, so no ring opens and a program falls back to
calls the trace sees (node's libuv asks for a ring per event loop and falls
back to epoll).

The trace allows, and so the rule permits, exactly:
  - a send with no address (on a socket already connected, whose connect
    the trace has judged, or on a socket pair, which never leaves the
    process tree);
  - a connect or send to an AF_NETLINK address, which talks to the kernel
    only (glibc asks it for the machine's interfaces);
  - a connect to a filesystem AF_UNIX socket named in ALLOWED_UNIX, which
    is empty.

Anything else is a finding: every AF_INET and AF_INET6 address, loopback
included (the stub resolver at 127.0.0.53 is DNS), every other AF_UNIX
socket (systemd-resolved, nscd, D-Bus and Docker each listen on one and
would reach the network for the caller), an address strace could not decode,
and an io_uring_setup that opened a ring anyway, since a ring can connect
without a call this trace sees.
A DNS lookup reaches the resolver through one of these, so it is a finding.

Prints each finding and exits 1 when there is one; otherwise exits with the
command's own status. Exits 2 when the sandbox cannot start. Linux only:
needs unshare (util-linux), strace, and unprivileged user namespaces.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

TRACED = ("connect", "sendto", "sendmsg", "sendmmsg", "io_uring_setup")
ALLOWED_FAMILIES = {"AF_NETLINK"}
ALLOWED_UNIX: frozenset[str] = frozenset()

# `1234 connect(3, {sa_family=...}, 16) = -1 ENETUNREACH (...)`, or the
# first half of a call strace split around another process's line.
CALL = re.compile(r"^(?:\d+\s+)?(?:\[pid\s+\d+\]\s+)?(\w+)\((.*)$")
FAMILY = re.compile(r"\b(?:sa|nl|sll|svm|sin6?)_family=(\w+)")
UNIX_PATH = re.compile(r'sun_path=(@?"(?:[^"\\]|\\.)*")')
# An io_uring_setup, whole or resumed, that returned a ring.
RING = re.compile(r"io_uring_setup(?:\(| resumed>).*\)\s+=\s+\d+")
# An address strace printed as a bare pointer: it could not read it.
RAW_ADDRESS = {
    "connect": re.compile(r"^\d+, 0x[0-9a-f]+,"),
    "sendto": re.compile(r", 0x[0-9a-f]+, \d+\)"),
    "sendmsg": re.compile(r"msg_name=0x[0-9a-f]+"),
    "sendmmsg": re.compile(r"msg_name=0x[0-9a-f]+"),
}


def findings(trace: str) -> list[str]:
    """Each call in strace's output that the rule above does not allow."""
    out = []
    for line in trace.splitlines():
        if RING.search(line):
            out.append(f"io_uring_setup opened a ring, which can connect where this trace cannot see: {line.strip()}")
            continue
        m = CALL.match(line.strip())
        if not m or m[1] not in TRACED or m[1] == "io_uring_setup":
            continue
        call, rest = m[1], m[2]
        families = FAMILY.findall(rest)
        paths = UNIX_PATH.findall(rest)
        refused = [f for f in families if f not in ALLOWED_FAMILIES and f != "AF_UNIX"]
        refused += [f"AF_UNIX {p}" for p in paths if p.strip('"') not in ALLOWED_UNIX]
        if "AF_UNIX" in families and not paths:
            refused.append("AF_UNIX with no path")
        if call == "connect" and not families:
            refused.append("an address strace could not read")
        elif RAW_ADDRESS[call].search(rest):
            refused.append("an address strace could not read")
        if refused:
            out.append(f"{call} to {', '.join(refused)}: {line.strip()}")
    return out


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        print("usage: no-network.py <trace-file> <command> [args...]", file=sys.stderr)
        return 2
    trace, command = Path(argv[1]), argv[2:]
    missing = [tool for tool in ("unshare", "strace") if shutil.which(tool) is None]
    if missing:
        print(f"no-network: {' and '.join(missing)} not found; this runs on Linux only", file=sys.stderr)
        return 2
    if subprocess.run(["unshare", "--user", "--map-root-user", "--net", "true"], capture_output=True).returncode:
        print(
            "no-network: cannot make a user and network namespace; on Ubuntu 24.04 run "
            "`sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`",
            file=sys.stderr,
        )
        return 2
    trace.unlink(missing_ok=True)
    status = subprocess.run(
        [
            "unshare", "--user", "--map-root-user", "--net",
            "strace", "--follow-forks", "--seccomp-bpf", "-qq", "-e", "signal=none",
            "-s", "256", "-e", f"trace={','.join(TRACED)}", "-e", "inject=io_uring_setup:error=ENOSYS",
            "-o", str(trace), "--",
            *command,
        ],
        env=os.environ,
    ).returncode
    if not trace.exists():
        print(f"no-network: strace wrote no trace to {trace}", file=sys.stderr)
        return 2
    problems = findings(trace.read_text(errors="replace"))
    for problem in problems:
        print(f"no-network: {problem}", file=sys.stderr)
    if problems:
        print(
            f"no-network: {command[0]} tried to reach the network {len(problems)} time(s); "
            "the add-ons and a fixture render reach nothing (ADR-024, ADR-058)",
            file=sys.stderr,
        )
        return 1
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv))
