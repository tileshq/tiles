# ADR-050: macOS system fonts set what Inter cannot, an emoji joiner is accepted, and a cut right-to-left label ends on its left

- Status: Accepted
- Date: 2026-10-21

## Context

The worst-case goldens ([ADR-049](ADR-049-worst-case-cards.md)) showed three faults that needed more than a local fix:

- The engine bundles Inter 400, 500 and 600 and turns font fallback off (playbook T1, [ADR-002](ADR-002-native-engine.md), [ADR-010](ADR-010-engine-v0-1.md)), so 王秀英, نور الهدى عبد الرحمن, 🦊 and 👩🏽 drew as empty boxes on every form.
- A model that sent `👩🏽‍💻 Priya` was refused with "contains the invisible character U+200D", while `text::strip_format` keeps that same joiner in an Omni cell ([ADR-024](ADR-024-security-baseline-in-ci.md), F6).
- A label too wide for its room kept its first characters and put the ellipsis after them. For an Arabic name that cut is right, since the name reads from the right, but the ellipsis then drew on the name's right, away from the cut.

Tiles ships for macOS only ([ADR-035](ADR-035-macos-package-and-tiles-setup.md)), and every Mac carries fonts for these scripts.

## Decision

- **Three macOS fonts sit behind Inter.** On macOS, `text::Fonts` asks Skia's font manager for PingFang SC (Chinese, Japanese and Korean), Geeza Pro (Arabic) and Apple Color Emoji, at 400, 500 and 600, and registers each one the machine has. The paragraph tries Inter first, then those three in that order, for each character. Skia's own fallback stays off, so no other system font is ever used, and no font is added to the binary. PingFang SC and Geeza Pro glyphs are drawn as unhinted outlines like Inter's. Apple Color Emoji glyphs are colour pictures with no outline, so they are drawn as glyphs in their own colours, taking only the item's fade and opacity.
- **A card is the same on one machine, and the goldens are the same in CI.** The system fonts change between macOS releases, so a card with CJK, Arabic or emoji text can differ from one Mac to another, and stays the same on any one Mac. A card in Inter alone is unchanged everywhere. Off macOS, or where a font is missing, those characters draw as empty boxes, as before. The worst-case goldens in `worst-case/` are drawn with `Engine::inter_only` on every machine. The worst cases holding a character Inter lacks are also drawn with the system fonts against `worst-case-macos/`, whose `fonts.txt` records each font's family, weight and a digest of its `head` table. A machine whose fonts differ from that record, Linux and the macOS CI runner among them, prints that it checked those cases with Inter alone; `TILES_BLESS=1` on a Mac redraws them and rewrites the record.
- **An intent may hold a zero width joiner between two emoji.** `text::first_refused` is the input check for every text the model sends, a dashboard's title and note and a missing title's fallback to the measure's column name included: every unsafe character ([ADR-009](ADR-009-security-baseline.md)) is refused as before, except U+200D with an emoji on each side, an emoji presentation selector allowed before it, which is the rule `strip_format` already applies to fetched cells. A joiner between letters, at either end, or beside another joiner is still refused. Printed text is unchanged: every output path still removes the joiner, so the terminal shows `👩🏽💻` where the card draws one emoji. The hostile corpus holds both an emoji sequence, which an intent accepts, and a joiner between letters, which it refuses.
- **A cut right-to-left label has its ellipsis on its left.** When a label's first letter is Hebrew, Arabic, Syriac, Thaana or N'Ko, the engine still keeps its logical start and cuts its end, and puts the ellipsis before the kept text, so it draws at the label's visual left where the cut is. Where two such labels would print alike and each shows the word where it differs, an ellipsis on one side only of that word likewise goes to the other logical end, so it draws on the side of its cut; an ellipsis between two kept parts or on both sides is unchanged.

## Consequences

- The release card can show Chinese, Arabic and emoji labels. A Mac without one of the three fonts draws boxes for that script only.
- Only a Mac whose fonts match `fonts.txt` checks the system-font goldens. Neither CI runner does, so a change to how they draw is seen on the owner's Mac, not in CI.
- The Inter-only goldens of `multi-line-worst` and `slope-worst` change: their boxed Arabic name is cut, and its ellipsis moves to the left. `column-zwj` draws instead of being refused, and `multi-line-rtl` adds a long Arabic name that is cut.
- A label that starts in Latin and ends in Arabic keeps its ellipsis on the right, which can sit away from the cut.
