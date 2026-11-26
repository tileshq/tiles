#!/usr/bin/env python3
"""Decide which of CI's jobs a run may skip (ADR-063).

Writes three outputs to GITHUB_OUTPUT, each `true` or `false`:

  outside     A pull request from a fork whose author has no write access and
              which lacks the full-ci label. Only policy and secrets run, and
              the `full suite` check fails until a maintainer adds the label.
  skip-build  rust, release-binary, no-network, fuzz and formal skip: every
              changed file is docs text that no build, test, fuzz target,
              proof or release check reads, or the pull request is outside.
  skip-deep   fuzz and formal skip: every changed file is docs text or under
              adapters/, which neither the fuzz targets nor the proofs read,
              or the pull request is outside.

A job skips only on the literal `true`. Any doubt writes `false`: an event
other than a pull request or a push, a diff git cannot produce, an empty diff.
A crash leaves no output, which the workflow reads as run every job.

Reads EVENT (github.event_name), REPO, HEAD_REPO, AUTHOR_ASSOCIATION, LABELS
(a JSON list of label names), BEFORE (the commit a push replaced) and the
checkout in the working directory: the merge commit a pull request's CI
tests, or the pushed commit.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

# Text no CI job but policy reads. docs/install.md is not one: the rust job's
# scripts/test_install.sh checks that it prints install.sh's key and SHA-256.
DOCS_TEXT_FILES = frozenset(
    {"README.md", "AGENTS.md", "CLAUDE.md", "CONTRIBUTING.md", "SECURITY.md", "TRADEMARKS.md"}
)
DOCS_TEXT_DIR = "docs/"
READ_BY_A_BUILD_JOB = frozenset({"docs/install.md"})

# The add-ons: tiles-cli embeds them (crates/tiles-cli/src/addons.rs), so the
# rust, release-binary and no-network jobs still run; no fuzz target or proof
# reads them.
ADAPTERS_DIR = "adapters/"

WRITE_ACCESS = frozenset({"OWNER", "MEMBER", "COLLABORATOR"})
FULL_CI_LABEL = "full-ci"
NULL_SHA = "0" * 40


def is_docs_text(path: str) -> bool:
    if path in READ_BY_A_BUILD_JOB:
        return False
    return path in DOCS_TEXT_FILES or path.startswith(DOCS_TEXT_DIR)


def is_adapter(path: str) -> bool:
    return path.startswith(ADAPTERS_DIR)


def is_outside(event: str, repo: str, head_repo: str, association: str, labels: list[str]) -> bool:
    """A pull request that waits for the full-ci label before its full suite."""
    if event != "pull_request":
        return False
    if head_repo == repo or association in WRITE_ACCESS:
        return False
    return FULL_CI_LABEL not in labels


def plan(outside: bool, changed: list[str] | None) -> tuple[dict[str, bool], str]:
    """Return the outputs and the log line saying why.

    `changed` is None when the run cannot list its changed files.
    """
    if outside:
        return (
            {"outside": True, "skip-build": True, "skip-deep": True},
            "An outside pull request without the full-ci label: only policy and secrets run, "
            "and `full suite` fails until a maintainer adds the label.",
        )
    if changed is None:
        return _everything("this run has no diff to read")
    if not changed:
        return _everything("the diff lists no file")
    code = [path for path in changed if not is_docs_text(path)]
    if not code:
        return (
            {"outside": False, "skip-build": True, "skip-deep": True},
            f"All {len(changed)} changed files are docs text: rust, release-binary, no-network, fuzz and formal skip.",
        )
    deep = [path for path in code if not is_adapter(path)]
    if not deep:
        return (
            {"outside": False, "skip-build": False, "skip-deep": True},
            f"All {len(changed)} changed files are docs text or add-ons: fuzz and formal skip.",
        )
    return _everything(f"{deep[0]} is code or a file a check reads, so every job runs")


def _everything(reason: str) -> tuple[dict[str, bool], str]:
    return {"outside": False, "skip-build": False, "skip-deep": False}, f"Running every job: {reason}."


def git(*args: str) -> str:
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True, timeout=60).stdout


def changed_files(event: str, before: str) -> list[str] | None:
    """The paths a pull request's merge or a push changes, or None."""
    try:
        if event == "pull_request":
            # A pull request's CI checks out GitHub's merge commit: its first
            # parent is main, so this is what the pull request adds to main.
            base = "HEAD^1"
        elif event == "push" and len(before) == 40 and before != NULL_SHA:
            git("fetch", "--no-tags", "--depth=1", "origin", before)
            base = before
        else:
            return None
        listed = git("diff", "--no-renames", "--name-only", "-z", base, "HEAD")
    except (subprocess.SubprocessError, OSError):
        return None
    return [path for path in listed.split("\0") if path]


def main() -> int:
    env = os.environ
    event = env.get("EVENT", "")
    try:
        labels = [str(label) for label in json.loads(env.get("LABELS") or "[]")]
    except (ValueError, TypeError):
        labels = []
    outside = is_outside(event, env.get("REPO", ""), env.get("HEAD_REPO", ""), env.get("AUTHOR_ASSOCIATION", ""), labels)
    outputs, line = plan(outside, None if outside else changed_files(event, env.get("BEFORE", "")))
    print(line)
    if "GITHUB_OUTPUT" in env:
        with open(env["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            for name, value in outputs.items():
                out.write(f"{name}={'true' if value else 'false'}\n")
    if "GITHUB_STEP_SUMMARY" in env:
        with open(env["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(line + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
