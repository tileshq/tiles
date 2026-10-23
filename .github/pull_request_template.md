<!--
Title: a conventional commit in plain language, one outcome, about 65 characters:
"fix(tiles-cli): cards keep their size when Warp zooms".
Write for someone who has never seen this repository: no chat quotes, people's
names, local paths, task ids or private plans. Those belong in your own notes.
-->

## Problem

<!-- What was wrong or missing, as a user would see it, in a sentence or two.
Link the public issue if there is one. -->

## Change

<!-- How this fixes it: the files, flags and behaviour that changed. Name the ADR
that records a design decision. -->

## Verification

<!-- What you ran and what it showed, with numbers. A bug fix says its new test
fails on the old code and how that was checked. Embed before/after images for a
visual change. End with what you could not check, on its own line. -->

## Checklist

<!-- One line per point that applies; delete the rest. -->
- Security (ADR-009): <!-- the threat-model rows a surface change meets, or "No surface touched." -->
- Pixels: <!-- why the pixels changed, with before, after and diff images, for an engine change; or "No pixels changed." -->
- New dependencies: <!-- crate, one-line reason, licence; or "None." -->
- verify-tiles: <!-- maintain-verification-skill outcome: clean, changed or blocked -->

Built with: <!-- model and harness, e.g. Claude Opus 5.5 in Claude Code; "no AI" when none was used -->
