#!/usr/bin/env python3
"""Pin the Homebrew formula to a release, from that release's SHA256SUMS (ADR-061).

    bump-formula.py <tag> <SHA256SUMS>             check the tag and the sums only
    bump-formula.py <tag> <SHA256SUMS> <formula>   also rewrite the formula in place

The rewrite changes the version in both tarball urls and in the test block, and
both sha256 values. A formula that already says all of that is left untouched
and the script succeeds, so a re-run of the release job changes nothing. Anything
else exits 1 with nothing written: a tag that is not vMAJOR.MINOR.PATCH, sums
without exactly one good line per tarball, a formula of another shape, or a
rewrite that would change a formula already at this version or a newer one.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin")
TAG = re.compile(r"v(\d+)\.(\d+)\.(\d+)")
SUM_LINE = re.compile(r"([0-9a-f]{64}) [ *](\S+)")
PIN = re.compile(
    r'(url "https://github\.com/tileshq/tiles/releases/download/)v([^/"-]+)/tiles-v([^/"-]+)-'
    r'([^"]+?)(\.tar\.gz"\n\s*sha256 ")([0-9a-f]{64})"'
)
TEST_VERSION = re.compile(r'(assert_match ")[^"]*(", shell_output\("#\{bin\}/tiles --version"\))')


class Refused(Exception):
    pass


def parse_version(text: str) -> tuple[int, ...]:
    return tuple(int(part) for part in text.split("."))


def parse_tag(tag: str) -> str:
    if not TAG.fullmatch(tag):
        raise Refused(f"{tag} is a prerelease or not a vMAJOR.MINOR.PATCH tag; brew installs only final releases")
    return tag[1:]


def tarball_hashes(sums: str, tag: str) -> dict[str, str]:
    found: dict[str, list[str]] = {}
    for number, line in enumerate(sums.splitlines(), 1):
        match = SUM_LINE.fullmatch(line)
        if not match:
            raise Refused(f"line {number} of SHA256SUMS is not a sha256 and a file name")
        found.setdefault(match[2], []).append(match[1])
    hashes = {}
    for target in TARGETS:
        name = f"tiles-{tag}-{target}.tar.gz"
        if len(found.get(name, [])) != 1:
            raise Refused(f"SHA256SUMS has {len(found.get(name, []))} lines for {name}, not one")
        hashes[target] = found[name][0]
    return hashes


def rewrite(formula: str, version: str, hashes: dict[str, str]) -> tuple[str, str]:
    """Return the rewritten formula and the highest version the old one pins."""
    pins = list(PIN.finditer(formula))
    if sorted(pin[4] for pin in pins) != sorted(TARGETS) or any(pin[2] != pin[3] for pin in pins):
        raise Refused("the formula does not have one url and sha256 pair of one version per tarball")
    if len(TEST_VERSION.findall(formula)) != 1:
        raise Refused("the formula's test block does not have one tiles --version assertion")
    old = max((pin[2] for pin in pins), key=parse_version)

    def pin_line(pin: re.Match[str]) -> str:
        return f'{pin[1]}v{version}/tiles-v{version}-{pin[4]}{pin[5]}{hashes[pin[4]]}"'

    new = TEST_VERSION.sub(rf"\g<1>{version}\g<2>", PIN.sub(pin_line, formula))
    return new, old


def bump(tag: str, sums: str, formula_path: Path | None) -> str:
    version = parse_tag(tag)
    hashes = tarball_hashes(sums, tag)
    if formula_path is None:
        return f"{tag} and its SHA256SUMS can pin the formula"
    formula = formula_path.read_text()
    new, old = rewrite(formula, version, hashes)
    if new == formula:
        return f"the formula already pins {version}; nothing to do"
    if parse_version(old) >= parse_version(version):
        raise Refused(f"the formula pins {old}, and {tag} would change it to other values for {version}")
    if old in new:
        raise Refused(f"the formula names {old} somewhere this script does not rewrite")
    formula_path.write_text(new)
    return f"the formula moved from {old} to {version}"


def main(argv: list[str]) -> int:
    if len(argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        return 2
    try:
        print(bump(argv[0], Path(argv[1]).read_text(), Path(argv[2]) if len(argv) == 3 else None))
    except (Refused, OSError) as error:
        print(f"::error::bump-formula: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
