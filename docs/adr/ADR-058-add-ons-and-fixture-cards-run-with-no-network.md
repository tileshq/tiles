# ADR-058: CI runs the add-ons' tests and fixture cards with no network and fails on any try to reach one

- Status: Accepted; narrows [ADR-024](ADR-024-security-baseline-in-ci.md)'s "review is what stops" a disguised destination for the paths it runs; adds `no-network` to [ADR-055](ADR-055-main-skips-a-tree-its-pull-request-passed.md)'s gated jobs
- Date: 2026-10-22

## Context

[ADR-005](ADR-005-privacy-and-secrecy.md) and [ADR-024](ADR-024-security-baseline-in-ci.md) say Tiles sends no telemetry and the add-ons reach no network. `scripts/check-network-owners.py` holds that from cargo's dependency graph and from source text. Its own header calls the source checks a lint: a host built at run time or `globalThis["fe" + "tch"]` in an add-on passes it, and ADR-024 left that case to review. Nothing ran the add-ons or the binary and watched what they did.

The documented owners are the Omni host the user configures, BigQuery and Google's token endpoint, and `api.github.com` for the daily update check ([ADR-031](ADR-031-warehouse-sources.md), [ADR-032](ADR-032-update-command-and-harness-versions.md)), all through `tiles-net`. A card drawn from inline rows needs none of them.

## Decision

### The job

A `no-network` job on `ubuntu-24.04` runs three steps, the last two under `scripts/no-network.py` and the first through it:

1. The `NoNetwork` tests in `scripts/test_policy_scripts.py`, with `TILES_NO_NETWORK_REQUIRED=1` so a missing sandbox fails them instead of skipping. Each probe runs under the wrapper, swallows its own error and exits 0, so only the trace can fail it: a Python and a node connect to `192.0.2.1:443`, and a Python name lookup. Another shows node runs and exits 0 with its io_uring refused. The last runs `tiles doctor` with a fresh update-check state: the trace fails it and the state file shows the check ran; with `TILES_UPDATE_CHECK=off` the same run passes.
2. The add-on tests, `node --test adapters/pi/index.test.ts adapters/claude/card.test.ts`.
3. `scripts/drive-addon-paths.py`, which makes the calls the add-ons make with no Omni and no warehouse configured: `tiles card -` for each of the 17 intents in `crates/tiles-engine/tests/intents/` with `TILES_DRAW=image` and with `TILES_DRAW=block` (each must draw), `tiles card -` for each worst-case intent (each must draw or refuse in one JSON line), `tiles theme`, `tiles theme dark`, and one `tiles mcp` session with a `tiles_chart` and a `tiles_theme` call. Pi's extension runs `tiles card -` and `tiles theme`; Claude Code and Codex reach Tiles through `tiles mcp`.

The job uses the binary the `release-binary` job builds, uploaded as an artifact kept one day, so it adds no Rust build. `release-binary` now sets `TILES_VERSION=0.0.1`, as the release job sets its tag's version: a build without one is 0.0.0, which never runs the update check, and a trace of it would show nothing about that check. The job needs `main-gate` and `release-binary` and is skipped with the other heavy jobs on a push to main whose tree a pull request already passed (ADR-055).

### The sandbox

`unshare --user --map-root-user --net` (`unshare -rn`) puts the command in a network namespace of its own, whose only interface is a loopback that is down, so the command's own packets go nowhere. `strace --follow-forks --seccomp-bpf` records every `connect`, `sendto`, `sendmsg`, `sendmmsg` and `io_uring_setup` the command and its children make, failed calls included, and `scripts/no-network.py` reads the trace. strace also answers every `io_uring_setup` with `ENOSYS` (`-e inject=io_uring_setup:error=ENOSYS`), so no ring opens: node 22's libuv asks for one per event loop, and the job's first run failed on those ten calls, though the node tests made no connect. libuv falls back to epoll, whose sockets the trace sees. `unshare` ships in util-linux on the runner image and strace is one small package; with `--seccomp-bpf` strace stops a process only on those five calls, so node and the renders run at close to full speed. bubblewrap would add a package for the same namespace and no record of the attempts; an iptables or eBPF rule needs root on the whole runner and sees no Unix socket. Ubuntu 24.04's AppArmor refuses unprivileged user namespaces by default, so the job sets `kernel.apparmor_restrict_unprivileged_userns=0` first.

### The allow rule

The trace permits exactly:

- a send with no address: on a socket already connected, whose `connect` the trace judged, or on a socket pair;
- a `connect` or send to an `AF_NETLINK` address, which reaches the kernel only (glibc asks it for the machine's interfaces);
- a `connect` to a filesystem `AF_UNIX` socket listed in `ALLOWED_UNIX`, which is empty.

Everything else fails the job: any `AF_INET` or `AF_INET6` address, loopback included (the stub resolver at `127.0.0.53` is DNS); any other `AF_UNIX` socket, since systemd-resolved, nscd, D-Bus and Docker each listen on one and would reach the network for the caller; any other family; an address strace could not decode; and an `io_uring_setup` that opened a ring despite the injected error, since a ring can connect without a system call strace sees. A DNS lookup reaches its resolver through one of these, so the job fails on it. No documented owner is allowed.

### The update check

`tiles card`, `tiles theme` and `tiles mcp` never run the daily update check. `update::refresh` runs only from `tiles doctor` and from `tiles run` (not `--dry-run`), and the add-ons set no `TILES_UPDATE_CHECK`. The job shows it: `drive-addon-paths.py` gives every call a fresh state directory, `refresh` writes its state file before it fetches, and the driver fails if the file appears; the `tiles doctor` test shows the same binary's check fails the trace when it does run.

## Consequences

- A destination hidden from `check-network-owners.py` in add-on code the tests run, or in the `tiles card` and `tiles mcp` paths the fixtures draw, now fails CI. The lint and the dependency checks stay as they are.
- The job does not prove anything about code the fixtures do not run: a live Omni or warehouse query, a lookup tool, `tiles run`, `tiles doctor` with the check on, `tiles update`, or the macOS-only code (the Keychain and the companion window). Code that hands a request to a process outside the traced tree over a channel the rule permits would also pass; only kernel netlink and unaddressed sends are permitted, and nothing outside the tree listens on either.
- A name lookup that glibc sends to systemd-resolved over its Unix socket is answered by the host, so that one query leaves the runner before the trace fails the job. Only the `NoNetwork` tests make lookups on purpose, and theirs name `tiles-no-network.invalid` and `api.github.com`.
- A contributor on Linux runs the same check with `sudo apt-get install strace`, then `python3 scripts/no-network.py <trace> <command>`; on macOS the probe tests skip and the trace tests still run.
- In the job's first run, its steps up to the add-on tests took 20 seconds after `release-binary` finished, 6 of them installing strace.
