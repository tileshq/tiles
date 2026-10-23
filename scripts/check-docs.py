#!/usr/bin/env python3
"""Fail when a rule in the docs names no check, or a doc cites a missing file.

Every point in docs/definition-of-done.md must be followed by a "Checked by:"
line, and every rule row in docs/playbook.md and docs/card-rules.md must end
with a "Checked by" cell. The card rules page carries no values (no colour,
size, time, share, ratio or decimal), and while the playbook is present the
two list the same rule ids. A build tree without the playbook (ADR-037) is
checked on the card rules page alone; one of the two must exist. Each
reference must name something that exists:

    ci `job`              a job in .github/workflows/ci.yml
    lint `name`           a lint configured under [workspace.lints] in Cargo.toml
    deny `section`        a section of deny.toml
    test|script|file `p`  a path in the repository
    maintainer review     a person checks it

References are separated by " · ". Relative links in README.md, AGENTS.md and
docs/ must also resolve. Run from anywhere; paths are relative to the repo.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

REFERENCE = re.compile(r"^(ci|lint|deny|test|script|file) `([^`]+)`$")
POINT = re.compile(r"^(\d+)\. ")
CHECKED = re.compile(r"^\s+Checked by:\s*(.*)$")
RULE_ROW = re.compile(r"^\| ([A-Z]\d+) \|")
VALUE = re.compile(
    r"#[0-9a-fA-F]{3,8}\b"
    r"|(?<![\w.:])\d+(?:\.\d+)?(?:\s?(?:px|ms|em|%)(?!\w)|:\d+(?:\.\d+)?\b)"
    r"|(?<![\w.])\d*\.\d+\b"
)
PLAYBOOK = "docs/playbook.md"
CARD_RULES = "docs/card-rules.md"
LINK = re.compile(r"\]\(([^)\s]+)\)")


def ci_jobs(root: Path) -> set[str]:
    text = (root / ".github/workflows/ci.yml").read_text()
    jobs = text.split("\njobs:\n", 1)[1] if "\njobs:\n" in text else ""
    return set(re.findall(r"^ {2}([a-z][\w-]*):\s*$", jobs, re.M))


def lints(root: Path) -> set[str]:
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    names: set[str] = set()
    for tool, table in manifest.get("workspace", {}).get("lints", {}).items():
        for lint in table:
            names.add(lint if tool == "rust" else f"{tool}::{lint}")
    return names


def deny_sections(root: Path) -> set[str]:
    path = root / "deny.toml"
    return set(tomllib.loads(path.read_text())) if path.exists() else set()


def check_references(where: str, text: str, root: Path, known: dict, problems: list[str]) -> None:
    parts = [part.strip() for part in text.split("·") if part.strip()]
    if not parts:
        problems.append(f'{where}: "Checked by" is empty')
    for part in parts:
        if part == "maintainer review":
            continue
        match = REFERENCE.match(part)
        if not match:
            problems.append(f'{where}: cannot read "{part}"')
            continue
        kind, name = match.groups()
        if kind in ("ci", "lint", "deny"):
            ok = name in known[kind]
        else:
            ok = (root / name).exists()
        if not ok:
            problems.append(f"{where}: {kind} `{name}` does not exist")


def check_definition_of_done(root: Path, known: dict, problems: list[str]) -> int:
    lines = (root / "docs/definition-of-done.md").read_text().split("\n")
    points = 0
    for index, line in enumerate(lines):
        point = POINT.match(line)
        if not point:
            continue
        points += 1
        following = next((c for c in lines[index + 1 :] if c.strip()), "")
        checked = CHECKED.match(following)
        where = f"definition-of-done point {point[1]}"
        if not checked:
            problems.append(f'{where}: no "Checked by:" line')
        else:
            check_references(where, checked[1], root, known, problems)
    return points


def check_rules(root: Path, path: str, label: str, columns: int, known: dict, problems: list[str]) -> list[str]:
    """Checks each rule row of the page at `path`; returns the rule ids in order."""
    ids: list[str] = []
    for line in (root / path).read_text().split("\n"):
        row = RULE_ROW.match(line)
        if not row:
            continue
        ids.append(row[1])
        cells = [cell.strip() for cell in line.strip().strip("|").split(" | ")]
        if len(cells) < columns:
            problems.append(f'{label} {row[1]}: no "Checked by" column')
            continue
        if path == CARD_RULES:
            value = next((VALUE.search(cell) for cell in cells[1:-1] if VALUE.search(cell)), None)
            if value:
                problems.append(f'{label} {row[1]}: carries the value "{value[0]}"')
        check_references(f"{label} {row[1]}", cells[-1], root, known, problems)
    return ids


def check_rule_pages(root: Path, known: dict, problems: list[str]) -> int:
    playbook = card_rules = None
    if (root / PLAYBOOK).exists():
        playbook = check_rules(root, PLAYBOOK, "playbook", 4, known, problems)
    if (root / CARD_RULES).exists():
        card_rules = check_rules(root, CARD_RULES, "card-rules", 3, known, problems)
    if playbook is None and card_rules is None:
        problems.append(f"neither {PLAYBOOK} nor {CARD_RULES} exists")
        return 0
    if playbook is not None and card_rules is not None and playbook != card_rules:
        problems.append(
            f"{CARD_RULES} lists rules {', '.join(card_rules)} but {PLAYBOOK} lists {', '.join(playbook)}"
        )
    return len(playbook if playbook is not None else card_rules)


def check_links(root: Path, problems: list[str]) -> None:
    files = [root / "README.md", root / "AGENTS.md", *sorted((root / "docs").rglob("*.md"))]
    for path in files:
        if not path.exists():
            continue
        in_fence = False
        for number, line in enumerate(path.read_text().split("\n"), 1):
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
            if in_fence:
                continue
            for target in LINK.findall(line):
                if re.match(r"^[a-z]+:", target) or target.startswith("#"):
                    continue
                if not (path.parent / target.split("#", 1)[0]).exists():
                    rel = path.relative_to(root)
                    problems.append(f"{rel}:{number}: link `{target}` does not resolve")


def run(root: Path) -> list[str]:
    problems: list[str] = []
    known = {"ci": ci_jobs(root), "lint": lints(root), "deny": deny_sections(root)}
    points = check_definition_of_done(root, known, problems)
    rules = check_rule_pages(root, known, problems)
    if points == 0 or rules == 0:
        problems.append("found no points or no rules; has the format changed?")
    check_links(root, problems)
    return problems


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    problems = run(root)
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        return 1
    print("Every definition-of-done point and card rule names a check that exists; every doc link resolves.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
