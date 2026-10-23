# ADR-054: `tiles check` and `tiles doctor` ask `/dev/tty` only when stdin or stdout is a terminal

- Status: Accepted; amends [ADR-025](ADR-025-warp-cell-size-rows-and-laptop-check.md)'s `/dev/tty` rule
- Date: 2026-10-21

## Context

[ADR-025](ADR-025-warp-cell-size-rows-and-laptop-check.md) has `tiles check` and `tiles doctor` ask `/dev/tty` whenever stdin or stdout is not a terminal, so `tiles check | tee check.txt` still describes the terminal it ran in. The same rule applies when neither is a terminal, as when a coding agent runs `tiles check` through its shell tool with both piped. If that process still has a controlling terminal, the check opens it, puts it in raw mode and writes its queries to it: the agent's own terminal receives escape sequences it never asked for, and the report describes whatever terminal the agent's session runs in, not one the user pointed it at.

## Decision

`tiles check` and `tiles doctor` ask stdin and stdout when both are terminals, `/dev/tty` when exactly one is, and no terminal when neither is (`wrap::Probe::open_at`). With neither, the `graphics:` and `answers:` lines say `not asked` with the reason `stdin and stdout are not terminals`. `tiles run` is unchanged.

## Consequences

- A report piped from a shell, `tiles check --tag v0.1.1 | tee check.txt`, still asks the terminal through `/dev/tty`, as the laptop check needs.
- An agent's run of `tiles check` or `tiles doctor` writes nothing to any terminal and fails its `graphics:` line as `not asked`; the other lines report as before.
- A user who redirects both, `tiles check < /dev/null > check.txt`, gets the `not asked` lines too.
- Tests: `dev_tty_is_asked_only_beside_a_terminal` and `a_probe_with_no_terminal_names_dev_tty_and_why` in `crates/tiles-cli/src/wrap/mod.rs`.
