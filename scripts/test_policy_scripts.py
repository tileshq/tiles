#!/usr/bin/env python3
"""Tests that each policy script fails on the case it exists to catch."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load(name: str):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), HERE / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


check_docs = load("check-docs")
network = load("check-network-owners")
release_binary = load("check-release-binary")
ci_main_gate = load("ci-main-gate")
no_network = load("no-network")


def repo(dod: str, playbook: str | None, readme: str = "# Tiles\n", card_rules: str | None = None) -> Path:
    root = Path(tempfile.mkdtemp())
    (root / ".github/workflows").mkdir(parents=True)
    (root / ".github/workflows/ci.yml").write_text("name: CI\non: push\njobs:\n  rust:\n    runs-on: x\n")
    (root / "Cargo.toml").write_text('[workspace.lints.rust]\nunsafe_code = "forbid"\n[workspace.lints.clippy]\npedantic = "warn"\n')
    (root / "deny.toml").write_text("[licenses]\nallow = []\n")
    (root / "docs").mkdir()
    (root / "docs/definition-of-done.md").write_text(textwrap.dedent(dod))
    if playbook is not None:
        (root / "docs/playbook.md").write_text(textwrap.dedent(playbook))
    if card_rules is not None:
        (root / "docs/card-rules.md").write_text(textwrap.dedent(card_rules))
    (root / "README.md").write_text(readme)
    (root / "present.txt").write_text("x")
    return root


GOOD_PLAYBOOK = """\
| # | Rule | Test | Checked by |
|---|---|---|---|
| C1 | Radius | 14 px | file `present.txt` |
"""

GOOD_CARD_RULES = """\
| # | Rule | Checked by |
|---|---|---|
| C1 | The card has round corners | file `present.txt` |
"""

DOD = "1. **Point.**\n   Checked by: maintainer review\n"


class CheckDocs(unittest.TestCase):
    def test_accepts_every_kind_of_real_reference(self):
        root = repo(
            """\
            1. **Point.**
               Checked by: ci `rust` · lint `unsafe_code` · lint `clippy::pedantic` · deny `licenses` · file `present.txt` · maintainer review
            """,
            GOOD_PLAYBOOK,
        )
        self.assertEqual(check_docs.run(root), [])

    def test_refuses_a_point_without_checked_by(self):
        root = repo("1. **Point.** Nothing checks it.\n2. **Next.**\n   Checked by: maintainer review\n", GOOD_PLAYBOOK)
        self.assertEqual(check_docs.run(root), ['definition-of-done point 1: no "Checked by:" line'])

    def test_refuses_references_to_missing_things(self):
        root = repo(
            """\
            1. **Point.**
               Checked by: ci `nope` · lint `clippy::nope` · deny `bans` · test `missing.rs` · vibes
            """,
            GOOD_PLAYBOOK,
        )
        self.assertEqual(
            check_docs.run(root),
            [
                "definition-of-done point 1: ci `nope` does not exist",
                "definition-of-done point 1: lint `clippy::nope` does not exist",
                "definition-of-done point 1: deny `bans` does not exist",
                "definition-of-done point 1: test `missing.rs` does not exist",
                'definition-of-done point 1: cannot read "vibes"',
            ],
        )

    def test_refuses_a_playbook_rule_without_a_real_check(self):
        playbook = "| # | Rule | Test | Checked by |\n|---|---|---|---|\n| C1 | Radius | 14 px | file `gone.txt` |\n"
        root = repo("1. **Point.**\n   Checked by: maintainer review\n", playbook)
        self.assertEqual(check_docs.run(root), ["playbook C1: file `gone.txt` does not exist"])

    def test_checks_the_card_rules_page_in_place_of_a_missing_playbook(self):
        root = repo(DOD, None, card_rules=GOOD_CARD_RULES)
        self.assertEqual(check_docs.run(root), [])
        gone = GOOD_CARD_RULES.replace("present.txt", "gone.txt")
        root = repo(DOD, None, card_rules=gone)
        self.assertEqual(check_docs.run(root), ["card-rules C1: file `gone.txt` does not exist"])

    def test_still_checks_the_playbook_while_it_is_present(self):
        playbook = GOOD_PLAYBOOK.replace("present.txt", "gone.txt")
        root = repo(DOD, playbook, card_rules=GOOD_CARD_RULES)
        self.assertEqual(check_docs.run(root), ["playbook C1: file `gone.txt` does not exist"])

    def test_refuses_a_tree_with_neither_rules_page(self):
        problems = check_docs.run(repo(DOD, None))
        self.assertIn("neither docs/playbook.md nor docs/card-rules.md exists", problems)

    def test_refuses_a_value_on_the_card_rules_page(self):
        for rule, value in [
            ("Corners of 14 px", "14 px"),
            ("The border is #e1e4e8", "#e1e4e8"),
            ("Fades over 300ms", "300ms"),
            ("Areas at 25% opacity", "25%"),
            ("Text stands at 4.5:1 on the card", "4.5:1"),
            ("Colours stand at 3:1 on the card", "3:1"),
            ("Areas at 0.25 opacity", "0.25"),
            ("Lines at .5 opacity", ".5"),
        ]:
            with self.subTest(value=value):
                page = GOOD_CARD_RULES.replace("The card has round corners", rule)
                root = repo(DOD, None, card_rules=page)
                self.assertEqual(check_docs.run(root), [f'card-rules C1: carries the value "{value}"'])

    def test_refuses_card_rules_that_drift_from_the_playbook(self):
        extra = GOOD_CARD_RULES + "| C2 | Content sits inside even padding | file `present.txt` |\n"
        root = repo(DOD, GOOD_PLAYBOOK, card_rules=extra)
        self.assertEqual(
            check_docs.run(root),
            ["docs/card-rules.md lists rules C1, C2 but docs/playbook.md lists C1"],
        )

    def test_refuses_a_broken_doc_link_outside_code_fences(self):
        readme = "See [here](docs/missing.md) and [web](https://x.example).\n```\n[ignored](nowhere.md)\n```\n"
        root = repo("1. **Point.**\n   Checked by: maintainer review\n", GOOD_PLAYBOOK, readme)
        self.assertEqual(check_docs.run(root), ["README.md:1: link `docs/missing.md` does not resolve"])


def metadata(edges: dict[str, list[tuple[str, str | None]]], members: list[str]) -> dict:
    names = set(edges) | {dep for deps in edges.values() for dep, _ in deps}
    return {
        "packages": [{"id": n, "name": n, "manifest_path": f"/nonexistent/{n}/Cargo.toml"} for n in names],
        "workspace_members": members,
        "resolve": {
            "nodes": [
                {"id": n, "deps": [{"pkg": d, "dep_kinds": [{"kind": k}]} for d, k in edges.get(n, [])]}
                for n in names
            ]
        },
    }


class NetworkOwners(unittest.TestCase):
    def test_refuses_a_runtime_path_to_a_network_crate(self):
        meta = metadata({"tiles-cli": [("helper", None)], "helper": [("ureq", None)]}, ["tiles-cli"])
        self.assertEqual(network.runtime_reach(meta), ["tiles-cli reaches ureq: tiles-cli -> helper -> ureq"])

    def test_ignores_build_and_dev_dependencies(self):
        meta = metadata({"tiles-engine": [("skia-bindings", None)], "skia-bindings": [("ureq", "build")]}, ["tiles-engine"])
        self.assertEqual(network.runtime_reach(meta), [])

    def test_allows_the_network_owners(self):
        meta = metadata(
            {"tiles-omni": [("tiles-net", None)], "tiles-warehouse": [("tiles-net", None)], "tiles-net": [("ureq", None)]},
            ["tiles-omni", "tiles-warehouse", "tiles-net"],
        )
        self.assertEqual(network.runtime_reach(meta), [])

    def test_allows_reaching_the_network_through_the_owner_only(self):
        meta = metadata(
            {"tiles-mcp": [("tiles-omni", None)], "tiles-omni": [("ureq", None)]},
            ["tiles-mcp", "tiles-omni"],
        )
        self.assertEqual(network.runtime_reach(meta), [])
        meta = metadata(
            {"tiles-mcp": [("tiles-omni", None), ("helper", None)], "tiles-omni": [("ureq", None)], "helper": [("ureq", None)]},
            ["tiles-mcp", "tiles-omni"],
        )
        self.assertEqual(network.runtime_reach(meta), ["tiles-mcp reaches ureq: tiles-mcp -> helper -> ureq"])

    def test_refuses_a_non_member_crate_named_like_the_owner(self):
        meta = metadata({"tiles-mcp": [("tiles-omni", None)], "tiles-omni": [("ureq", None)]}, ["tiles-mcp"])
        self.assertEqual(network.runtime_reach(meta), ["tiles-mcp reaches ureq: tiles-mcp -> tiles-omni -> ureq"])


def crates(sources: dict[str, str]) -> dict:
    root = Path(tempfile.mkdtemp())
    packages = []
    for name, lib in sources.items():
        (root / name / "src").mkdir(parents=True)
        (root / name / "Cargo.toml").write_text(f'[package]\nname = "{name}"\n')
        (root / name / "src/lib.rs").write_text(textwrap.dedent(lib))
        packages.append({"id": name, "name": name, "manifest_path": str(root / name / "Cargo.toml")})
    return {"packages": packages, "workspace_members": list(sources)}


class SocketSources(unittest.TestCase):
    def test_refuses_a_member_that_opens_a_socket(self):
        meta = crates({"tiles-cli": "pub fn f() {}\nuse std::net::TcpStream;\n"})
        problems = network.socket_sources(meta)
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].endswith("tiles-cli/src/lib.rs:2: tiles-cli opens sockets"), problems[0])

    def test_allows_sockets_in_the_network_owner(self):
        meta = crates({"tiles-omni": "use std::net::TcpStream;\n", "tiles-engine": "pub fn draw() {}\n"})
        self.assertEqual(network.socket_sources(meta), [])



class Telemetry(unittest.TestCase):
    def test_refuses_a_telemetry_crate_even_in_the_network_owner(self):
        meta = metadata({"tiles-omni": [("ureq", None), ("sentry", None)]}, ["tiles-omni"])
        self.assertEqual(network.telemetry_reach(meta), ["tiles-omni reaches telemetry crate sentry: tiles-omni -> sentry"])

    def test_refuses_one_reached_through_a_helper_and_ignores_dev_dependencies(self):
        meta = metadata(
            {"tiles-cli": [("helper", None), ("bench", "dev")], "helper": [("opentelemetry", None)], "bench": [("sentry", None)]},
            ["tiles-cli"],
        )
        self.assertEqual(
            network.telemetry_reach(meta),
            ["tiles-cli reaches telemetry crate opentelemetry: tiles-cli -> helper -> opentelemetry"],
        )


class UrlSources(unittest.TestCase):
    def test_refuses_a_host_that_is_not_omni(self):
        meta = crates({"tiles-mcp": 'const PING: &str = "https://stats.example.com/v1";\n'})
        problems = network.url_sources(meta)
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].endswith("tiles-mcp names stats.example.com, not a documented host"), problems[0])

    def test_allows_omni_hosts_placeholders_comments_and_tests(self):
        meta = crates({"tiles-omni": """\
            //! Docs may cite https://docs.example.com.
            const USAGE: &str = "tiles omni set-key <https://yourcompany.omniapp.co>";
            fn url(host: &str) -> String { format!("https://{host}/api") }
            fn scheme(raw: &str) -> bool { raw.starts_with("https://") }
            #[cfg(test)]
            mod helpers;
            fn after() { let _ = "https://acme.omniapp.co"; }
            #[cfg(test)]
            mod tests {
                const EVIL: &str = "https://evil.test";
            }
            """})
        self.assertEqual(network.url_sources(meta), [])

    def test_allows_exactly_the_two_bigquery_hosts(self):
        meta = crates({"tiles-warehouse": """\
            const TOKEN: &str = "https://oauth2.googleapis.com/token";
            const API: &str = "https://BigQuery.googleapis.com/bigquery/v2";
            """})
        self.assertEqual(network.url_sources(meta), [])

    def test_refuses_look_alikes_of_the_bigquery_hosts(self):
        for host in [
            "evil.googleapis.com",
            "bigquery.googleapis.com.evil.test",
            "oauth2.googleapis.com.evil.test",
            "x.bigquery.googleapis.com",
            "bigqueryxgoogleapis.com",
            "googleapis.com",
        ]:
            with self.subTest(host=host):
                meta = crates({"tiles-warehouse": f'const API: &str = "https://{host}/v2";\n'})
                problems = network.url_sources(meta)
                self.assertEqual(len(problems), 1, problems)
                self.assertTrue(problems[0].endswith(f"names {host}, not a documented host"), problems[0])

    def test_allows_the_two_github_hosts_in_the_update_code_only(self):
        meta = crates({"tiles-cli": "pub mod update;\n"})
        root = Path(meta["packages"][0]["manifest_path"]).parent
        (root / "src/update.rs").write_text(textwrap.dedent("""\
            fn url(repo: &str) -> String { format!("https://api.github.com/repos/{repo}/releases/latest") }
            fn page(repo: &str) -> String { format!("https://GitHub.com/{repo}/releases/latest") }
            """))
        self.assertEqual(network.url_sources(meta), [])
        for name, path in [("tiles-cli", "src/lib.rs"), ("tiles-cli", "src/run.rs"), ("tiles-mcp", "src/update.rs")]:
            with self.subTest(name=name, path=path):
                meta = crates({name: "pub fn f() {}\n"})
                root = Path(meta["packages"][0]["manifest_path"]).parent
                (root / path).write_text('const API: &str = "https://api.github.com/repos/o/r";\n')
                problems = network.url_sources(meta)
                self.assertEqual(len(problems), 1, problems)
                self.assertTrue(
                    problems[0].endswith(f"{name} names api.github.com, which only tiles-cli's src/update.rs may name"),
                    problems[0],
                )

    def test_refuses_look_alikes_of_the_github_hosts(self):
        for host in [
            "evil.github.com",
            "gist.github.com",
            "api.github.com.evil.test",
            "github.com.evil.test",
            "x.api.github.com",
            "apixgithub.com",
            "githubusercontent.com",
            "objects.githubusercontent.com",
            "api.github.co",
            "github.io",
        ]:
            with self.subTest(host=host):
                meta = crates({"tiles-cli": "pub fn f() {}\n"})
                root = Path(meta["packages"][0]["manifest_path"]).parent
                (root / "src/update.rs").write_text(f'const API: &str = "https://{host}/repos";\n')
                problems = network.url_sources(meta)
                self.assertEqual(len(problems), 1, problems)
                self.assertTrue(problems[0].endswith(f"names {host}, not a documented host"), problems[0])

    def test_code_after_a_cfg_test_mod_line_is_still_checked(self):
        meta = crates({"tiles-cli": '#[cfg(test)]\nmod tests;\nconst X: &str = "http://tracker.test";\n'})
        self.assertEqual(len(network.url_sources(meta)), 1)


def addons(files: dict[str, str]) -> Path:
    root = Path(tempfile.mkdtemp())
    for name, text in files.items():
        (root / "adapters" / name).parent.mkdir(parents=True, exist_ok=True)
        (root / "adapters" / name).write_text(textwrap.dedent(text))
    return root


class AddonSources(unittest.TestCase):
    def test_allows_reading_files_and_running_tiles(self):
        root = addons({"pi/index.ts": """\
            import { spawn } from "node:child_process";
            import { readFileSync } from "node:fs";
            const child = spawn(spec.tiles, ["card", "-"], { stdio: "pipe" });
            const m = /PNG: (.+)/.exec(text);
            """, "pi/index.test.ts": 'await fetch("https://x.test");\n'})
        self.assertEqual(network.addon_sources(root), [])

    def test_refuses_network_apis_and_urls(self):
        root = addons({
            "a/x.ts": 'await fetch(url);\n',
            "b/y.ts": 'import https from "node:https";\n',
            "c/z.tsx": 'new WebSocket("wss://x");\n',
            "d/SKILL.md": "Report to https://telemetry.test\n",
        })
        self.assertEqual(len(network.addon_sources(root)), 4)

    def test_refuses_running_anything_but_tiles(self):
        root = addons({"pi/index.ts": """\
            import { spawn, exec } from "child_process";
            spawn("curl", ["https"]);
            exec(`tiles card`);
            """})
        problems = network.addon_sources(root)
        self.assertEqual(len(problems), 2, problems)
        self.assertIn('runs "curl"', problems[0])

    def test_refuses_child_process_imported_where_its_calls_hide(self):
        for source in [
            'import * as cp from "node:child_process";\ncp.spawn("curl", ["x"]);\n',
            'import cp from "child_process";\ncp.execSync("curl x");\n',
            'const child_process = require("child_process");\nchild_process.execSync("curl x");\n',
            'import { spawn as run } from "node:child_process";\nrun("curl", ["x"]);\n',
            'const { execSync } = await import("node:child_process");\nexecSync("curl x");\n',
        ]:
            problems = network.addon_sources(addons({"pi/index.ts": source}))
            self.assertTrue(
                any("imports child_process other than by name" in p for p in problems),
                (source, problems),
            )

    def test_allows_a_named_import_across_lines(self):
        root = addons({"pi/index.ts": 'import {\n  spawn,\n  spawnSync,\n} from "node:child_process";\nspawn(spec.tiles, []);\n'})
        self.assertEqual(network.addon_sources(root), [])


def skia_bindings_version() -> str:
    lock = (HERE.parent / "Cargo.lock").read_text()
    return lock.split('name = "skia-bindings"\nversion = "', 1)[1].split('"', 1)[0]


class FetchSkia(unittest.TestCase):
    """scripts/fetch-skia.sh accepts only the archive whose SHA-256 is pinned."""

    def fetch(self, archive: bytes, pinned: bytes, target: str = "aarch64-apple-darwin"):
        import hashlib
        import subprocess

        base = Path(tempfile.mkdtemp())
        name = f"skia-binaries-0123abcd-{target}-pdf-textlayout.tar.gz"
        version = skia_bindings_version()
        (base / version).mkdir()
        (base / version / name).write_bytes(archive)
        sums = base / "sums"
        sums.write_text(f"{hashlib.sha256(pinned).hexdigest()}  {version}/{name}\n")
        out = base / "out"
        return subprocess.run(
            ["sh", str(HERE / "fetch-skia.sh"), target, str(out)],
            env={"PATH": "/usr/bin:/bin", "TILES_SKIA_BASE": f"file://{base}", "TILES_SKIA_SUMS": str(sums)},
            capture_output=True,
            text=True,
        ), out / name

    def test_accepts_the_pinned_archive(self):
        result, path = self.fetch(b"skia", b"skia")
        self.assertEqual(result.returncode, 0, result.stderr)
        url = result.stdout.strip()
        self.assertTrue(url.startswith("file:///"), url)
        self.assertEqual(Path(url.removeprefix("file://")).resolve(), path.resolve())

    def test_refuses_and_removes_a_tampered_archive(self):
        result, path = self.fetch(b"evil", b"skia")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not the pinned", result.stderr)
        self.assertFalse(path.exists())

    def test_refuses_a_target_with_no_pinned_archive(self):
        import subprocess

        result = subprocess.run(
            ["sh", str(HERE / "fetch-skia.sh"), "riscv64-none", tempfile.mkdtemp()],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("pins no single archive", result.stderr)

    def test_pins_one_archive_per_release_target_for_the_locked_version(self):
        version = skia_bindings_version()
        pins = []
        for line in (HERE / "skia-binaries.sha256").read_text().splitlines():
            digest, path = line.split("  ")
            pinned_version, file = path.split("/")
            self.assertRegex(digest, r"^[0-9a-f]{64}$", line)
            self.assertEqual(pinned_version, version, line)
            pins.append(file)
        for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"]:
            pattern = rf"skia-binaries-[0-9a-f]+-{re.escape(target)}-[a-z-]+\.tar\.gz"
            matching = [f for f in pins if re.fullmatch(pattern, f)]
            self.assertEqual(len(matching), 1, target)


class FetchGitleaks(unittest.TestCase):
    """scripts/fetch-gitleaks.sh refuses a gitleaks download that is not the pinned one."""

    def test_refuses_and_removes_a_tampered_download(self):
        import platform
        import subprocess

        pinned = {("Linux", "x86_64"): "linux_x64", ("Darwin", "x86_64"): "darwin_x64", ("Darwin", "arm64"): "darwin_arm64"}
        name = pinned.get((platform.system(), platform.machine()))
        if name is None:
            self.skipTest("no gitleaks pinned for this machine")
        base = Path(tempfile.mkdtemp())
        version = re.search(r"^version=(\S+)$", (HERE / "fetch-gitleaks.sh").read_text(), re.M)[1]
        file = f"gitleaks_{version}_{name}.tar.gz"
        (base / f"v{version}").mkdir()
        (base / f"v{version}" / file).write_bytes(b"evil")
        out = base / "out"
        result = subprocess.run(
            ["sh", str(HERE / "fetch-gitleaks.sh"), str(out)],
            env={"PATH": "/usr/bin:/bin", "TILES_GITLEAKS_BASE": f"file://{base}"},
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not the pinned", result.stderr)
        self.assertFalse((out / file).exists())
        self.assertFalse((out / "gitleaks").exists())


class FetchTla2tools(unittest.TestCase):
    """scripts/fetch-tla2tools.sh keeps only the jar with the pinned SHA-256."""

    def setUp(self):
        self.base = Path(tempfile.mkdtemp())
        self.script = (HERE / "fetch-tla2tools.sh").read_text()
        self.version = re.search(r"^version=(\S+)$", self.script, re.M)[1]
        (self.base / f"v{self.version}").mkdir()
        self.served = self.base / f"v{self.version}" / "tla2tools.jar"
        self.out = self.base / "out"

    def fetch(self, script: Path):
        import subprocess

        return subprocess.run(
            ["sh", str(script), str(self.out)],
            env={"PATH": "/usr/bin:/bin", "TILES_TLA2TOOLS_BASE": f"file://{self.base}"},
            capture_output=True,
            text=True,
        )

    def test_refuses_and_removes_a_tampered_download(self):
        self.served.write_bytes(b"evil")
        result = self.fetch(HERE / "fetch-tla2tools.sh")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not the pinned", result.stderr)
        self.assertEqual(list(self.out.iterdir()), [])

    def test_accepts_the_pinned_jar_and_prints_its_path(self):
        import hashlib

        jar = b"PK\x03\x04 a jar whose digest is pinned below"
        self.served.write_bytes(jar)
        pinned = Path(tempfile.mkdtemp()) / "fetch-tla2tools.sh"
        pinned.write_text(re.sub(r"^want=\S+$", f"want={hashlib.sha256(jar).hexdigest()}", self.script, count=1, flags=re.M))
        result = self.fetch(pinned)
        self.assertEqual(result.returncode, 0, result.stderr)
        printed = Path(result.stdout.strip())
        self.assertEqual(printed.resolve(), (self.out / f"tla2tools-{self.version}.jar").resolve())
        self.assertEqual(printed.read_bytes(), jar)


class FetchKaniBundle(unittest.TestCase):
    """scripts/fetch-kani-bundle.sh refuses a Kani bundle that is not the pinned one."""

    def test_refuses_and_removes_a_tampered_download(self):
        import subprocess

        base = Path(tempfile.mkdtemp())
        version = re.search(r"^version=(\S+)$", (HERE / "fetch-kani-bundle.sh").read_text(), re.M)[1]
        file = f"kani-{version}-x86_64-unknown-linux-gnu.tar.gz"
        (base / f"kani-{version}").mkdir()
        (base / f"kani-{version}" / file).write_bytes(b"evil")
        out = base / "out"
        result = subprocess.run(
            ["sh", str(HERE / "fetch-kani-bundle.sh"), str(out)],
            env={"PATH": "/usr/bin:/bin", "TILES_KANI_BUNDLE_BASE": f"file://{base}"},
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not the pinned", result.stderr)
        self.assertEqual(list(out.iterdir()), [])


class ReleaseBinary(unittest.TestCase):
    """scripts/check-release-binary.py finds every kind of leak in a binary."""

    ROOT = HERE.parent

    def scan(self, binary: bytes, forbid: list[str] | None = None) -> list[str]:
        return release_binary.scan(b"\x00\x7fELF" + binary + b"\x00", self.ROOT, forbid or [])

    def test_passes_a_binary_with_sealed_assets(self):
        self.assertEqual(self.scan(bytes(range(256)) * 64 + b'tiles/12:40:7 /deps/serde-1.0/src/de.rs "name": x'), [])

    def test_finds_home_cargo_and_crate_source_paths(self):
        for path in [
            b"/Users/runner/work/tiles/x.rs",
            b"/home/runner/.cargo/registry/src/serde/de.rs",
            b"crates/tiles-engine/src/layout.rs",
            b"/tiles/tiles-mcp/src/card.rs",
        ]:
            with self.subTest(path=path):
                self.assertTrue(any(p.startswith("path ") for p in self.scan(path)), path)

    def test_finds_a_path_passed_with_forbid(self):
        self.assertTrue(self.scan(b"/opt/build/tiles/out", ["/opt/build/tiles/"]))

    def test_finds_plain_tokens_and_forms(self):
        assets = self.ROOT / "crates/tiles-engine/assets"
        tokens = assets / "plain/tokens.json"
        for path in [tokens, assets / "forms/bar.json"]:
            with self.subTest(path=path.name):
                problems = self.scan(path.read_bytes())
                self.assertTrue(any("JSON key" in p for p in problems), problems)
        accent = json.loads(tokens.read_text())["themes"]["list"][0]["accent"]
        problems = self.scan(tokens.read_bytes())
        self.assertTrue(any(f"colour {accent}" in p for p in problems), problems)
        summary = json.loads((assets / "forms/bar.json").read_text())["summary"]
        problems = self.scan(summary.encode())
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("form text", problems[0])

    def test_finds_a_colour_in_either_case_and_a_json_key_with_spaces(self):
        tokens = json.loads((self.ROOT / "crates/tiles-engine/assets/plain/tokens.json").read_text())
        accent = tokens["themes"]["list"][0]["accent"]
        self.assertTrue(self.scan(accent.upper().encode()))
        # White, which x86 code spells bare, counts only in quotes.
        white = self.pack('{"themes": {"list": [{"name": "white", "card": "#ffffff"}]}}')
        self.assertTrue(release_binary.scan(b'\x00"#FFFFFF"\x00', self.ROOT, [], white))
        code = bytes.fromhex("31dbeb23666666666666662e0f1f8400")
        self.assertEqual(release_binary.scan(code, self.ROOT, [], white), [])
        self.assertTrue(self.scan(b'"title_line" : 19'))
        self.assertEqual(self.scan(b"title_line"), [])


    def pack(self, tokens: str) -> Path:
        """A design pack holding `tokens`, sealed as crates/tiles-engine/src/pack.rs seals one."""
        seed_a, seed_b = list(range(8)), list(range(8, 16))
        key = release_binary.derive(seed_a, seed_b)
        sealed = release_binary.apply(key, "tokens.json", tokens.encode())
        hexed = lambda words: b"".join(w.to_bytes(4, "little") for w in words).hex()
        head = f"tiles-design-pack 1\nnumber 9\nseed-a {hexed(seed_a)}\nseed-b {hexed(seed_b)}\n"
        head += f"asset tokens.json {len(sealed)}\nend\n"
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        (tmp / "tiles-design-9.pack").write_bytes(head.encode() + sealed)
        return tmp / "tiles-design-9.pack"

    def test_the_cipher_matches_the_rfc_vector(self):
        key = [int.from_bytes(bytes(range(i, i + 4)), "little") for i in range(0, 32, 4)]
        out = release_binary.block(key, 1, [0x09000000, 0x4A000000, 0])
        self.assertEqual(out[:4], [0xE4E7F110, 0x15593BD1, 0x1FDD0F50, 0xC47120A3])
        self.assertEqual(out[15], 0x4E3C50A2)

    def test_reads_the_needles_from_the_pack_the_build_took(self):
        pack = self.pack('{"themes": {"list": [{"name": "plain", "accent": "#a1b2c3"}]}}')
        self.assertEqual(release_binary.open_pack(pack.read_bytes())["tokens.json"][:11], '{"themes": ')
        problems = release_binary.scan(b"x #A1B2C3 x", self.ROOT, [], pack)
        self.assertTrue(any("colour #a1b2c3" in p for p in problems), problems)
        self.assertEqual(release_binary.scan(b"x #a1b2c3 x", self.ROOT, []), [])
        # The pack replaces the tree's tokens; the tree's forms still count.
        tokens = json.loads((self.ROOT / "crates/tiles-engine/assets/plain/tokens.json").read_text())
        accent = tokens["themes"]["list"][0]["accent"]
        self.assertTrue(release_binary.scan(accent.encode(), self.ROOT, []))
        self.assertEqual(release_binary.scan(accent.encode(), self.ROOT, [], pack), [])
        summary = json.loads((self.ROOT / "crates/tiles-engine/assets/forms/bar.json").read_text())["summary"]
        self.assertTrue(release_binary.scan(summary.encode(), self.ROOT, [], pack))

    def test_refuses_a_file_that_is_not_a_pack(self):
        with self.assertRaises(ValueError):
            release_binary.open_pack(b"tiles-design-pack 2\nend\n")


class ReleaseRustflags(unittest.TestCase):
    """scripts/release-rustflags.sh remaps every workspace source file."""

    def run_script(self, *args: str) -> str:
        import subprocess

        result = subprocess.run(["sh", str(HERE / "release-rustflags.sh"), *args], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_maps_every_source_file_after_the_broad_prefixes(self):
        flags = self.run_script().split()
        root = str(HERE.parent)
        self.assertTrue(all(f.startswith("--remap-path-prefix=") for f in flags), flags[:3])
        sources = sorted(str(p.relative_to(root)) for p in (HERE.parent / "crates").rglob("*.rs") if "target" not in p.parts)
        mapped = [f.split("=", 2)[1] for f in flags]
        for source in sources:
            self.assertIn(source, mapped)
        self.assertLess(mapped.index(root), mapped.index(sources[0]))
        self.assertEqual(len(self.run_script("--map").splitlines()), len(sources))


class SignMacos(unittest.TestCase):
    """scripts/sign-macos.sh checks each build's hash and signs all or nothing."""

    def sign(self, tamper: bool = False, **secrets: str):
        import hashlib
        import subprocess

        dist = Path(tempfile.mkdtemp())
        archive = dist / "tiles-v0.0.0-aarch64-apple-darwin.tar.gz"
        archive.write_bytes(b"tarball")
        digest = hashlib.sha256(b"tarball").hexdigest()
        (dist / f"{archive.name}.sha256").write_text(f"{digest}  {archive.name}\n")
        if tamper:
            archive.write_bytes(b"swapped")
        env = {"PATH": "/usr/bin:/bin", **secrets}
        result = subprocess.run(["sh", str(HERE / "sign-macos.sh"), str(dist)], env=env, capture_output=True, text=True)
        return result, archive

    def test_passes_unsigned_tarballs_through_without_secrets(self):
        result, archive = self.sign()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ships unsigned", result.stdout)
        self.assertEqual(archive.read_bytes(), b"tarball")

    def test_refuses_a_tarball_that_does_not_match_its_build_hash(self):
        result, _ = self.sign(tamper=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match its build job's hash", result.stderr)

    def test_refuses_some_but_not_all_secrets(self):
        result, _ = self.sign(APPLE_ID="a@example.com", APPLE_TEAM_ID="TEAM123456")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("2 of the 6 Apple signing secrets", result.stderr)


class CiMainGate(unittest.TestCase):
    """scripts/ci-main-gate.py skips main's heavy jobs only on a proven tree (ADR-055).

    The fixtures in fixtures/github/ci-main-gate/ are recorded GitHub API
    answers; the replay refuses any call they do not record.
    """

    FIXTURES = HERE.parent / "fixtures/github/ci-main-gate"

    def fixture(self, name: str) -> dict:
        case = json.loads((self.FIXTURES / name).read_text())
        if "base" in case:
            base = self.fixture(case["base"])
            base["responses"].update(case.get("responses", {}))
            case = {**base, **case, "responses": base["responses"]}
        case.setdefault("errors", {})
        return case

    def decide(self, case: dict) -> tuple[bool, str]:
        import urllib.error

        def api(path: str) -> object:
            if path in case["errors"]:
                error = case["errors"][path]
                raise urllib.error.HTTPError(path, error["status"], "Forbidden", {}, None)
            if path not in case["responses"]:
                raise LookupError(f"unrecorded call {path}")
            return json.loads(json.dumps(case["responses"][path]))

        push = case["push"]
        return ci_main_gate.decide(api, push["repo"], push["sha"], push["before"], push["run_id"])

    def test_each_recorded_case(self):
        for name in sorted(p.name for p in self.FIXTURES.glob("*.json")):
            with self.subTest(name):
                case = self.fixture(name)
                skip, line = self.decide(case)
                self.assertEqual(skip, case["skip"], line)
                self.assertIn(case["says"], line)
                self.assertTrue(line.startswith("Skipping the heavy jobs:" if skip else "Running every job:"), line)

    def edited(self, edit) -> tuple[bool, str]:
        case = self.fixture("skip-pr-53.json")
        responses = case["responses"]
        prefix = "/repos/saiemamer/tiles"
        edit(
            {
                "pulls": responses[f"{prefix}/commits/c818d1ac66bc669cc7f543bb1b4c77ae778d7c75/pulls"],
                "compare": responses[f"{prefix}/compare/67c93c3d154cec50c5230e34eec8d7859fa30504...be5c99033f3856e92e3148d32bf38a54d0e26e13"],
                "runs": responses[f"{prefix}/actions/workflows/388022651/runs?event=pull_request&head_sha=be5c99033f3856e92e3148d32bf38a54d0e26e13&per_page=100"],
                "jobs": responses[f"{prefix}/actions/runs/39727041782/jobs?filter=latest&per_page=100"],
            }
        )
        return self.decide(case)

    def test_runs_everything_for_a_commit_in_two_pull_requests(self):
        skip, line = self.edited(lambda r: r["pulls"].append(dict(r["pulls"][0], number=54)))
        self.assertFalse(skip)
        self.assertIn("belongs to 2 pull requests", line)

    def test_runs_everything_when_the_head_lacks_main_before_the_push(self):
        skip, line = self.edited(lambda r: r["compare"].update(status="diverged"))
        self.assertFalse(skip)
        self.assertIn("compare says diverged", line)

    def test_runs_everything_when_a_job_was_skipped_or_cancelled(self):
        for conclusion in ("skipped", "cancelled"):
            with self.subTest(conclusion):
                skip, line = self.edited(lambda r: r["jobs"]["jobs"][2].update(conclusion=conclusion))
                self.assertFalse(skip)
                self.assertIn(f"job rust (macos-15) of https://github.com/saiemamer/tiles/actions/runs/39727041782 is completed with conclusion {conclusion}", line)

    def test_ignores_the_gate_job_skipped_on_the_pull_request_run(self):
        def add_gate(r):
            r["jobs"]["jobs"].append({"name": "main-gate", "status": "completed", "conclusion": "skipped"})
            r["jobs"]["total_count"] += 1

        skip, line = self.edited(add_gate)
        self.assertTrue(skip, line)
        self.assertIn("passed all 9 of its jobs", line)

    def test_runs_everything_when_the_newest_run_for_the_head_failed(self):
        def add_failed_run(r):
            r["runs"]["workflow_runs"].append(dict(r["runs"]["workflow_runs"][0], id=39727041783, conclusion="failure"))

        skip, line = self.edited(add_failed_run)
        self.assertFalse(skip)
        self.assertIn("is completed with conclusion failure", line)

    def test_runs_everything_when_the_only_run_is_for_another_pull_request(self):
        def other(r):
            r["runs"]["workflow_runs"][0]["pull_requests"] = [{"number": 60, "base": {"ref": "main"}}]

        skip, line = self.edited(other)
        self.assertFalse(skip)
        self.assertIn("has no pull_request run for PR #53's head", line)

    def test_runs_everything_when_a_field_is_missing(self):
        skip, line = self.edited(lambda r: r["pulls"][0].pop("merged_at"))
        self.assertFalse(skip)
        self.assertIn("KeyError: 'merged_at'", line)

    def test_writes_skip_false_when_the_api_cannot_be_reached(self):
        import subprocess

        out = Path(tempfile.mkdtemp()) / "output"
        env = {
            "PATH": "/usr/bin:/bin",
            "GITHUB_API_URL": "http://127.0.0.1:9",
            "GITHUB_REPOSITORY": "saiemamer/tiles",
            "GITHUB_SHA": "c818d1ac66bc669cc7f543bb1b4c77ae778d7c75",
            "BEFORE": "67c93c3d154cec50c5230e34eec8d7859fa30504",
            "GITHUB_RUN_ID": "39728350286",
            "GITHUB_OUTPUT": str(out),
        }
        result = subprocess.run([sys.executable, str(HERE / "ci-main-gate.py")], env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Running every job: the GitHub API answer could not be read (URLError", result.stdout)
        self.assertEqual(out.read_text(), "skip=false\n")


def sandbox_ready() -> bool:
    """Whether scripts/no-network.py can run here: Linux with strace, unshare
    and unprivileged user namespaces."""
    import platform
    import subprocess

    if platform.system() != "Linux" or not shutil.which("strace") or not shutil.which("unshare"):
        return False
    return subprocess.run(["unshare", "--user", "--map-root-user", "--net", "true"], capture_output=True).returncode == 0


class NoNetwork(unittest.TestCase):
    """scripts/no-network.py fails a command that tries to reach the network (ADR-058).

    The trace tests run anywhere. The probe tests run a command that connects
    or looks up a name, swallows the error and exits 0, under the same
    wrapper CI's `no-network` job uses; they need Linux with strace and user
    namespaces and skip elsewhere, unless TILES_NO_NETWORK_REQUIRED=1 (set in
    that job) makes a missing sandbox a failure. With TILES_BIN naming a
    release-stamped tiles, they also show its daily update check is caught.
    """

    def findings(self, *lines: str) -> list[str]:
        return no_network.findings("\n".join(lines))

    def test_allows_netlink_unaddressed_sends_and_resumed_halves(self):
        self.assertEqual(
            self.findings(
                '4242 sendto(3, [{nlmsg_len=20, nlmsg_type=RTM_GETADDR, nlmsg_flags=NLM_F_REQUEST|NLM_F_DUMP, '
                'nlmsg_seq=1, nlmsg_pid=0}, {ifa_family=AF_UNSPEC}], 20, 0, {sa_family=AF_NETLINK, nl_pid=0, '
                "nl_groups=00000000}, 12) = 20",
                '4242 sendto(4, "\\1\\0", 2, MSG_NOSIGNAL, NULL, 0) = 2',
                '4242 sendmsg(7, {msg_name=NULL, msg_namelen=0, msg_iov=[{iov_base="x", iov_len=1}], msg_iovlen=1, '
                "msg_controllen=0, msg_flags=0}, 0) = 1",
                "4243 <... connect resumed>) = -1 ENETUNREACH (Network is unreachable)",
                "4243 +++ exited with 0 +++",
                "2647 io_uring_setup(256, {flags=IORING_SETUP_NO_SQARRAY, sq_thread_cpu=0, sq_thread_idle=0}) "
                "= -1 ENOSYS (Function not implemented) (INJECTED)",
            ),
            [],
        )

    def test_finds_every_inet_connect_loopback_and_dns_included(self):
        found = self.findings(
            '4242 connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("192.0.2.1")}, 16) '
            "= -1 ENETUNREACH (Network is unreachable)",
            '4242 connect(5, {sa_family=AF_INET6, sin6_port=htons(8080), sin6_flowinfo=htonl(0), '
            'inet_pton(AF_INET6, "::1", &sin6_addr), sin6_scope_id=0}, 28) = -1 ENETUNREACH (Network is unreachable)',
            '[pid  4250] connect(3, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("127.0.0.53")}, 16 '
            "<unfinished ...>",
        )
        self.assertEqual(len(found), 3, found)
        self.assertTrue(found[0].startswith("connect to AF_INET:"), found[0])
        self.assertTrue(found[1].startswith("connect to AF_INET6:"), found[1])
        self.assertIn("127.0.0.53", found[2])

    def test_finds_addressed_sends(self):
        found = self.findings(
            '4242 sendto(3, "q", 1, 0, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("8.8.8.8")}, 16) = 1',
            '4242 sendmmsg(3, [{msg_hdr={msg_name={sa_family=AF_INET6, sin6_port=htons(53), sin6_flowinfo=htonl(0), '
            'inet_pton(AF_INET6, "2001:db8::1", &sin6_addr), sin6_scope_id=0}, msg_namelen=28}, msg_len=30}], 1, 0) = 1',
            "4242 sendmsg(3, {msg_name=0x7ffd0000, msg_namelen=16, msg_iovlen=1}, 0) = 1",
            "4242 sendto(3, \"q\", 1, 0, 0x7ffd0000, 16) = -1 EFAULT (Bad address)",
        )
        self.assertEqual([f.split(":")[0] for f in found], [
            "sendto to AF_INET", "sendmmsg to AF_INET6",
            "sendmsg to an address strace could not read", "sendto to an address strace could not read",
        ])

    def test_finds_unix_sockets_that_reach_the_network_and_an_open_ring(self):
        found = self.findings(
            '4242 connect(3, {sa_family=AF_UNIX, sun_path="/run/systemd/resolve/io.systemd.Resolve"}, 42) = 0',
            '4242 connect(4, {sa_family=AF_UNIX, sun_path="/var/run/docker.sock"}, 23) = 0',
            '4242 connect(5, {sa_family=AF_UNIX, sun_path=@"/tmp/.X11-unix/X0"}, 20) = -1 ECONNREFUSED',
            "4242 connect(6, 0x7ffd0000, 16) = -1 EFAULT (Bad address)",
            "4242 io_uring_setup(8, {flags=0, sq_thread_cpu=0, sq_thread_idle=0}) = 9",
            "4243 io_uring_setup(8, {flags=0, sq_thread_cpu=0, sq_thread_idle=0} <unfinished ...>",
            "4243 <... io_uring_setup resumed>) = 10",
        )
        self.assertEqual(len(found), 6, found)
        self.assertIn("io.systemd.Resolve", found[0])
        self.assertIn("docker.sock", found[1])
        self.assertIn('@"/tmp/.X11-unix/X0"', found[2])
        self.assertIn("could not read", found[3])
        self.assertTrue(found[4].startswith("io_uring_setup opened a ring"), found[4])
        self.assertIn("resumed", found[5])

    def wrapped(self, *command: str, env: dict[str, str] | None = None):
        import os
        import subprocess

        if not sandbox_ready():
            if os.environ.get("TILES_NO_NETWORK_REQUIRED") == "1":
                self.fail("TILES_NO_NETWORK_REQUIRED=1, but this machine cannot run scripts/no-network.py")
            self.skipTest("needs Linux with strace, unshare and user namespaces")
        trace = Path(tempfile.mkdtemp()) / "trace"
        return subprocess.run(
            [sys.executable, str(HERE / "no-network.py"), str(trace), *command],
            env=env, capture_output=True, text=True, timeout=120,
        )

    def test_a_clean_command_passes_with_its_own_status(self):
        result = self.wrapped(sys.executable, "-c", "print('drawn'); raise SystemExit(3)")
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertEqual(result.stdout, "drawn\n")
        self.assertNotIn("no-network:", result.stderr)

    def test_fails_a_probe_that_connects_out_and_exits_0(self):
        probe = (
            "import socket\n"
            "try:\n    socket.create_connection(('192.0.2.1', 443), timeout=2)\n"
            "except OSError:\n    pass\n"
        )
        result = self.wrapped(sys.executable, "-c", probe)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('no-network: connect to AF_INET: ', result.stderr)
        self.assertIn('inet_addr("192.0.2.1")', result.stderr)

    def test_fails_a_node_probe_that_connects_out_and_exits_0(self):
        if not shutil.which("node"):
            self.skipTest("no node on PATH")
        probe = "require('node:net').connect(443, '192.0.2.1').on('error', () => {})"
        result = self.wrapped("node", "-e", probe)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('inet_addr("192.0.2.1")', result.stderr)

    def test_node_runs_with_its_io_uring_refused(self):
        if not shutil.which("node"):
            self.skipTest("no node on PATH")
        result = self.wrapped("node", "-e", "setTimeout(() => console.log('ran'), 10)")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "ran\n")
        self.assertNotIn("no-network:", result.stderr)

    def test_fails_a_probe_that_looks_up_a_name(self):
        probe = (
            "import socket\n"
            "try:\n    socket.getaddrinfo('tiles-no-network.invalid', 443)\n"
            "except OSError:\n    pass\n"
        )
        result = self.wrapped(sys.executable, "-c", probe)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("no-network: connect to ", result.stderr)

    def test_catches_the_daily_update_check_and_passes_with_it_off(self):
        import os

        tiles = os.environ.get("TILES_BIN")
        if not tiles:
            if os.environ.get("TILES_NO_NETWORK_REQUIRED") == "1":
                self.fail("TILES_NO_NETWORK_REQUIRED=1 needs TILES_BIN, a tiles built with TILES_VERSION set")
            self.skipTest("TILES_BIN names no tiles")
        home = Path(tempfile.mkdtemp())
        env = {"PATH": "/usr/bin:/bin", "HOME": str(home), "TILES_STATE_DIR": str(home / "state")}
        due = self.wrapped(tiles, "doctor", env=env)
        self.assertEqual(due.returncode, 1, due.stdout + due.stderr)
        self.assertIn("no-network: connect to ", due.stderr)
        self.assertTrue((home / "state/update-check.json").exists(), "the update check did not run")
        off = self.wrapped(tiles, "doctor", env={**env, "TILES_STATE_DIR": str(home / "off"), "TILES_UPDATE_CHECK": "off"})
        self.assertEqual(off.returncode, 0, off.stdout + off.stderr)
        self.assertNotIn("no-network:", off.stderr)


if __name__ == "__main__":
    unittest.main()
