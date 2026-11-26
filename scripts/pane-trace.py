#!/usr/bin/env python3
"""Turns a `tiles pane --trace` file into the go or no-go table (ADR-063).

    python3 scripts/pane-trace.py <trace>... [--machine work|intel] [--label <terminal>]

Prints a Markdown table: each measure, its value from the traces (the first
trace that measured it, so a dashboard trace and a table stress trace make
one table), its go threshold and, with --machine, whether it passed. Measures the trace cannot
hold (the hover as seen in a screen recording, the Warp and Orca checks) are
rows for the person running the test to fill in. The trace format is in
crates/tiles-cli/src/pane/trace.rs.
"""

from __future__ import annotations

import argparse
import statistics
import sys
from pathlib import Path

# Go thresholds per machine: the work laptop and the 2017 Intel Mac (ADR-063).
GATES = {
    "hover_p95_ms": {"work": 16.0, "intel": 33.0},
    "hover_bytes_kb": {"work": 64.0, "intel": 64.0},
    "card_click_ms": {"work": 16.0, "intel": 50.0},
    "eight_cards_ms": {"work": 50.0, "intel": 150.0},
    "scroll_p95_ms": {"work": 33.0, "intel": None},
    "first_frame_ms": {"work": 200.0, "intel": 200.0},
    "rss_mb": {"work": 100.0, "intel": 100.0},
    "idle_cpu_pct": {"work": 1.0, "intel": 1.0},
}

INPUT_KINDS = {"hover", "click", "scroll"}


class Update:
    """One update: its kind, when it began, the read it answers (`start`),
    its draw marks, and its write."""

    def __init__(self, kind: str, begin: float, start: float) -> None:
        self.kind = kind
        self.begin = begin
        self.start = start
        self.draws: list[float] = []
        self.write: float | None = None
        self.bytes = 0
        self.cards = 0


class Trace:
    """Everything a trace file holds."""

    def __init__(self) -> None:
        self.updates: dict[int, Update] = {}
        self.reads: list[float] = []
        self.rss: list[tuple[float, int]] = []
        self.cpu: list[tuple[float, float]] = []
        self.activity: list[float] = []
        self.start: dict[str, str] = {}


def parse(text: str) -> Trace:
    """Reads a trace; a line that does not parse is skipped."""
    trace = Trace()
    for line in text.splitlines():
        words = line.split()
        if len(words) < 2:
            continue
        try:
            at = float(words[0])
        except ValueError:
            continue
        event = words[1]
        fields = dict(w.split("=", 1) for w in words[2:] if "=" in w)
        try:
            record(trace, at, event, fields)
        except (KeyError, ValueError):
            continue
    return trace


def record(trace: Trace, at: float, event: str, f: dict[str, str]) -> None:
    if event == "start":
        trace.start = f
    elif event == "read":
        trace.reads.append(at)
        trace.activity.append(at)
    elif event == "begin":
        kind = f["kind"]
        start = at
        if kind in INPUT_KINDS:
            earlier = [r for r in trace.reads if r <= at]
            start = earlier[-1] if earlier else at
        elif kind == "first":
            start = 0.0
        trace.updates[int(f["update"])] = Update(kind=kind, begin=at, start=start)
        trace.activity.append(at)
    elif event == "draw":
        trace.updates[int(f["update"])].draws.append(at)
        trace.activity.append(at)
    elif event == "write":
        u = trace.updates[int(f["update"])]
        u.write = at
        u.bytes = int(f["bytes"])
        u.cards = int(f["cards"])
        trace.activity.append(at)
    elif event == "rss":
        trace.rss.append((at, int(f["kb"])))
    elif event == "cpu":
        trace.cpu.append((at, float(f["ms"])))


def p95(values: list[float]) -> float | None:
    """The 95th percentile by the nearest-rank method."""
    if not values:
        return None
    ordered = sorted(values)
    rank = max(1, -(-95 * len(ordered) // 100))
    return ordered[rank - 1]


def done(trace: Trace, kind: str) -> list[Update]:
    return [u for u in trace.updates.values() if u.kind == kind and u.write is not None]


def card_times(u: Update) -> list[float]:
    """Each card's draw time in an update: from the previous mark."""
    marks = [u.start, *u.draws]
    return [b - a for a, b in zip(marks, marks[1:])]


def measures(trace: Trace) -> dict[str, tuple[float | None, str]]:
    """Each measure's value and how it was taken."""
    out: dict[str, tuple[float | None, str]] = {}
    hovers = done(trace, "hover")
    lat = [u.write - u.start for u in hovers]
    out["hover_p95_ms"] = (p95(lat), f"{len(lat)} hover updates")
    sizes = [u.bytes / 1024 for u in hovers]
    out["hover_bytes_kb"] = (max(sizes) if sizes else None, f"largest of {len(sizes)}; p95 {fmt(p95(sizes), 1)} KB")
    clicks = done(trace, "click")
    per_card = [t for u in clicks for t in card_times(u)[: u.cards]]
    click_lat = [u.write - u.start for u in clicks]
    out["card_click_ms"] = (
        p95(per_card),
        f"{len(per_card)} cards over {len(clicks)} clicks; whole click p95 {fmt(p95(click_lat), 1)} ms",
    )
    full = [u for u in done(trace, "resize") if u.cards >= 8]
    how = f"{len(full)} resizes of 8 or more cards"
    if not full:
        full = [u for u in done(trace, "first") if u.cards >= 8]
        how = "the first frame from its begin (no resize in the trace)"
    redraw = [u.write - u.begin for u in full]
    out["eight_cards_ms"] = (max(redraw) if redraw else None, f"slowest of {how}")
    scrolls = [u.write - u.start for u in done(trace, "scroll")]
    out["scroll_p95_ms"] = (p95(scrolls), f"{len(scrolls)} scroll steps")
    first = done(trace, "first")
    out["first_frame_ms"] = (first[0].write if first else None, "process start to the first frame written")
    after = (first[0].write + 2000) if first else 0
    steady = [kb / 1024 for t, kb in trace.rss if t >= after]
    out["rss_mb"] = (
        statistics.median(steady) if steady else None,
        f"median of {len(steady)} samples from 2 s after the first frame",
    )
    out["idle_cpu_pct"] = idle_cpu(trace, first[0].write if first else 0)
    return out


def idle_cpu(trace: Trace, after: float) -> tuple[float | None, str]:
    """CPU over the sample intervals with no input, draw or write in them."""
    busy = sorted(trace.activity)
    wall = cpu = 0.0
    count = 0
    for (t0, c0), (t1, c1) in zip(trace.cpu, trace.cpu[1:]):
        if t0 < after or any(t0 < a <= t1 for a in busy):
            continue
        wall += t1 - t0
        cpu += c1 - c0
        count += 1
    if wall <= 0:
        return (None, "no idle interval in the trace")
    return (100 * cpu / wall, f"{count} idle seconds, sampler included")


def fmt(v: float | None, places: int = 1) -> str:
    return "not measured" if v is None else f"{v:.{places}f}"


ROWS = [
    ("hover_p95_ms", "Hover, mouse report read to bytes written, p95", "ms"),
    ("hover_bytes_kb", "Bytes per hover update", "KB"),
    ("card_click_ms", "One card redrawn after a click, p95", "ms"),
    ("eight_cards_ms", "Eight-card dashboard redrawn", "ms"),
    ("scroll_p95_ms", "Table stress scene (40 rows by 8 columns), one scroll step, p95", "ms"),
    ("first_frame_ms", "Start to first full frame", "ms"),
    ("rss_mb", "Resident memory, steady", "MB"),
    ("idle_cpu_pct", "Idle CPU with the pointer still", "%"),
]

BY_HAND = [
    "Hover as seen in a 60 fps screen recording: tooltip within 2 frames of the pointer, no flicker",
    "Fuzz target on the input parser: 60 s under AddressSanitizer, no crash",
    "Warp, in a split opened by hand: mouse reports arrive and cards replace cleanly",
    "Orca's terminal: the same, and `orca terminal split` with `orca terminal create --command` places the pane",
]


def merged(traces: list[Trace]) -> dict[str, tuple[float | None, str]]:
    """Each measure from the first trace that has a value for it."""
    each = [measures(t) for t in traces]
    return {key: next((m[key] for m in each if m[key][0] is not None), each[0][key]) for key in each[0]}


def table(traces: Trace | list[Trace], machine: str | None, label: str | None) -> str:
    traces = traces if isinstance(traces, list) else [traces]
    trace = traces[0]
    got = merged(traces)
    where = label or trace.start.get("terminal", "unknown terminal")
    head = f"Terminal: {where}; cell {trace.start.get('cell', '?')}, scale {trace.start.get('scale', '?')}, pixel mouse {trace.start.get('pixels', '?')}"
    lines = [head, ""]
    if machine:
        lines += ["| Measure | Value | Go | Result | How |", "|---|---|---|---|---|"]
    else:
        lines += ["| Measure | Value | Go, work laptop | Go, 2017 Mac | How |", "|---|---|---|---|---|"]
    for key, name, unit in ROWS:
        value, how = got[key]
        shown = f"{fmt(value, 2 if unit == '%' else 1)} {unit}" if value is not None else "not measured"
        gate = GATES[key]
        if machine:
            g = gate[machine]
            if g is None:
                lines.append(f"| {name} | {shown} | recorded | recorded | {how} |")
                continue
            result = "not measured" if value is None else ("go" if value <= g else "miss")
            lines.append(f"| {name} | {shown} | {limit(g, unit)} | {result} | {how} |")
        else:
            lines.append(
                f"| {name} | {shown} | {limit(gate['work'], unit)} | {limit(gate['intel'], unit)} | {how} |"
            )
    lines += ["", "By hand:", ""] + [f"- [ ] {item}" for item in BY_HAND]
    return "\n".join(lines) + "\n"


def limit(g: float | None, unit: str) -> str:
    if g is None:
        return "recorded"
    if unit == "%":
        return f"under {g:g}%"
    return f"{g:g} {unit} or less"


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("trace", type=Path, nargs="+")
    ap.add_argument("--machine", choices=["work", "intel"])
    ap.add_argument("--label")
    args = ap.parse_args(argv)
    traces = []
    for path in args.trace:
        try:
            traces.append(parse(path.read_text(encoding="utf-8", errors="replace")))
        except OSError as e:
            print(f"pane-trace: cannot read {path}: {e.strerror}", file=sys.stderr)
            return 1
    sys.stdout.write(table(traces, args.machine, args.label))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
