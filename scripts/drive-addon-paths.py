#!/usr/bin/env python3
"""Run the tiles calls the harness add-ons make, over the repo's own intents (ADR-058).

    python3 scripts/drive-addon-paths.py <tiles>

Pi's extension runs `tiles card -` with the intent on standard input and
`tiles theme [name]`; Claude Code and Codex talk to `tiles mcp` over stdio.
This runs each of them with no Omni and no warehouse configured:

  - `tiles card -` for every intent in crates/tiles-engine/tests/intents/,
    with TILES_DRAW=image and TILES_DRAW=block, each of which must draw, and
    for every worst-case intent, which must draw or refuse in one JSON line;
  - `tiles theme` and `tiles theme dark`;
  - one `tiles mcp` session: initialize, tools/list, a tiles_chart call and
    a tiles_theme call.

It runs under scripts/no-network.py in CI, which fails the job on any try to
reach the network. HOME, the cache and the update check's state are fresh
temporary directories, so a daily update check would be due if any of these
calls ran it, and nothing on the machine running this is read or changed.
Exits 1 when a call does not answer as expected.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INTENTS = ROOT / "crates/tiles-engine/tests/intents"


def environment(home: Path, draw: str) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith(("TILES_", "XDG_"))}
    env.update(
        HOME=str(home),
        TILES_CACHE_DIR=str(home / "cache"),
        TILES_STATE_DIR=str(home / "state"),
        TILES_DRAW=draw,
    )
    return env


def card(tiles: str, intent: Path, env: dict[str, str]) -> dict:
    done = subprocess.run([tiles, "card", "-"], input=intent.read_bytes(), env=env, capture_output=True, timeout=60)
    try:
        return json.loads(done.stdout)
    except ValueError:
        return {"ok": None, "error": f"no JSON answer (exit {done.returncode}): {done.stderr[-300:]!r}"}


def mcp(tiles: str, intent: Path, env: dict[str, str]) -> list[dict]:
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "no-network", "version": "1"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "tiles_chart", "arguments": json.loads(intent.read_text())}},
        {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "tiles_theme", "arguments": {}}},
    ]
    stdin = "".join(json.dumps(r) + "\n" for r in requests)
    done = subprocess.run([tiles, "mcp"], input=stdin, env=env, capture_output=True, text=True, timeout=60)
    return [json.loads(line) for line in done.stdout.splitlines() if line.strip()]


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: drive-addon-paths.py <tiles>", file=sys.stderr)
        return 2
    tiles = argv[1]
    problems = []
    drawn = 0
    with tempfile.TemporaryDirectory() as tmp:
        home = Path(tmp)
        for draw in ("image", "block"):
            env = environment(home, draw)
            for intent in sorted(INTENTS.glob("*.json")):
                answer = card(tiles, intent, env)
                if answer.get("ok") is True:
                    drawn += 1
                else:
                    problems.append(f"tiles card {intent.name} (TILES_DRAW={draw}) did not draw: {answer.get('error')}")
        env = environment(home, "image")
        for intent in sorted((INTENTS / "worst-case").glob("*.json")):
            answer = card(tiles, intent, env)
            if answer.get("ok") is True:
                drawn += 1
            elif answer.get("ok") is not False:
                problems.append(f"tiles card worst-case/{intent.name} gave no answer: {answer.get('error')}")
        for args in (["theme"], ["theme", "dark"]):
            done = subprocess.run([tiles, *args], env=env, capture_output=True, text=True, timeout=60)
            if done.returncode:
                problems.append(f"tiles {' '.join(args)} failed: {done.stderr.strip()[-300:]}")
        replies = {r.get("id"): r for r in mcp(tiles, INTENTS / "column.json", env)}
        tools = [t["name"] for t in replies.get(2, {}).get("result", {}).get("tools", [])]
        if "tiles_chart" not in tools or "tiles_theme" not in tools:
            problems.append(f"tiles mcp listed {tools}, not tiles_chart and tiles_theme")
        for id_, name in ((3, "tiles_chart"), (4, "tiles_theme")):
            result = replies.get(id_, {}).get("result")
            if not result or result.get("isError"):
                problems.append(f"tiles mcp {name} failed: {replies.get(id_)}")
            elif name == "tiles_chart":
                drawn += 1
        update_state = home / "state" / "update-check.json"
        if update_state.exists():
            problems.append(f"a call ran the daily update check: {update_state.read_text()[:200]}")
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        return 1
    print(f"{drawn} cards drawn through tiles card and tiles mcp, and tiles theme listed and set a theme")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
