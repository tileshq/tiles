#!/usr/bin/env python3
"""Fail when a release `tiles` binary carries readable chart assets or source paths.

The forms and design tokens ship sealed (ADR-022), and a release build remaps
every source path (scripts/release-rustflags.sh). This scans the raw bytes of
the binary, which finds everything `strings` would, for:

    paths   a home directory (/Users/, /home/), a cargo registry, a Tiles
            crate's source path (crates/tiles-*, tiles-*/src/), or any path
            passed with --forbid
    assets  every hex colour in the assets the build sealed, in quotes and bare (bare
            #ffffff is left out: x86 code spells it, a jump byte 0x23 then
            0x66 padding), every form text of 16 or more characters
            (summaries, role notes), and every asset key written as a JSON
            key ("key":), less the few the add-ons `tiles` embeds also use
            (crates/tiles-cli/src/addons.rs)

The assets are the source tree's plain tokens (`assets/plain/tokens.json`,
ADR-037) and forms, or, for a build that took a design pack
(`TILES_DESIGN_PACK`, ADR-036), the pack's `tokens.json` and the tree's
forms, as the build seals them. The script opens the pack itself, so the
token needles are whatever that pack holds.

    python3 scripts/check-release-binary.py target/release/tiles --forbid "$PWD" [--pack <file>]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PATHS = (b"/Users/", b"/home/", b".cargo/registry", b"crates/tiles-")
# Bytes x86 code holds anyway: `jmp` (0x23 operand) then 0x66 NOP prefixes.
CODE_SPELLED = {"#ffffff"}
TEXT_MIN = 16
ADDON_SUFFIXES = (".ts", ".tsx", ".json", ".md")


# ChaCha20 (RFC 8439) and the key derivation of crates/tiles-engine/src/chacha.rs,
# to open a design pack (crates/tiles-engine/src/pack.rs, ADR-036).
SIGMA = (0x61707865, 0x3320646E, 0x79622D32, 0x6B206574)
DERIVE = (0x74696C65, 0x732D6B65, 0x79000001)
MASK = 0xFFFFFFFF
PACK_MAGIC = b"tiles-design-pack 1"


def rotl(v: int, n: int) -> int:
    return ((v << n) & MASK) | (v >> (32 - n))


def block(key: list[int], counter: int, nonce: list[int]) -> list[int]:
    init = [*SIGMA, *key, counter, *nonce]
    s = init[:]

    def quarter(a: int, b: int, c: int, d: int) -> None:
        s[a] = (s[a] + s[b]) & MASK
        s[d] = rotl(s[d] ^ s[a], 16)
        s[c] = (s[c] + s[d]) & MASK
        s[b] = rotl(s[b] ^ s[c], 12)
        s[a] = (s[a] + s[b]) & MASK
        s[d] = rotl(s[d] ^ s[a], 8)
        s[c] = (s[c] + s[d]) & MASK
        s[b] = rotl(s[b] ^ s[c], 7)

    for _ in range(10):
        for a, b, c, d in ((0, 4, 8, 12), (1, 5, 9, 13), (2, 6, 10, 14), (3, 7, 11, 15)):
            quarter(a, b, c, d)
        for a, b, c, d in ((0, 5, 10, 15), (1, 6, 11, 12), (2, 7, 8, 13), (3, 4, 9, 14)):
            quarter(a, b, c, d)
    return [(x + y) & MASK for x, y in zip(s, init)]


def derive(a: list[int], b: list[int]) -> list[int]:
    seed = [a[i] ^ rotl(b[7 - i], i * 3 + 1) for i in range(8)]
    return block(seed, 0, list(DERIVE))[:8]


def nonce(path: str) -> list[int]:
    h = 0xCBF29CE484222325
    for byte in path.encode():
        h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return [h & MASK, h >> 32, len(path.encode())]


def apply(key: list[int], path: str, data: bytes) -> bytes:
    out = bytearray(data)
    for i in range(0, len(out), 64):
        stream = b"".join(w.to_bytes(4, "little") for w in block(key, i // 64 + 1, nonce(path)))
        for j in range(min(64, len(out) - i)):
            out[i + j] ^= stream[j]
    return bytes(out)


def open_pack(data: bytes) -> dict[str, str]:
    """The asset of a design pack, its `tokens.json`, opened: path to plain text."""
    head, sep, body = data.partition(b"\nend\n")
    lines = head.decode().split("\n")
    if not sep or lines[0] != PACK_MAGIC.decode():
        raise ValueError("not a design pack")
    fields = dict(line.split(" ", 1) for line in lines[1:4])
    words = lambda hex_text: [int.from_bytes(bytes.fromhex(hex_text)[i : i + 4], "little") for i in range(0, 32, 4)]
    key = derive(words(fields["seed-a"]), words(fields["seed-b"]))
    assets: dict[str, str] = {}
    for line in lines[4:]:
        _, path, size = line.split(" ")
        sealed, body = body[: int(size)], body[int(size) :]
        assets[path] = apply(key, path, sealed).decode()
    return assets


def asset_texts(root: Path, pack: Path | None) -> list[str]:
    """The plain text of every asset the build sealed."""
    assets = root / "crates/tiles-engine/assets"
    texts = {"tokens.json": (assets / "plain/tokens.json").read_text()}
    for form in sorted((assets / "forms").glob("*.json")):
        texts[f"forms/{form.name}"] = form.read_text()
    if pack is not None:
        texts["tokens.json"] = open_pack(pack.read_bytes())["tokens.json"]
    return [texts[path] for path in sorted(texts)]


def walk(value, keys: set[str], texts: set[str]) -> None:
    if isinstance(value, dict):
        for key, inner in value.items():
            keys.add(key)
            walk(inner, keys, texts)
    elif isinstance(value, list):
        for inner in value:
            walk(inner, keys, texts)
    elif isinstance(value, str):
        texts.add(value)


def addon_text(root: Path) -> str:
    adapters = root / "adapters"
    files = sorted(p for p in adapters.rglob("*") if p.suffix in ADDON_SUFFIXES and p.is_file())
    return "\n".join(p.read_text(errors="replace") for p in files)


def colour_pattern(text: str) -> re.Pattern[bytes]:
    colour = re.escape(text.encode())
    return re.compile(b'"' + colour + b'"' if text.lower() in CODE_SPELLED else colour, re.I)


def colour_needles(tokens: dict) -> list[tuple[str, re.Pattern[bytes]]]:
    """Each hex colour of one tokens file, as needles; scripts/public-scan.py
    looks for the house tokens' in the public tree (ADR-059)."""
    texts: set[str] = set()
    walk(tokens, set(), texts)
    return [(f"colour {t}", colour_pattern(t)) for t in sorted(texts) if re.fullmatch(r"#[0-9a-fA-F]{6}", t)]


def needles(root: Path, pack: Path | None = None) -> list[tuple[str, re.Pattern[bytes]]]:
    """Each asset leak to look for, with a name for the report."""
    keys: set[str] = set()
    texts: set[str] = set()
    for text in asset_texts(root, pack):
        walk(json.loads(text), keys, texts)
    addons = addon_text(root)
    found: list[tuple[str, re.Pattern[bytes]]] = []
    for text in sorted(texts):
        if re.fullmatch(r"#[0-9a-fA-F]{6}", text):
            found.append((f"colour {text}", colour_pattern(text)))
        elif len(text) >= TEXT_MIN:
            found.append((f"form text {text[:40]!r}", re.compile(re.escape(text.encode()))))
    for key in sorted(keys):
        json_key = rf'"{re.escape(key)}"\s*:'
        if re.search(json_key, addons):
            continue
        found.append((f"JSON key {key!r}", re.compile(json_key.encode())))
    return found


def crate_paths(root: Path) -> list[bytes]:
    return [f"{p.name}/src/".encode() for p in sorted((root / "crates").iterdir()) if p.is_dir()]


def scan(binary: bytes, root: Path, forbid: list[str], pack: Path | None = None) -> list[str]:
    problems: list[str] = []
    for path in [*PATHS, *crate_paths(root), *(f.rstrip("/").encode() for f in forbid if f)]:
        count = binary.count(path)
        if count:
            at = binary.find(path)
            sample = binary[max(0, at - 40) : at + 80].decode("ascii", "replace")
            sample = re.sub(r"[^\x20-\x7e]", ".", sample)
            problems.append(f"path {path.decode()!r} appears {count} times, as in: {sample}")
    for name, pattern in needles(root, pack):
        if pattern.search(binary):
            problems.append(f"asset {name} is readable")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("binary", type=Path)
    parser.add_argument("--forbid", action="append", default=[], help="a path that must not appear")
    parser.add_argument("--pack", type=Path, help="the design pack the build took (TILES_DESIGN_PACK)")
    args = parser.parse_args()
    problems = scan(args.binary.read_bytes(), ROOT, args.forbid, args.pack)
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        print(f"{args.binary}: {len(problems)} leaks; see ADR-022", file=sys.stderr)
        return 1
    print(f"{args.binary}: no source path and no readable form, token or colour.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
