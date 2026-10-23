# List the forms

`tiles forms` prints every chart form with one line on when to use it and, under it, each role with its kind, whether it is required and what it means. A model or a person reads it to write an intent.

## Sub-features

- `forms-list` lists all 15 forms in a stable order.
- `forms-roles` lists each form's roles with kind and requirement.

## How to get to it (user POV)

- Run `tiles forms`.

## Driving it with drive.sh

Preconditions:

- `scripts/doctor.sh` passes.

- **List.** Run `.agents/skills/verify-tiles/scripts/drive.sh <run> forms forms`. Exit 0; the transcript starts with `column` and lists `bar`, `line`, `area`, `big-number`, `multi-line`, `grouped-bar`, `stacked-bar`, `lollipop`, `dot-plot`, `slope`, `dumbbell`, `bullet`, `waterfall` and `scatter`.
- **Roles.** In the same transcript, `scatter` lists `x` and `y` as `Number, required` and `label` as `Category, optional`.

## Gotchas

- A form added in a debug build through `TILES_DEV_ASSETS` does not appear in the release binary; the doctor checks the release binary.
