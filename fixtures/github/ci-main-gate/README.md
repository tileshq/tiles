# CI main-gate fixtures

GitHub API answers for `scripts/ci-main-gate.py` (ADR-055), keyed by request path, trimmed to the fields the script reads. `CiMainGate` in `scripts/test_policy_scripts.py` replays them and refuses any call a file does not record.

| File | Case | Source |
|---|---|---|
| `skip-pr-53.json` | The squash commit of #53 has its head's tree and the head's CI run passed every job, so the heavy jobs skip | Recorded from this repository on 2026-10-22 |
| `tree-mismatch-pr-43.json` | The squash commit of #43 has a different tree from its head, so everything runs | Recorded from this repository on 2026-10-22 |
| `cancelled-pr-run.json` | `skip-pr-53.json` with the head's CI run cancelled, as `cancel-in-progress` leaves it, so everything runs | Edited from the #53 recording |
| `api-failure.json` | `skip-pr-53.json` with the runs listing answering 403, as GitHub does to a token without `actions: read`, so everything runs | Edited from the #53 recording |

A file with `base` starts from that file and replaces the responses it lists; `errors` maps a path to the HTTP status it answers with. `skip` and `says` are the decision and a piece of the log line the test expects.
