#!/usr/bin/env python3
"""Fail when Tiles could send anything to anyone but the documented owners (ADR-005, ADR-024, ADR-031, ADR-032).

The documented owners: at run time the `tiles` binary talks only to the Omni
host the user configures, through `tiles-omni`, to BigQuery and Google's
token endpoint, through `tiles-warehouse`, and, for the daily update check,
to api.github.com, through `tiles-cli`'s update code, all over the shared
client in `tiles-net`; the harness add-ons talk to nothing and run only the
`tiles` binary. Tiles sends no telemetry, crash report or analytics, and
collects no personal data.

Checks:
  1. No workspace crate outside NETWORK_OWNERS reaches a networking crate
     through its normal (run-time) dependencies, except through a workspace
     NETWORK_OWNERS crate. Build and dev dependencies are ignored:
     skia-bindings downloads Skia at build time, never at run time.
  2. No source file of such a crate names std::net or a socket type.
  3. No workspace crate, the owner included, reaches a telemetry, crash
     reporting or analytics crate at run time.
  4. No shipped Rust source (test modules and tests.rs files excluded) names
     an http(s) URL whose host is not an Omni host, one of the two BigQuery
     hosts, a placeholder for the configured one, or one of the two GitHub
     hosts, which only crates/tiles-cli/src/update.rs may name.
  5. No add-on file under adapters/ (tests excluded) names a URL or a
     network API (fetch, node:http, node:net, WebSocket and the like), and
     an add-on that imports child_process imports it by name only (no
     namespace, default, aliased, required or dynamic import, whose calls
     this check could not see) and spawns only the tiles binary.

Checks 1 and 3 read cargo's resolved dependency graph and are the evidence
that nothing sends telemetry; checks 2, 4 and 5 read source text and are a
lint that catches the plain ways to add a destination, not a proof.

Reads `cargo metadata`, so run it where cargo works.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from collections import deque
from pathlib import Path

NETWORK_OWNERS = {"tiles-net", "tiles-omni", "tiles-warehouse"}
NETWORK_CRATES = {
    "async-std", "attohttpc", "curl", "curl-sys", "h2", "h3", "hickory-resolver", "hyper",
    "isahc", "native-tls", "openssl", "quinn", "reqwest", "rustls", "socket2", "surf",
    "tokio-tungstenite", "trust-dns-resolver", "tungstenite", "ureq",
}
SOCKET_SOURCE = re.compile(r"\bstd::net\b|\b(TcpStream|TcpListener|UdpSocket)\b")
TELEMETRY_CRATES = {
    "amplitude", "bugsnag", "cadence", "datadog-apm", "dogstatsd", "honeycomb", "keen",
    "metrics-exporter-prometheus", "mixpanel", "newrelic", "opentelemetry", "opentelemetry-http",
    "opentelemetry-otlp", "opentelemetry_sdk", "posthog-rs", "rollbar", "rudderanalytics",
    "segment", "sentry", "sentry-core", "statsd", "tracing-opentelemetry",
}
URL = re.compile(r"\bhttps?://([^/\s\"'`<>)\\]*)")
# An Omni host (the docs' examples and the host rule), one of the two
# BigQuery hosts with no wildcard (ADR-031), one of the two GitHub hosts the
# update code names (ADR-032), or a placeholder that formats in the host the
# user configured. An empty host is a scheme check.
DOCUMENTED_HOST = re.compile(
    r"^(\{[a-z_]*\}|[a-z0-9-]+\.omniapp\.co|bigquery\.googleapis\.com|oauth2\.googleapis\.com"
    r"|api\.github\.com|github\.com)?$",
    re.IGNORECASE,
)
# Hosts only one file may name, as (crate, path under the crate): the update
# check's metadata GET and the release page its steps by hand open.
HOST_OWNERS = {
    "api.github.com": ("tiles-cli", "src/update.rs"),
    "github.com": ("tiles-cli", "src/update.rs"),
}
ADDON_SUFFIXES = {".ts", ".tsx", ".js", ".mjs", ".cjs", ".json", ".md"}
ADDON_NETWORK = re.compile(
    r"\bfetch\s*\(|\bnode:(?:http2?|https|net|tls|dgram|dns)\b"
    r"|\brequire\(\s*['\"](?:http2?|https|net|tls|dgram|dns|undici)['\"]|['\"]undici['\"]"
    r"|\b(?:WebSocket|XMLHttpRequest|EventSource|sendBeacon)\b|https?://"
)
CHILD_PROCESS = re.compile(r"['\"](?:node:)?child_process['\"]")
NAMED_IMPORT = re.compile(r"\bimport\s*\{([\w\s,]*)\}\s*from\s*['\"](?:node:)?child_process['\"]")
SPAWN = re.compile(r"(?<![\w.])(spawn|spawnSync|execFile|execFileSync|exec|execSync|fork)\s*\(\s*([^,)]*)")


def runtime_reach(metadata: dict) -> list[str]:
    """Problems for every member outside NETWORK_OWNERS that reaches a network crate."""
    packages = {p["id"]: p for p in metadata["packages"]}
    edges: dict[str, list[str]] = {}
    for node in metadata["resolve"]["nodes"]:
        edges[node["id"]] = [
            dep["pkg"]
            for dep in node.get("deps", [])
            if any(kind.get("kind") is None for kind in dep.get("dep_kinds", []))
        ]
    members = set(metadata["workspace_members"])
    problems = []
    for member in metadata["workspace_members"]:
        name = packages[member]["name"]
        if name in NETWORK_OWNERS:
            continue
        seen = {member: [name]}
        queue = deque([member])
        while queue:
            current = queue.popleft()
            for dep in edges.get(current, []):
                if dep in seen:
                    continue
                path = seen[current] + [packages[dep]["name"]]
                seen[dep] = path
                if dep in members and packages[dep]["name"] in NETWORK_OWNERS:
                    continue
                if packages[dep]["name"] in NETWORK_CRATES:
                    problems.append(f"{name} reaches {packages[dep]['name']}: {' -> '.join(path)}")
                else:
                    queue.append(dep)
    return problems


def socket_sources(metadata: dict) -> list[str]:
    problems = []
    for package in metadata["packages"]:
        if package["id"] not in metadata["workspace_members"] or package["name"] in NETWORK_OWNERS:
            continue
        src = Path(package["manifest_path"]).parent / "src"
        for path in sorted(src.rglob("*.rs")):
            for number, line in enumerate(path.read_text().split("\n"), 1):
                if SOCKET_SOURCE.search(line):
                    problems.append(f"{path}:{number}: {package['name']} opens sockets")
    return problems


def telemetry_reach(metadata: dict) -> list[str]:
    """Problems for every member, the network owner included, that reaches a telemetry crate."""
    packages = {p["id"]: p for p in metadata["packages"]}
    edges: dict[str, list[str]] = {}
    for node in metadata["resolve"]["nodes"]:
        edges[node["id"]] = [
            dep["pkg"]
            for dep in node.get("deps", [])
            if any(kind.get("kind") is None for kind in dep.get("dep_kinds", []))
        ]
    problems = []
    for member in metadata["workspace_members"]:
        name = packages[member]["name"]
        seen = {member: [name]}
        queue = deque([member])
        while queue:
            current = queue.popleft()
            for dep in edges.get(current, []):
                if dep in seen:
                    continue
                seen[dep] = seen[current] + [packages[dep]["name"]]
                if packages[dep]["name"] in TELEMETRY_CRATES:
                    problems.append(f"{name} reaches telemetry crate {packages[dep]['name']}: {' -> '.join(seen[dep])}")
                else:
                    queue.append(dep)
    return problems


def shipped_lines(path: Path) -> list[tuple[int, str]]:
    """The numbered lines of a Rust file that ship: comment lines dropped, a
    `#[cfg(test)] mod x;` line skipped and an inline `#[cfg(test)]` module cut
    to the end of the file, where every crate keeps it."""
    lines = path.read_text().split("\n")
    out = []
    skip_next = False
    for number, line in enumerate(lines, 1):
        stripped = line.strip()
        if skip_next:
            skip_next = False
            continue
        if stripped == "#[cfg(test)]":
            following = next((l.strip() for l in lines[number:] if l.strip()), "")
            if re.fullmatch(r"(pub(\(crate\))? )?mod \w+;", following):
                skip_next = True
                continue
            break
        if stripped.startswith("//"):
            continue
        out.append((number, line))
    return out


def url_sources(metadata: dict) -> list[str]:
    problems = []
    for package in metadata["packages"]:
        if package["id"] not in metadata["workspace_members"]:
            continue
        root = Path(package["manifest_path"]).parent
        files = sorted((root / "src").rglob("*.rs")) + [root / "build.rs"]
        for path in files:
            if not path.exists() or path.name == "tests.rs":
                continue
            for number, line in shipped_lines(path):
                for host in URL.findall(line):
                    owner = HOST_OWNERS.get(host.lower())
                    if not DOCUMENTED_HOST.match(host):
                        problems.append(f"{path}:{number}: {package['name']} names {host or 'a URL'}, not a documented host")
                    elif owner and (package["name"], path.relative_to(root).as_posix()) != owner:
                        problems.append(
                            f"{path}:{number}: {package['name']} names {host}, which only {owner[0]}'s {owner[1]} may name"
                        )
    return problems


def addon_sources(root: Path) -> list[str]:
    problems = []
    adapters = root / "adapters"
    for path in sorted(adapters.rglob("*")) if adapters.exists() else []:
        if path.suffix not in ADDON_SUFFIXES or ".test." in path.name or not path.is_file():
            continue
        text = path.read_text()
        spawns_ok = not CHILD_PROCESS.search(text)
        named = {
            m.end()
            for m in NAMED_IMPORT.finditer(text)
            if all(re.fullmatch(r"\w+", name.strip()) for name in m[1].split(",") if name.strip())
        }
        for m in CHILD_PROCESS.finditer(text):
            if m.end() not in named:
                number = text.count("\n", 0, m.start()) + 1
                problems.append(f"{path}:{number}: an add-on imports child_process other than by name")
        for number, line in enumerate(text.split("\n"), 1):
            if ADDON_NETWORK.search(line):
                problems.append(f"{path}:{number}: an add-on reaches the network")
            if spawns_ok:
                continue
            for call, program in SPAWN.findall(line):
                if call not in {"spawn", "spawnSync", "execFile", "execFileSync"} or "tiles" not in program:
                    problems.append(f"{path}:{number}: an add-on runs {program.strip() or call}, not tiles")
    return problems


def main() -> int:
    raw = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        check=True, capture_output=True, text=True,
    ).stdout
    metadata = json.loads(raw)
    root = Path(__file__).resolve().parent.parent
    problems = (
        runtime_reach(metadata)
        + socket_sources(metadata)
        + telemetry_reach(metadata)
        + url_sources(metadata)
        + addon_sources(root)
    )
    # The public tree has no tiles-warehouse (ADR-059), so name only the
    # owners this workspace holds.
    names = {p["name"] for p in metadata["packages"] if p["id"] in metadata["workspace_members"]}
    owners = sorted(NETWORK_OWNERS & names)
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        print(
            f"Only {owners} may reach the network, and only the configured Omni host, BigQuery's two hosts "
            "and the update code's two GitHub hosts; the add-ons reach nothing and nothing sends telemetry "
            "(ADR-005, ADR-024, ADR-031, ADR-032).",
            file=sys.stderr,
        )
        return 1
    members = len(metadata["workspace_members"])
    print(
        f"No crate outside {owners} can reach the network, no code names a host but Omni's, BigQuery's "
        "and, in the update code only, GitHub's, "
        f"no add-on reaches the network, and nothing sends telemetry ({members} crates checked)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
