//! The card's tests, with a fake painter so no Skia is needed.

use std::path::PathBuf;

use tiles_intent::schema::{FormEntry, RoleEntry};
use tiles_intent::{Intent, IntentError};

use super::{
    Card, EnginePainter, Face, Fit, Heading, Look, Painted, Painter, Refusal, Tile, make,
    save::card_id, text_result,
};
use crate::board;
use crate::cache::Cache;
use crate::instructions::Draw;

/// A painter that checks the roles a form needs and draws a fixed PNG,
/// each card ten rows tall.
pub struct FakePainter;

impl Painter for FakePainter {
    fn forms(&self) -> Vec<FormEntry> {
        vec![FormEntry {
            name: "column".into(),
            summary: "Values over a few periods".into(),
            roles: ["x", "y"]
                .map(|r| RoleEntry {
                    name: r.into(),
                    required: true,
                    doc: r.into(),
                })
                .to_vec(),
        }]
    }

    fn fit(&self, intent: &Intent, _: bool) -> Result<Fit, IntentError> {
        if intent.form == "column" {
            Ok(Fit {
                min: 36,
                ideal: 40,
                max: 72,
            })
        } else {
            Err(IntentError::new("form", "unknown", "use column"))
        }
    }

    fn paint(
        &self,
        rows: &[Vec<Tile<'_>>],
        _: Option<&Heading>,
        look: &Look,
    ) -> Result<Painted, Refusal> {
        let mut faces = Vec::new();
        for tile in rows.iter().flatten() {
            let intent = tile.intent;
            let x = intent.column("x").unwrap();
            let y = intent.column("y").unwrap();
            intent
                .require_category_values(&[x])
                .map_err(|e| Refusal::Intent(super::within(tile.key, e)))?;
            faces.push(Face {
                title: intent.title.clone(),
                form: intent.form.clone(),
                values: intent
                    .table
                    .rows
                    .iter()
                    .map(|r| format!("{}: {}", r[x].label(), r[y].label()))
                    .collect(),
                notes: intent.notes.clone(),
                fetched: None,
            });
        }
        let sizes: Vec<Vec<(u16, u16)>> = rows
            .iter()
            .map(|r| r.iter().map(|t| (t.cols, 10)).collect())
            .collect();
        let (_, cells) = board::place(&sizes);
        Ok(Painted {
            png: [b"\x89PNG fake ".as_slice(), look.theme.name().as_bytes()].concat(),
            width_px: super::whole(f64::from(look.width_css(cells.0) * look.scale), 1 << 16),
            height_px: 320,
            cells,
            faces,
            frames: (1..look.frames)
                .map(|i| format!("frame {i}").into_bytes())
                .collect(),
            frame_ms: 39,
        })
    }
}

fn face() -> Face {
    Face {
        title: "Signups dipped in W3".into(),
        form: "column".into(),
        values: vec!["week: W3 · signups: 298".into()],
        notes: Vec::new(),
        fetched: None,
    }
}

fn card() -> Card {
    Card {
        id: "a1b2c3".into(),
        path: PathBuf::from("/c/a1b2c3.png"),
        png: vec![],
        title: "Signups dipped in W3".into(),
        cells: (40, 3),
        size_px: (640, 90),
        faces: vec![face()],
        heading: None,
        lines: 1,
        stacked: None,
    }
}

fn cache() -> Cache {
    let dir = std::env::temp_dir().join(format!("tiles-card-test-{}", std::process::id()));
    Cache::open(&dir).unwrap()
}

fn no_omni(_: super::Query<'_>, _: std::time::Instant) -> Result<super::Fetched, String> {
    Err("no Omni here".into())
}

/// The big-number fixture with its own title, its values and goal scaled.
fn big_number(title: &str, scale: f64) -> serde_json::Value {
    let json = include_str!("../../../tiles-engine/tests/intents/big-number.json");
    let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
    v["title"] = title.into();
    for row in v["data"]["rows"].as_array_mut().unwrap() {
        for key in ["arr", "goal"] {
            row[key] = (row[key].as_f64().unwrap() * scale).round().into();
        }
    }
    v
}

/// Warp on a laptop, 2026-10-18: a cell reported as 8x16 points
/// made the column card 70 columns by about 51 rows, twice its size,
/// with its headline above the screen.
#[test]
fn a_card_fits_the_rows_a_cropping_route_shows() {
    let painter = EnginePainter::new().unwrap();
    let json = include_str!("../../../tiles-engine/tests/intents/column.json");
    let warp = Look {
        term_cols: 154,
        ..Look::default()
    };
    let free = make(json, &painter, &no_omni, &cache(), &warp).unwrap();
    assert_eq!(free.cells.0, 61, "the form's width for six weeks");
    let tall = free.cells.1;
    for term_rows in [50, 30, 16] {
        let look = Look {
            term_rows: Some(term_rows),
            ..warp
        };
        let most = look.card_rows().unwrap();
        let card = make(json, &painter, &no_omni, &cache(), &look).unwrap();
        assert!(card.cells.1 <= most, "{term_rows} rows: {:?}", card.cells);
        assert!(
            card.cells.0 <= look.room(),
            "{term_rows} rows: {:?}",
            card.cells
        );
        if tall <= most {
            assert_eq!(card.cells, free.cells, "a card that fits stays as it is");
        } else {
            assert!(card.cells.0 < free.cells.0, "{:?}", card.cells);
            assert!(card.size_px.0 >= free.size_px.0, "as sharp, on fewer cells");
        }
        // The headline is on the card's first rows: the same layout,
        // laid out on the cells the fitted card covers.
        let fitted = Look {
            cell_px: (
                f32::from(u16::try_from(card.size_px.0).unwrap()) / f32::from(card.cells.0),
                f32::from(u16::try_from(card.size_px.1).unwrap()) / f32::from(card.cells.1),
            ),
            ..look
        };
        let intent = tiles_intent::parse(json).unwrap();
        let tile = [Tile {
            intent: &intent,
            cols: card.cells.0,
            key: "",
        }];
        let faces = painter.row_faces(&tile, &fitted, false).unwrap();
        assert!(faces[0].0.part(tiles_engine::scene::Part::Title).count() > 0);
        let last = faces[0]
            .0
            .part(tiles_engine::scene::Part::Title)
            .map(|t| t.bounds().bottom())
            .fold(0.0, f32::max);
        let row = last / fitted.cell_height_css();
        assert!(
            row < 4.0,
            "{term_rows} rows: the headline ends on row {row}"
        );
    }
    assert_eq!(Look::default().card_rows(), None, "kitty and Ghostty");
}

#[test]
fn a_dashboard_fitted_to_the_rows_stays_within_the_room() {
    let painter = EnginePainter::new().unwrap();
    let json = dashboard().to_string();
    for (term_cols, term_rows) in [(154, 50), (154, 30), (100, 24), (80, 20)] {
        let look = Look {
            term_cols,
            term_rows: Some(term_rows),
            ..Look::default()
        };
        let card = make(&json, &painter, &no_omni, &cache(), &look).unwrap();
        let most = look.card_rows().unwrap();
        assert!(
            card.cells.0 <= look.room() && card.cells.1 <= most,
            "{term_cols}x{term_rows}: {:?}",
            card.cells
        );
    }
}

/// A launcher build of `OpenCode`, 2026-10-19: a tiles server from an
/// earlier run, without the session's rows and reading Warp's 8x16-point
/// cell as device pixels, refused the four-card dashboard three times
/// ("the cards stack 123 rows tall in this terminal, over 99"). With the
/// geometry `tiles run opencode` gives its own server, 108 columns beside
/// `OpenCode`'s sidebar and 51 rows in Warp, the same call is fitted and
/// drawn, and with the rows even the stale cell fits before the refusal.
#[test]
fn a_dashboard_refused_without_the_rows_fits_with_the_sessions() {
    let painter = EnginePainter::new().unwrap();
    let json = dashboard().to_string();
    let stale = Look {
        term_cols: 108,
        cell_px: (8.0, 16.0),
        ..Look::default()
    };
    let Err(Refusal::Intent(e)) = make(&json, &painter, &no_omni, &cache(), &stale) else {
        panic!("a server without the rows draws it");
    };
    let said = e.to_string();
    assert!(
        said.contains("rows tall in this terminal, over 99"),
        "{said}"
    );
    for cell_px in [(16.0, 32.0), (8.0, 16.0)] {
        let session = Look {
            term_rows: Some(51),
            cell_px,
            ..stale
        };
        let card = make(&json, &painter, &no_omni, &cache(), &session).unwrap();
        assert!(
            card.cells.0 <= session.room() && card.cells.1 <= session.card_rows().unwrap(),
            "{cell_px:?}: {:?}",
            card.cells
        );
    }
}

#[test]
fn the_block_text_carries_every_row_token() {
    let text = text_result(&card(), Draw::Block, false, &[]);
    assert!(text.contains("(id a1b2c3, 40x3 cells)"));
    assert!(text.contains(
        "\n\n```tiles\n[tile:a1b2c3 01/03] Signups dipped in W3\n[tile:a1b2c3 02/03]\n[tile:a1b2c3 03/03]\n```\n"
    ));
    // OpenCode draws a bare [tile:] line as a link without its brackets,
    // so the block goes to the model inside a fence (ADR-052).
    assert!(
        text.contains("inside the ```tiles code fence shown, fence lines included"),
        "{text}"
    );
    assert!(
        !text.contains("plain lines") && !text.contains("never inside"),
        "{text}"
    );
    assert!(text.contains("week: W3 · signups: 298"));
    assert!(text.contains("PNG: /c/a1b2c3.png"));
}

#[test]
fn the_image_text_has_no_block() {
    let text = text_result(&card(), Draw::Image, false, &[]);
    assert!(!text.contains("[tile:"));
    assert!(text.contains("below this result"));
}

#[test]
fn the_text_route_names_the_companion_window_only_when_it_is_on() {
    let off = text_result(&card(), Draw::Text, false, &[]);
    assert!(off.contains("give the user its path"), "{off}");
    assert!(!off.contains("Tiles window"), "{off}");
    assert_eq!(
        off.contains("by running `tiles open a1b2c3`. Tell them so in a few words."),
        cfg!(target_os = "macos"),
        "{off}"
    );
    let on = text_result(&card(), Draw::Text, true, &[]);
    assert!(
        on.contains(
            " The user already sees the card in the Tiles window beside this terminal; say \
             so in one line and do not ask them to open anything. The PNG path below is for \
             reference.\n"
        ),
        "{on}"
    );
    assert!(on.contains("PNG: /c/a1b2c3.png"), "{on}");
    assert!(on.contains("week: W3 · signups: 298"), "{on}");
    assert!(
        !on.contains("tiles open") && !on.contains("give the user its path"),
        "{on}"
    );
    for draw in [Draw::Image, Draw::Block] {
        assert_eq!(
            text_result(&card(), draw, true, &[]),
            text_result(&card(), draw, false, &[]),
            "{draw:?} has no companion window"
        );
    }
}

#[test]
fn a_fetched_build_labels_a_lone_card_or_a_dashboard_that_has_none() {
    let intent = || {
        tiles_intent::parse(
            r#"{"v":1,"form":"column","title":"T","fields":{"x":"a","y":"b"},"data":{"rows":[{"a":"x","b":1}]}}"#,
        )
        .unwrap()
    };
    let fetched = |label: Option<&str>| {
        Some(super::Fetched {
            table: tiles_intent::source::Table::from_json_rows(Vec::new(), None, "t").unwrap(),
            origin: "BigQuery".into(),
            notes: Vec::new(),
            label: label.map(str::to_string),
        })
    };
    let dev = || fetched(Some("dev build · personal_ada.fct_payments"));
    let mut lone = [intent()];
    super::request::label(&mut lone, None, &[dev()]);
    assert_eq!(
        lone[0].subtitle.as_deref(),
        Some("dev build · personal_ada.fct_payments")
    );
    let mut titled = [intent()];
    titled[0].subtitle = Some("The model's own".into());
    super::request::label(&mut titled, None, &[dev()]);
    assert_eq!(titled[0].subtitle.as_deref(), Some("The model's own"));
    let mut cards = [intent(), intent()];
    let mut heading = Heading {
        title: "Payments".into(),
        note: None,
    };
    super::request::label(&mut cards, Some(&mut heading), &[dev(), dev()]);
    assert_eq!(
        heading.note.as_deref(),
        Some("dev build · personal_ada.fct_payments")
    );
    assert!(
        cards.iter().all(|c| c.subtitle.is_none()),
        "cards keep no subtitle"
    );
    let mut mixed = Heading {
        title: "Payments".into(),
        note: None,
    };
    super::request::label(
        &mut cards,
        Some(&mut mixed),
        &[dev(), fetched(Some("prod build · dbt_marts.fct_payments"))],
    );
    assert_eq!(mixed.note, None, "two builds share no note");
    let mut partly = Heading {
        title: "Payments".into(),
        note: None,
    };
    super::request::label(&mut cards, Some(&mut partly), &[dev(), fetched(None)]);
    assert_eq!(partly.note, None, "a card read from tables named in full");
    let mut inline = [intent()];
    super::request::label(&mut inline, None, &[None]);
    assert_eq!(inline[0].subtitle, None, "rows the model sent get no label");
}

#[test]
fn an_omni_card_keeps_its_values_on_this_machine() {
    let omni = Card {
        faces: vec![Face {
            fetched: Some("Omni".into()),
            ..face()
        }],
        ..card()
    };
    for draw in [Draw::Image, Draw::Block, Draw::Text] {
        let text = text_result(&omni, draw, false, &[]);
        assert!(!text.contains("298") && !text.contains("W3:"), "{draw:?}");
        assert!(!text.contains("Values on the card:"), "{draw:?}");
        assert!(text.contains("Tiles drew \"Signups dipped in W3\" as a column card (id a1b2c3"));
        assert!(
            text.contains("PNG: /c/a1b2c3.png"),
            "the add-ons still find the file"
        );
    }
    let shared = text_result(&omni, Draw::Image, false, &["Omni".to_string()]);
    assert!(shared.contains("week: W3 · signups: 298"), "the opt-in");
    let inline = text_result(&card(), Draw::Image, false, &[]);
    assert!(
        inline.contains("week: W3 · signups: 298"),
        "rows the model sent"
    );
}

#[test]
fn a_row_with_one_omni_card_keeps_only_that_cards_values_here() {
    let row = Card {
        title: "A · B".into(),
        cells: (81, 3),
        faces: vec![
            Face {
                title: "A".into(),
                ..face()
            },
            Face {
                title: "B".into(),
                values: vec!["week: W1 · orders: 777".into()],
                fetched: Some("Omni".into()),
                ..face()
            },
        ],
        ..card()
    };
    let text = text_result(&row, Draw::Block, false, &[]);
    assert!(text.starts_with(
        "Tiles drew 2 cards side by side in one row as one picture (id a1b2c3, 81x3 cells): \
         \"A\" (column), \"B\" (column)."
    ));
    assert!(text.contains("Values on \"A\":\n  week: W3 · signups: 298\n"));
    assert!(!text.contains("777"));
    assert!(text.contains("The rows of \"B\" came from Omni"));
    assert!(text.contains("[tile:a1b2c3 01/03] A · B\n"));
    assert_eq!(text.matches("PNG: /c/a1b2c3.png").count(), 1);
    assert!(text.ends_with("do not state or guess any of its numbers."));
}

#[test]
fn a_big_number_ignores_a_highlight_and_says_so() {
    let mut v = big_number("ARR reached $4.82M", 1.0);
    v["highlight"] = serde_json::json!(["Q3 26"]);
    let card = make(
        &v.to_string(),
        &EnginePainter::new().unwrap(),
        &no_omni,
        &cache(),
        &Look::default(),
    )
    .unwrap();
    let note = "Form big-number has no category to colour, so Tiles ignored highlight.";
    assert_eq!(card.faces[0].notes, [note]);
    let text = text_result(&card, Draw::Block, false, &[]);
    let (cols, rows) = card.cells;
    assert!(
        text.contains(&format!(
            "{cols}x{rows} cells). {note} Put all {rows} lines"
        )),
        "{text}"
    );
}

#[test]
fn a_value_highlight_returns_its_note_without_an_omni_label() {
    let fetch = |query: super::Query<'_>, _: std::time::Instant| {
        let super::Query::Omni(source) = query else {
            return Err("not Omni".to_string());
        };
        let rows: Vec<serde_json::Map<String, serde_json::Value>> =
            serde_json::from_value(serde_json::json!([
                { "orders.week": "Wk Acme", "orders.count": 3 },
                { "orders.week": "Wk Initech", "orders.count": 9 },
            ]))
            .unwrap();
        tiles_intent::source::Table::from_json_rows(rows, Some(&source.fields), "omni.query")
            .map(super::Fetched::omni)
            .map_err(|e| e.to_string())
    };
    let v = serde_json::json!({
        "v": 1, "form": "column", "title": "Orders peaked",
        "claim": { "kind": "rank", "tone": "good" },
        "fields": { "x": "orders.week", "y": "orders.count" },
        "highlight": ["9"],
        "data": { "omni": { "topic": "orders", "fields": ["orders.week", "orders.count"] } }
    });
    let card = make(
        &v.to_string(),
        &EnginePainter::new().unwrap(),
        &fetch,
        &cache(),
        &Look::default(),
    )
    .unwrap();
    let note = "Highlight \"9\" is a value, not a category, so Tiles highlighted the row it \
                is in.";
    assert_eq!(card.faces[0].notes, [note]);
    let text = text_result(&card, Draw::Image, false, &[]);
    assert!(text.contains(note), "{text}");
    assert!(
        !text.contains("Initech") && !text.contains("Acme"),
        "{text}"
    );
}

#[test]
fn the_entrance_is_saved_beside_a_row_when_the_wrapper_plays_it() {
    let painter = EnginePainter::new().unwrap();
    let look = Look {
        term_cols: 120,
        frames: 15,
        ..Look::default()
    };
    let mut two = big_number("ARR reached $4.82M", 1.0);
    two["row"] = serde_json::json!([big_number("Users reached 482K", 0.1)]);
    let cache = cache();
    let card = make(&two.to_string(), &painter, &no_omni, &cache, &look).unwrap();
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cache.meta_path(&card.id)).unwrap()).unwrap();
    assert_eq!(
        meta["frames"], 11,
        "450 ms and one 50 ms stagger, 50 ms apart: 10 entrance frames and the settled picture"
    );
    assert_eq!(meta["frame_ms"], 50);
    assert_eq!(meta["session"], serde_json::Value::Null);
    let size = |png: &[u8]| {
        let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
        (word(16), word(20))
    };
    let (w, h) = size(&card.png);
    for n in 0..11 {
        let png = std::fs::read(cache.frame_path(&card.id, n)).unwrap();
        assert!(png.starts_with(b"\x89PNG"), "frame {n}");
        // Each frame is the whole row at half its resolution.
        let (fw, fh) = size(&png);
        assert!(
            fw.abs_diff(w / 2) <= 1 && fh.abs_diff(h / 2) <= 1,
            "frame {n}: {fw}x{fh} against {w}x{h}"
        );
    }
    assert!(!cache.frame_path(&card.id, 11).exists());
    let still = make(
        &two.to_string(),
        &painter,
        &no_omni,
        &cache,
        &Look { frames: 0, ..look },
    )
    .unwrap();
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cache.meta_path(&still.id)).unwrap()).unwrap();
    assert_eq!(meta["frames"], 0);
}

#[test]
fn two_big_numbers_in_a_row_are_compact_and_alike() {
    let painter = EnginePainter::new().unwrap();
    let look = Look {
        term_cols: 120,
        ..Look::default()
    };
    let one = big_number("ARR reached $4.82M", 1.0);
    let alone = make(&one.to_string(), &painter, &no_omni, &cache(), &look).unwrap();
    let mut two = one.clone();
    two["row"] = serde_json::json!([big_number("Users reached 482K", 0.1)]);
    let row = make(&two.to_string(), &painter, &no_omni, &cache(), &look).unwrap();
    assert_eq!(alone.cells.0, 39, "35 + 0.5 x 7 quarters, rounded up");
    // Among others a scorecard takes its form's narrowest width and a
    // compact layout.
    assert_eq!(row.cells.0, 2 * 35 + board::GAP_COLS);
    assert!(
        row.cells.1 < alone.cells.1,
        "{:?} {:?}",
        row.cells,
        alone.cells
    );
    assert_eq!(row.title, "ARR reached $4.82M · Users reached 482K");
    assert_eq!(row.forms(), "big-number, big-number");
    assert!(row.stacked.is_none());
    let text = text_result(&row, Draw::Block, false, &[]);
    assert!(
        text.contains(
            "Values on \"Users reached 482K\":\n  arr: 482K · quarter: Q3 26 · goal: 500K"
        ),
        "{text}"
    );
}

#[test]
fn an_empty_or_null_row_draws_the_lone_card() {
    let painter = EnginePainter::new().unwrap();
    let look = Look::default();
    let one = big_number("ARR reached $4.82M", 1.0);
    let alone = make(&one.to_string(), &painter, &no_omni, &cache(), &look).unwrap();
    for row in [serde_json::json!([]), serde_json::Value::Null] {
        let mut v = one.clone();
        v["row"] = row.clone();
        let card = make(&v.to_string(), &painter, &no_omni, &cache(), &look)
            .unwrap_or_else(|e| panic!("row {row}: {e}"));
        assert_eq!((card.cells, card.size_px), (alone.cells, alone.size_px));
        assert_eq!(card.forms(), "big-number");
    }
}

#[test]
fn two_big_numbers_sit_side_by_side_in_80_columns() {
    let look = Look {
        term_cols: 80,
        ..Look::default()
    };
    let mut v = big_number("ARR reached $4.82M", 1.0);
    v["row"] = serde_json::json!([big_number("Users reached 482K", 0.1)]);
    let card = make(
        &v.to_string(),
        &EnginePainter::new().unwrap(),
        &no_omni,
        &cache(),
        &look,
    )
    .unwrap();
    assert!(card.stacked.is_none(), "{:?}", card.stacked);
    assert_eq!(card.cells.0, 2 * 35 + board::GAP_COLS);
    assert!(card.cells.0 <= look.room());
}

#[test]
fn a_row_too_wide_for_the_terminal_stacks_and_says_so() {
    let painter = EnginePainter::new().unwrap();
    let look = Look {
        term_cols: 70,
        ..Look::default()
    };
    let mut v = big_number("ARR reached $4.82M", 1.0);
    v["row"] = serde_json::json!([big_number("Users reached 482K", 0.1)]);
    let card = make(&v.to_string(), &painter, &no_omni, &cache(), &look).unwrap();
    let alone = make(
        &big_number("ARR reached $4.82M", 1.0).to_string(),
        &painter,
        &no_omni,
        &cache(),
        &look,
    )
    .unwrap();
    assert_eq!(card.cells.0, 35, "compact, as in a row");
    assert!(card.cells.1 < 2 * alone.cells.1);
    let note = card.stacked.as_deref().unwrap();
    assert_eq!(
        note,
        "The 2 cards need at least 79 columns side by side and this terminal has 70, so Tiles \
         stacked them one above the other."
    );
    let text = text_result(&card, Draw::Block, false, &[]);
    assert!(text.contains("2 cards one above the other as one picture"));
    assert!(text.contains(note));
}

/// Cards in a row line up: a title that wraps pushes the other card's
/// parts down with it, and both cards come out as tall.
#[test]
fn the_cards_of_a_row_line_up_part_for_part() {
    use tiles_engine::scene::{Part, Shape};
    let painter = EnginePainter::new().unwrap();
    let look = Look::default();
    let bare = |title: &str, scale: f64| {
        let mut v = big_number(title, scale);
        v.as_object_mut().unwrap().remove("subtitle");
        tiles_intent::parse(&v.to_string()).unwrap()
    };
    let long: Intent = bare(
        "Annual recurring revenue reached a record $4.82M this quarter",
        1.0,
    );
    let short: Intent = bare("Users", 0.1);
    let row = [
        Tile {
            intent: &long,
            cols: 36,
            key: "",
        },
        Tile {
            intent: &short,
            cols: 36,
            key: "row[0]",
        },
    ];
    let faces = painter.row_faces(&row, &look, true).unwrap();
    let first_y = |part: Part, scene: &tiles_engine::Scene| {
        scene
            .items
            .iter()
            .find(|i| i.part == part)
            .map(|i| match &i.shape {
                Shape::Text { y, .. } => *y,
                _ => f32::NAN,
            })
    };
    let (a, b) = (&faces[0].0, &faces[1].0);
    let titles = |s: &tiles_engine::Scene| s.items.iter().filter(|i| i.part == Part::Title).count();
    assert!(titles(a) > 1, "the long title wraps");
    assert_eq!(
        first_y(Part::Value, a),
        first_y(Part::Value, b),
        "big numbers"
    );
    assert_eq!(a.height, b.height);
    assert_eq!(a.card, b.card);
    // A compact scorecard has no default subtitle.
    assert_eq!(first_y(Part::Subtitle, b), None);
}

#[test]
fn a_row_refusal_names_the_card() {
    let painter = EnginePainter::new().unwrap();
    let refuse = |v: serde_json::Value| {
        make(
            &v.to_string(),
            &painter,
            &no_omni,
            &cache(),
            &Look::default(),
        )
        .unwrap_err()
        .to_string()
    };
    let mut bad = big_number("Users reached 482K", 0.1);
    bad["fields"]["y"] = "missing".into();
    let mut v = big_number("ARR reached $4.82M", 1.0);
    v["row"] = serde_json::json!([bad]);
    assert!(refuse(v).starts_with("row[0].fields.y:"));
    let four: Vec<_> = (0..4)
        .map(|_| big_number("ARR reached $4.82M", 1.0))
        .collect();
    let mut v = big_number("ARR reached $4.82M", 1.0);
    v["row"] = serde_json::json!(four);
    assert!(refuse(v).starts_with("row: a row holds 1 to 4 cards and this one has 5;"));
    // The call's own intent is the first card; a row alone is no intent.
    let bare = serde_json::json!({ "row": [
        big_number("ARR reached $4.82M", 1.0),
        big_number("Users reached 482K", 0.1),
    ] });
    assert!(refuse(bare).starts_with("arguments: not a valid chart"));
    let mut v = big_number("ARR reached $4.82M", 1.0);
    v["row"] = "not a list".into();
    assert!(refuse(v).starts_with("row: row must be an array"));
    let mut nested = big_number("ARR reached $4.82M", 1.0);
    nested["row"] = serde_json::json!([big_number("ARR reached $4.82M", 1.0)]);
    let mut v = big_number("ARR reached $4.82M", 1.0);
    v["row"] = serde_json::json!([nested]);
    assert!(refuse(v).starts_with("row[0].row:"));
}

/// The live check's dashboard: three scorecards over a line chart of
/// 12 months, under a title and a note.
fn dashboard() -> serde_json::Value {
    let line = include_str!("../../../tiles-engine/tests/intents/line.json");
    let mut line: serde_json::Value = serde_json::from_str(line).unwrap();
    line["title"] = "Revenue by month".into();
    let mut v = big_number("Revenue", 1.0);
    v["row"] = serde_json::json!([big_number("Active users", 0.1), big_number("Churn", 0.001)]);
    v["rows"] = serde_json::json!([[line]]);
    v["dashboard"] =
        serde_json::json!({ "title": "Business, last 12 months", "note": "Example data" });
    v
}

#[test]
fn a_dashboard_heading_takes_a_joiner_only_between_two_emoji() {
    let painter = EnginePainter::new().unwrap();
    let heading = |key: &str, text: &str| {
        let mut v = dashboard();
        v["dashboard"][key] = text.into();
        make(
            &v.to_string(),
            &painter,
            &no_omni,
            &cache(),
            &Look::default(),
        )
    };
    let card = heading("title", "👩🏽\u{200D}💻 Priya's team").unwrap();
    assert_eq!(card.heading.as_deref(), Some("👩🏽\u{200D}💻 Priya's team"));
    assert!(heading("note", "👩🏽\u{200D}💻 on call").is_ok());
    for key in ["title", "note"] {
        let e = heading(key, "Pri\u{200D}ya").unwrap_err().to_string();
        assert!(
            e.starts_with(&format!(
                "dashboard.{key}: contains the invisible character U+200D"
            )),
            "{e}"
        );
    }
}

#[test]
fn a_dashboard_packs_to_the_terminal_and_plays_card_by_card() {
    let painter = EnginePainter::new().unwrap();
    let at = |cols: u16| Look {
        term_cols: cols,
        ..Look::default()
    };
    let wide = make(
        &dashboard().to_string(),
        &painter,
        &no_omni,
        &cache(),
        &at(120),
    )
    .unwrap();
    assert_eq!(wide.heading.as_deref(), Some("Business, last 12 months"));
    assert_eq!(wide.title, "Business, last 12 months");
    assert_eq!(wide.lines, 2);
    assert_eq!(
        wide.cells.0,
        3 * 35 + 2 * board::GAP_COLS,
        "the line chart stretches to the scorecards"
    );
    assert!(wide.stacked.is_none());
    assert_eq!(wide.forms(), "big-number, big-number, big-number, line");
    let text = text_result(&wide, Draw::Block, false, &[]);
    assert!(
        text.starts_with(
            "Tiles drew a dashboard \"Business, last 12 months\" of 4 cards in 2 rows as one \
             picture"
        ),
        "{text}"
    );
    assert!(text.contains("] Business, last 12 months\n"), "{text}");
    let same = make(
        &dashboard().to_string(),
        &painter,
        &no_omni,
        &cache(),
        &at(200),
    )
    .unwrap();
    assert_eq!(
        same.cells, wide.cells,
        "at 200 columns it keeps its content's size"
    );

    let narrow = make(
        &dashboard().to_string(),
        &painter,
        &no_omni,
        &cache(),
        &at(80),
    )
    .unwrap();
    assert_eq!(narrow.lines, 3);
    assert!(narrow.cells.0 <= at(80).room());
    assert!(narrow.cells.1 > wide.cells.1);
    assert_eq!(
        narrow.stacked.as_deref(),
        Some(
            "Row 1's 3 cards need at least 115 columns side by side and this terminal has 80, \
             so Tiles put them on 2 lines."
        )
    );
    assert!(
        text_result(&narrow, Draw::Block, false, &[]).contains("so Tiles put them on 2 lines.")
    );

    // Without its heading the dashboard is a heading's rows shorter.
    let mut bare = dashboard();
    bare.as_object_mut().unwrap().remove("dashboard");
    let bare = make(&bare.to_string(), &painter, &no_omni, &cache(), &at(120)).unwrap();
    assert!(
        bare.cells.1 + 2 <= wide.cells.1,
        "{:?} {:?}",
        bare.cells,
        wide.cells
    );
    assert_eq!(
        bare.title,
        "Revenue · Active users · Churn · Revenue by month"
    );

    let frames = Look {
        frames: 15,
        ..at(120)
    };
    let cache = cache();
    let card = make(
        &dashboard().to_string(),
        &painter,
        &no_omni,
        &cache,
        &frames,
    )
    .unwrap();
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cache.meta_path(&card.id)).unwrap()).unwrap();
    // The heading and four cards enter 50 ms apart, 200 ms in all:
    // 650 ms, 50 ms apart, is 13 entrance frames and the settled
    // picture.
    assert_eq!(meta["frame_ms"], 50);
    assert_eq!(meta["frames"], 14);
}

/// The live ask as a model sent it on 2026-10-18: all four cards in one
/// row, the first with a subtitle repeating its title and the other two
/// scorecards one value each.
#[test]
fn the_scorecards_of_a_one_row_ask_go_on_top_and_keep_their_height() {
    let painter = EnginePainter::new().unwrap();
    let look = Look {
        term_cols: 136,
        ..Look::default()
    };
    let mut v = dashboard();
    let line = v["rows"][0][0].clone();
    let mut users = big_number("Active users", 0.1);
    users["data"]["rows"] =
        serde_json::json!([{ "quarter": "Q3 26", "arr": 43800, "goal": 50000 }]);
    v["subtitle"] = "Revenue".into();
    v["title"] = "Revenue".into();
    v["row"] = serde_json::json!([users, big_number("Churn", 0.001), line]);
    v.as_object_mut().unwrap().remove("rows");
    let card = make(&v.to_string(), &painter, &no_omni, &cache(), &look).unwrap();
    assert_eq!(card.lines, 2);
    assert_eq!(card.cells.0, 3 * 35 + 2 * board::GAP_COLS);
    assert_eq!(
        card.stacked.as_deref(),
        Some(
            "Tiles put the scorecards in a row of their own above the charts, so each card \
             keeps its own height."
        )
    );
    let alone = make(
        &dashboard().to_string(),
        &painter,
        &no_omni,
        &cache(),
        &look,
    )
    .unwrap();
    assert_eq!(
        card.cells, alone.cells,
        "laid out as the dashboard asked for in rows"
    );
    let revenue = tiles_intent::parse(&{
        let mut r = big_number("Revenue", 1.0);
        r["subtitle"] = "revenue ".into();
        r.to_string()
    })
    .unwrap();
    let tile = [Tile {
        intent: &revenue,
        cols: 40,
        key: "",
    }];
    let faces = painter.row_faces(&tile, &look, false).unwrap();
    assert_eq!(
        faces[0].0.part(tiles_engine::scene::Part::Subtitle).count(),
        0,
        "a subtitle repeating the title is dropped"
    );
}

/// The two refusals the live dashboard hit on 2026-10-18 draw now, each
/// with a note in the result: a value named as the line chart's
/// highlight, and an empty unit on a scorecard.
#[test]
fn the_live_dashboards_slips_draw_with_a_note() {
    let painter = EnginePainter::new().unwrap();
    let mut v = dashboard();
    let line = &mut v["rows"][0][0];
    let last = line["data"]["rows"].as_array().unwrap().last().unwrap()["teams"].clone();
    line["highlight"] = serde_json::json!([last.to_string()]);
    v["row"][0]["unit"] = "".into();
    let card = make(
        &v.to_string(),
        &painter,
        &no_omni,
        &cache(),
        &Look::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let text = text_result(&card, Draw::Block, false, &[]);
    assert!(
        text.contains(&format!(
            "Highlight \"{last}\" is a value, not a category, so Tiles highlighted the row \
             it is in."
        )),
        "{text}"
    );
    assert!(text.contains("Tiles read unit \"\" as no unit."), "{text}");
    assert!(!text.contains("refused"));
}

#[test]
fn a_chart_among_other_cards_has_no_default_subtitle() {
    use tiles_engine::scene::Part;
    let painter = EnginePainter::new().unwrap();
    let line = tiles_intent::parse(include_str!(
        "../../../tiles-engine/tests/intents/line.json"
    ))
    .unwrap();
    let tile = [Tile {
        intent: &line,
        cols: 72,
        key: "",
    }];
    let subtitles = |among: bool| {
        let faces = painter.row_faces(&tile, &Look::default(), among).unwrap();
        faces[0].0.part(Part::Subtitle).count()
    };
    assert!(subtitles(false) > 0, "alone it keeps its default subtitle");
    assert_eq!(subtitles(true), 0);
}

#[test]
fn card_ids_are_six_hex_and_differ() {
    let a = card_id(b"same");
    let b = card_id(b"same");
    assert!(tiles_term::block::is_card_id(&a));
    assert_ne!(a, b);
}

#[test]
fn a_spotlit_multi_line_card_lists_its_values() {
    let json = include_str!("../../../tiles-engine/tests/intents/gallery/multi-line.json");
    let intent = tiles_intent::parse(json).unwrap();
    assert_eq!(intent.highlight, ["Germany"]);
    let card = make(
        json,
        &EnginePainter::new().unwrap(),
        &no_omni,
        &cache(),
        &Look::default(),
    )
    .unwrap();
    assert_eq!(card.faces[0].values.len(), intent.table.rows.len());
}

#[test]
fn card_width_follows_the_content_within_the_terminal() {
    let look = |cols| Look {
        term_cols: cols,
        ..Look::default()
    };
    let line = Fit {
        min: 48,
        ideal: 66,
        max: 72,
    };
    assert_eq!(look(20).card_cols(line), 35, "the engine's narrowest tile");
    assert_eq!(look(60).card_cols(line), 52, "the terminal less a margin");
    assert_eq!(look(300).card_cols(line), 66, "what the content asks for");
    let wide = Fit {
        min: 60,
        ideal: 90,
        max: 90,
    };
    assert_eq!(look(300).card_cols(wide), 72);
    assert!((look(60).width_css(52) - 416.0).abs() < 0.01);
}
