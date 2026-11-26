#!/usr/bin/env python3
"""Warn when a CI job runs past its time budget (ADR-064).

Reads this run attempt's jobs from the GitHub API, writes a table of their
run times to the job summary, slowest first, and prints a GitHub warning for
each job that took longer than BUDGET_SECONDS. It never fails the run: a job
over budget is a prompt to split or speed it up, not a broken merge gate. It
always exits 0, an unreadable API answer included.

Reads GITHUB_API_URL, GITHUB_REPOSITORY, GITHUB_RUN_ID, GITHUB_RUN_ATTEMPT,
GITHUB_TOKEN and BUDGET_SECONDS.
"""

from __future__ import annotations

import json
import os
import sys
import urllib.request
from datetime import datetime
from typing import Callable

Api = Callable[[str], object]


def seconds(job: dict) -> int | None:
    """How long a finished job ran, from its start to its end, or None."""
    if job.get("status") != "completed" or not job.get("started_at") or not job.get("completed_at"):
        return None
    start = datetime.fromisoformat(job["started_at"].replace("Z", "+00:00"))
    end = datetime.fromisoformat(job["completed_at"].replace("Z", "+00:00"))
    return max(0, int((end - start).total_seconds()))


def report(jobs: list[dict], budget: int) -> tuple[list[str], list[str]]:
    """Return (the summary's lines, one warning per job over budget)."""
    timed = []
    for job in jobs:
        took = seconds(job)
        if took is not None:
            timed.append((job["name"], took, job.get("conclusion")))
    timed.sort(key=lambda row: (-row[1], row[0]))
    over = [f"{name} took {took} s, over the {budget} s budget" for name, took, _ in timed if took > budget]
    lines = [
        f"CI job times, budget {budget} s each; {len(over)} over.",
        "",
        "| Job | Seconds | Conclusion |",
        "|---|---:|---|",
        *(f"| {name}{' (over)' if took > budget else ''} | {took} | {conclusion} |" for name, took, conclusion in timed),
    ]
    return lines, over


def jobs_of(api: Api, repo: str, run_id: str, attempt: str) -> list[dict]:
    jobs: list[dict] = []
    page = 1
    while True:
        listed = api(f"/repos/{repo}/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100&page={page}")
        jobs.extend(listed["jobs"])
        if len(jobs) >= listed["total_count"] or not listed["jobs"]:
            return jobs
        page += 1


def github(base: str, token: str) -> Api:
    def get(path: str) -> object:
        request = urllib.request.Request(
            base + path,
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {token}",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.load(response)

    return get


def main() -> int:
    env = os.environ
    budget = int(env.get("BUDGET_SECONDS", "180"))
    try:
        jobs = jobs_of(
            github(env.get("GITHUB_API_URL", "https://api.github.com"), env.get("GITHUB_TOKEN", "")),
            env.get("GITHUB_REPOSITORY", ""),
            env.get("GITHUB_RUN_ID", ""),
            env.get("GITHUB_RUN_ATTEMPT", "1"),
        )
    except Exception as error:
        print(f"::warning title=CI budget::The job times could not be read ({type(error).__name__}: {error}).")
        return 0
    lines, over = report(jobs, budget)
    print("\n".join(lines))
    for warning in over:
        print(f"::warning title=CI budget::{warning}")
    if "GITHUB_STEP_SUMMARY" in env:
        with open(env["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write("\n".join(lines) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
