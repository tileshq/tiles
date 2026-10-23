#!/usr/bin/env python3
"""Decide whether a push to main may skip CI's heavy jobs (ADR-055).

A squash merge puts on main a tree its pull request's CI has already tested.
The push to main skips the heavy jobs only when the GitHub API shows all of:

  1. The pushed commit is the merge commit of exactly one merged pull request
     into main.
  2. The pushed commit's tree is the tree of that pull request's head commit.
  3. The head commit contains main as it was before this push, so the merge
     ref the pull request's CI checked out had that same tree.
  4. The newest run of this workflow for that head, on the pull_request
     event, completed with success, and every one of its jobs but this gate
     succeeded (none cancelled or skipped).

Anything else, an API error or a missing field included, means run every
job. The script always exits 0 and writes `skip=true` or `skip=false` to
GITHUB_OUTPUT; a crash leaves no output, which the workflow reads as run
every job.

Reads GITHUB_API_URL, GITHUB_REPOSITORY, GITHUB_SHA, GITHUB_RUN_ID and
GITHUB_TOKEN, and BEFORE, the commit main pointed at before the push.
"""

from __future__ import annotations

import json
import os
import re
import sys
import urllib.request
from typing import Callable

# This gate's own job in ci.yml, skipped on every pull_request run.
GATE_JOB = "main-gate"

SHA = re.compile(r"[0-9a-f]{40}")

Api = Callable[[str], object]


class NoSkip(Exception):
    """A condition for skipping does not hold."""


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise NoSkip(reason)


def decide(api: Api, repo: str, sha: str, before: str, run_id: str) -> tuple[bool, str]:
    """Return (skip, the log line saying why)."""
    try:
        return True, proven_skip(api, repo, sha, before, run_id)
    except NoSkip as why:
        return False, f"Running every job: {why}."
    except Exception as error:
        return False, f"Running every job: the GitHub API answer could not be read ({type(error).__name__}: {error})."


def proven_skip(api: Api, repo: str, sha: str, before: str, run_id: str) -> str:
    require(SHA.fullmatch(sha) is not None, f"the pushed commit {sha!r} is not a commit SHA")
    require(
        SHA.fullmatch(before) is not None and before != "0" * 40,
        f"main before this push, {before!r}, is not a commit SHA",
    )

    pulls = api(f"/repos/{repo}/commits/{sha}/pulls")
    require(isinstance(pulls, list), f"the pull requests of {sha} did not come as a list")
    require(len(pulls) == 1, f"{sha} belongs to {len(pulls)} pull requests, not one")
    pr = pulls[0]
    number = pr["number"]
    require(pr["merged_at"] is not None, f"PR #{number} is not merged")
    require(pr["merge_commit_sha"] == sha, f"{sha} is not PR #{number}'s merge commit")
    require(pr["base"]["ref"] == "main", f"PR #{number} merged into {pr['base']['ref']}, not main")
    head = pr["head"]["sha"]
    require(SHA.fullmatch(head) is not None, f"PR #{number}'s head {head!r} is not a commit SHA")

    tree = api(f"/repos/{repo}/git/commits/{sha}")["tree"]["sha"]
    head_tree = api(f"/repos/{repo}/git/commits/{head}")["tree"]["sha"]
    require(
        tree == head_tree,
        f"{sha} has tree {tree}, but PR #{number}'s head {head} has tree {head_tree}",
    )

    status = api(f"/repos/{repo}/compare/{before}...{head}")["status"]
    require(
        status in ("ahead", "identical"),
        f"PR #{number}'s head {head} does not contain main before this push, {before} (compare says {status})",
    )

    workflow = api(f"/repos/{repo}/actions/runs/{run_id}")["workflow_id"]
    listed = api(f"/repos/{repo}/actions/workflows/{workflow}/runs?event=pull_request&head_sha={head}&per_page=100")
    runs = [
        run
        for run in listed["workflow_runs"]
        if run["head_sha"] == head
        and run["event"] == "pull_request"
        and run["head_branch"] == pr["head"]["ref"]
        and all(p["number"] == number and p["base"]["ref"] == "main" for p in run["pull_requests"])
    ]
    require(bool(runs), f"this workflow has no pull_request run for PR #{number}'s head {head}")
    run = max(runs, key=lambda r: r["id"])
    require(
        run["status"] == "completed" and run["conclusion"] == "success",
        f"the newest run for PR #{number}'s head {head}, {run['html_url']}, is {run['status']} with conclusion {run['conclusion']}",
    )

    listed = api(f"/repos/{repo}/actions/runs/{run['id']}/jobs?filter=latest&per_page=100")
    jobs = [job for job in listed["jobs"] if job["name"] != GATE_JOB]
    require(listed["total_count"] == len(listed["jobs"]), f"run {run['html_url']} has more jobs than one page lists")
    require(bool(jobs), f"run {run['html_url']} lists no jobs")
    for job in jobs:
        require(
            job["status"] == "completed" and job["conclusion"] == "success",
            f"job {job['name']} of {run['html_url']} is {job['status']} with conclusion {job['conclusion']}",
        )

    return (
        f"Skipping the heavy jobs: {sha} has tree {tree}, the tree of PR #{number}'s head {head}, "
        f"which contains main before this push ({before}), and CI run {run['html_url']} "
        f"passed all {len(jobs)} of its jobs on that head."
    )


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
    skip, line = decide(
        github(env.get("GITHUB_API_URL", "https://api.github.com"), env.get("GITHUB_TOKEN", "")),
        env.get("GITHUB_REPOSITORY", ""),
        env.get("GITHUB_SHA", ""),
        env.get("BEFORE", ""),
        env.get("GITHUB_RUN_ID", ""),
    )
    print(line)
    if "GITHUB_OUTPUT" in env:
        with open(env["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write(f"skip={'true' if skip else 'false'}\n")
    if "GITHUB_STEP_SUMMARY" in env:
        with open(env["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(line + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
