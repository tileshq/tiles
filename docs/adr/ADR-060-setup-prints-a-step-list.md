# ADR-060: `tiles setup` and `install.sh` print one line per step, with marks only in a terminal

- Status: Accepted; supersedes the words [ADR-035](ADR-035-macos-package-and-tiles-setup.md) and [ADR-039](ADR-039-setup-replaces-a-stale-claude-code-server.md) give `tiles setup` to print, and the order of ADR-035's two questions and the installs; what setup changes, its one question, `--yes`, `--dry-run` and its exit codes stand
- Date: 2026-11-01

## Context

`brew install tileshq/tap/tiles` gives one command that installs Tiles, and its caveat points at `tiles setup`. Setup then printed `Tiles found:` with a path on most lines, `Tiles will change:` with every file and command, whatever `claude mcp add` and `codex mcp add` printed, and a block of data-source commands.

## Decision

- **The list.** A header `Tiles <version> setup`, one aligned line per step, `  <label>  (<state>)`, and one closing line. The steps, in order: `Tiles on PATH`, each `Other tiles`, `Terminal`, `Pictures` on the text route, each harness in `Harness::ALL`'s order, and each data source. The closing line says what to try next (`Next: run tiles check, then start a session with tiles run <harness>.`), or why not yet. While `tiles` in a terminal would not start this one, because an older copy stays ahead on `PATH` or this one is not on `PATH`, a closing line that names a `tiles` command points at that step first: `Not yet: see the Other tiles line, then run tiles check, ...`.
- **The states.** `installed`, `configured`, `already set up`, `will configure` before the question, `would configure` in a dry run, and `skipped: <reason>`. A line left for the user or a failure holds its whole reason and the fix, on that line. A second run prints the same list with `already set up`, including `Tiles on PATH`, and changes nothing.
- **Marks.** In a terminal with `NO_COLOR` unset, any value counting as set, each line starts with a mark: a green check for finished, a dim dot for pending, a dim dash for skipped, a yellow `!` for left to the user, a red cross for failed. A skipped step did nothing, so it gets no check. Otherwise the marks and colour go and the words stay the same. `tiles_term::style` holds the sequences, sanitizes every word and is in the hostile-corpus test.
- **`--verbose`.** Paths, each `Step`, the whole `pictures:` sentence and the old `tiles:` line for a copy move under their steps behind `--verbose`, and `tiles doctor` keeps its own lines. `tiles setup --dry-run --verbose` is the old list of changes.
- **Consent.** Before the question each harness to change has a line, and one with another `tiles` server says `set up with another tiles, <command>; Tiles will replace it`, so the question still names everything it covers. A line under the question names `tiles setup --dry-run --verbose` for the files and commands.
- **Questions first.** The harness question, then one question per copy offered a move, then the list, which carries out each move and install as it reaches its line. The move used to run after the installs; neither depends on the other.
- **Harness output.** Without `--verbose`, setup runs `claude` and `codex` with output captured and stdin closed, through `install::run_quiet`, and the first 300 characters of a failure's stderr join its line, ahead of the fix. `tiles install` and `tiles setup --verbose` still run them on the terminal.
- **Old installs.** An older copy ahead on `PATH`, moved or not, and a Claude Code server left or declined, name the exact command on their own line. One later on `PATH` is named and skipped, with no command, since the running one already wins. `tiles setup` still renames and never deletes.
- **`install.sh`** prints `Tiles install`, then `Download` or `Archive`, `Signature`, `Checksum`, `Tiles` and, when needed, `PATH` the same way, with the same rule for marks. `shasum`'s own line is gone. Then it runs `tiles setup`, or with no terminal closes on `Next: run tiles setup, ...`.

`crates/tiles-cli/src/setup/tests.rs` and `crates/tiles-cli/tests/setup.rs` hold whole outputs; `a_terminal_gets_marks_and_the_same_words_as_a_pipe` compares a terminal's lines with a pipe's; `scripts/test_install.sh` covers the script.

## Consequences

- The question no longer prints each file and command. The reader gets the harness names and, for a replacement, the old command; the rest is one flag away.
- `claude` or `codex` asking something on stdin would now fail at setup rather than wait. Neither asks in `mcp add` or `mcp remove` today.
- Anything parsing the old `Tiles found:` lines must change. The steps' labels and states are the new contract.
- A terminal Tiles cannot name, such as Rex today, reads `Terminal (nothing to set up; tiles check tests how cards draw here)`, since setup asks a terminal nothing.
- Related: [ADR-035](ADR-035-macos-package-and-tiles-setup.md), [ADR-039](ADR-039-setup-replaces-a-stale-claude-code-server.md), [ADR-046](ADR-046-companion-window-on-the-text-route.md), [ADR-009](ADR-009-security-baseline.md).
