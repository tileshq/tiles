# Security

## Reporting a vulnerability

Report a vulnerability privately. Do not open an issue or a pull request, and do not post the details anywhere else until a fix ships.

1. **GitHub private vulnerability reporting.** Choose "Report a vulnerability" on the Security tab of the public `tiles` repository. Use it first, so the report, the discussion and the fix stay private in one place.
2. **Email.** Write to Saiem Amer at saiem.amer07@gmail.com with "Tiles security" in the subject, if you cannot use GitHub's form.

Say what you found, the Tiles version (`tiles --version`), your terminal and harness, and the steps or input that show it. You get a reply confirming the report, and the fix is worked on privately and released before the details are made public.

## Supported versions

Tiles is pre-1.0. Only the latest release on the public `tiles` repository's Releases page gets security fixes; update to it before reporting. Older releases get none.

## What Tiles sends and keeps

- **No telemetry.** Tiles sends no usage data, crash report, analytics or licence check. The `tiles` binary opens two kinds of connection: HTTPS to the Omni host you configure, carrying your own query; and an update check, one anonymous `GET` of `https://api.github.com/repos/tileshq/tiles/releases/latest` (or of the repository `TILES_RELEASES_REPO` names) with only `User-Agent: tiles/<version>` and `Accept: application/json`. `tiles run` and `tiles doctor` send it at most once a day, and `TILES_UPDATE_CHECK=off` turns that off; `tiles update --check` sends it when you run it without `gh` signed in ([ADR-032](docs/adr/ADR-032-update-command-and-harness-versions.md)). Tiles never updates itself: only `tiles update` replaces the binary, through `gh` or files you downloaded, after checking them against the release's `SHA256SUMS`. The harness add-ons open no connection and run only the `tiles` binary.
- **No personal data.** Tiles collects nothing about you. Chart cards and the calls that drew them stay in your own cache directory (`~/Library/Caches/tiles` on macOS), readable only by you. The cache keeps the newest 300 cards and removes older ones as new cards arrive; `tiles cache clear` removes every card, call and entrance frame at once and prints how many cards went. Rows fetched from Omni never reach the disk: the session that fetched them keeps them in memory ([ADR-027](docs/adr/ADR-027-omni-rows-stay-in-memory.md)). Call files written by a Tiles release before that record keep their rows until their card is pruned, so run `tiles cache clear` once after upgrading. The Omni key stays in your Keychain or your environment.
- **What reaches the model.** Your harness sends a chart result to its model provider. That result is text only: the chart's title, form, id and saved file's path, and its values unless the rows came from Omni. Omni values reach the model only when you set `TILES_OMNI_VALUES_TO_MODEL=1` ([docs/omni.md](docs/omni.md)). With Omni configured, every Tiles tool asks before it runs unless you start the harness with `tiles run --allow-omni`.
- **What a model can read on this machine.** Tiles controls what its tool result says, not what else the model reads. Your harness's tool approval is the control: a model with an unrestricted shell or file reader can read any file your user can, the card pictures in the cache included, and those pictures show the values. `tiles run claude` and `tiles run opencode` deny the model's file and shell tools the cache directory; Codex and sessions started without `tiles run` get no such rule. Pi has no tool approvals: its Tiles extension asks before a chart that fetches from Omni, but nothing stops a Pi model's shell from reading the cache ([docs/omni.md](docs/omni.md#what-a-model-with-a-shell-can-read), [ADR-028](docs/adr/ADR-028-keep-the-model-out-of-the-cache.md)).

CI checks the network rule on every pull request with `scripts/check-network-owners.py`. It reads cargo's resolved dependency graph to show that only the data source crates (`tiles-net`, the HTTP client, and `tiles-omni`) link a networking crate and that no part of Tiles links a telemetry, crash-reporting or analytics crate; its scans of the source and add-ons for URLs and network calls are a lint on top. [ADR-024](docs/adr/ADR-024-security-baseline-in-ci.md) records it with the rest of the security baseline: dependency audits, fuzzing, property tests and secret scanning.
