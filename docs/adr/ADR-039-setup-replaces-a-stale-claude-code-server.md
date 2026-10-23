# ADR-039: `tiles setup` replaces a user-scope Claude Code `tiles` server for another `tiles` after its one question, and leaves every other one

- Status: Accepted; supersedes [ADR-035](ADR-035-macos-package-and-tiles-setup.md)'s rule that `tiles setup` never removes Claude Code's `tiles` server; ADR-035's one question, `--yes`, `--dry-run` and `tiles doctor`'s read of `~/.claude.json` stand
- Date: 2026-10-20

## Context

[ADR-035](ADR-035-macos-package-and-tiles-setup.md) left a Claude Code `tiles` server that runs another `tiles` for the user: `tiles setup` and `tiles doctor` named it and printed `run claude mcp remove tiles --scope user, then tiles setup`, since `claude mcp add` refuses a name that exists, and left removing it "to a later change". The common case is a package install beside an earlier `install.sh` one: Claude Code still runs `~/.local/bin/tiles` while every other harness `tiles setup` sets up runs `/usr/local/bin/tiles`, and the user has two more commands to type.

Claude Code 2.1.304's `claude mcp get tiles` prints the server's scope as its first line, `Scope: User config (available in all your projects)`, `Project config (shared via .mcp.json)` or `Local config (private to you in this project)`, and `claude mcp remove <name> --scope <scope>` removes it from that scope alone (both read from the installed CLI's `--help` and its labels).

## Decision

- **A user-scope server for another `tiles` is replaced.** When `claude mcp get tiles` prints `Scope: User config`, a `Command:` that is not this `tiles`, and no variable besides `TILES_DRAW`, the plan for Claude Code is two steps, `claude mcp remove tiles --scope user` then the `claude mcp add --scope user tiles -e TILES_DRAW=text -- <this tiles> mcp` that `tiles install claude` runs. Its found line reads `claude: <path>, set up with another tiles, <command>; Tiles will replace it`, and both steps are listed under `Tiles will change:` before the one question, so a no, no answer or no terminal changes nothing, `--dry-run` prints them, and `--yes` runs them.
- **A failed remove adds nothing.** The remove runs alone first; when it fails, setup stops for Claude Code and prints `tiles: claude not set up: <error>; Claude Code keeps its old tiles server: run claude mcp remove tiles --scope user, then tiles setup`, so Claude Code never ends up with a second server. When the remove succeeds and the add fails, the error ends `the old tiles server is removed: run tiles setup again`.
- **Every other server is left, with the commands that replace it.** A server in a project's or a folder's config is never touched: `claude: <path>, set up with another tiles, <command>, in Claude Code's project config; Tiles leaves it: run claude mcp remove tiles --scope project, then tiles setup` (or `local`). A user-scope server for another `tiles` that sets a variable of the user's, which `claude mcp add` would not write back, reads `..., with variables of yours; run claude mcp remove tiles --scope user, then tiles setup`. A server whose scope is not printed, one that runs no command, and one for this `tiles` without `mcp` and `TILES_DRAW=text` keep ADR-035's line. A server for this `tiles` with `mcp` and the variable stays `already set up`.
- **`tiles doctor` changes nothing.** It still reads only `mcpServers.tiles` from `~/.claude.json`, which is the user scope; an entry for another `tiles` now reads `  setup: Claude Code is set up with another tiles, <command>; run tiles setup to replace it`.

`crates/tiles-cli/src/setup/tests.rs` covers each plan and both failures against a faked `claude`; `crates/tiles-cli/tests/setup.rs` drives the binary against the stub `claude`: `n` runs neither command, `y` removes the old server and adds this one and a second run finds it set up, a project-scope server is named and neither removed nor added, and a remove that fails leaves the old server and runs no `mcp add`.

## Consequences

- An earlier `install.sh` copy stops reaching Claude Code after one `y`; `~/.local/bin/tiles` itself stays until the user moves it, and when it comes first on `PATH` `tiles setup` offers to move it aside ([ADR-035](ADR-035-macos-package-and-tiles-setup.md), 2026-10-21).
- `claude mcp get tiles` prints one server, read in the folder `tiles setup` runs from. Run in a project whose own config has a `tiles` server, it can print that one, which setup names and leaves, so the user-scope server is replaced only from a folder without one.
- Related: [ADR-011](ADR-011-surfaces-v0-1.md), [ADR-035](ADR-035-macos-package-and-tiles-setup.md).
