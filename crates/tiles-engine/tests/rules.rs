//! Craft rules from `docs/playbook.md` (ids and rules in
//! `docs/card-rules.md`), tested on the scene (ADR-006). Each assertion
//! names the rule id it checks, so a failure says which rule broke.
//!
//! The rules read the default theme's scene, and every fixture scene is also
//! laid out in each other theme the build carries, which must place the same
//! items with each colour taken from its own palette (ADR-037). The sizes
//! the rules check come from the tokens, so the house design and the plain
//! theme meet the same rules.

use std::path::PathBuf;

use tiles_engine::color::Rgba;
use tiles_engine::hit::{hit_at, hit_map};
use tiles_engine::scene::{Corners, Emphasis, Grow, Item, Part, Shape};
use tiles_engine::tokens::Palette;
use tiles_engine::{Engine, Moment, Options, RenderError, Scene, Theme};

/// The light theme, which every build carries.
fn light() -> Theme {
    Theme::from_name("light").expect("the build carries a light theme")
}

/// The dark theme, which every build carries.
fn dark() -> Theme {
    Theme::from_name("dark").expect("the build carries a dark theme")
}

fn engine() -> Engine {
    Engine::new().expect("the embedded assets should load")
}

fn fixture(form: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/intents")
        .join(format!("{form}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The default theme's scene of `v`, after checking every other theme lays
/// it out the same.
fn scene_of(v: &serde_json::Value) -> Result<Scene, RenderError> {
    let intent = tiles_intent::parse(&v.to_string())?;
    let e = engine();
    let first = e.scene(&intent, &Options::default())?;
    for &theme in Theme::all() {
        if theme != first.theme {
            let opts = Options {
                theme,
                ..Options::default()
            };
            assert_same_but_colours(&e, &first, &e.scene(&intent, &opts)?);
        }
    }
    Ok(first)
}

/// The colours a role may take in a palette.
fn role_colours(p: &Palette, role: Emphasis) -> Vec<Rgba> {
    match role {
        Emphasis::Accent => vec![p.accent],
        Emphasis::Bad => vec![p.bad, p.bad_soft],
        Emphasis::Grey => vec![p.grey],
        Emphasis::Muted => vec![p.muted],
        Emphasis::Ink => vec![p.ink],
        Emphasis::Series(i) => vec![p.categorical[i]],
        Emphasis::Neutral(i) => vec![p.neutrals[i]],
        Emphasis::Context(i) => vec![p.context[i]],
        // A waterfall's step connectors are gridline-quiet in muted.
        Emphasis::Grid => vec![p.grid, p.muted],
        Emphasis::Track => vec![p.track],
        Emphasis::Card => vec![p.card],
    }
}

/// `b`, the scene of `a`'s intent in another theme, places the same items
/// in the same roles, and each item's colour is its role's in `b`'s
/// palette at `a`'s opacity.
fn assert_same_but_colours(e: &Engine, a: &Scene, b: &Scene) {
    let theme = b.theme.name();
    assert_eq!(
        (a.width, a.height, a.items.len()),
        (b.width, b.height, b.items.len()),
        "{theme}: the card's size and item count match the default theme's"
    );
    let palette = e.tokens().palette(b.theme);
    for (x, y) in a.items.iter().zip(&b.items) {
        let same = (x.part, &x.shape, x.datum, x.highlighted, x.grow, &x.tip)
            == (y.part, &y.shape, y.datum, y.highlighted, y.grow, &y.tip);
        assert!(same, "{theme}: {y:?} is laid out as {x:?}");
        let opaque = |c: Rgba| Rgba { a: 255, ..c };
        assert!(
            y.color.a == x.color.a
                && role_colours(palette, y.emphasis)
                    .into_iter()
                    .any(|c| opaque(c) == opaque(y.color)),
            "{theme}: {y:?} takes its role's colour"
        );
    }
}

fn scene(form: &str) -> Scene {
    scene_of(&fixture(form)).unwrap_or_else(|e| panic!("{form}: {e}"))
}

fn without_highlight(form: &str) -> Scene {
    let mut v = fixture(form);
    v.as_object_mut().unwrap().remove("highlight");
    scene_of(&v).unwrap()
}

fn texts(scene: &Scene, part: Part) -> Vec<String> {
    scene
        .part(part)
        .filter_map(|i| i.text().map(String::from))
        .collect()
}

fn rect(item: &Item) -> skia_safe::Rect {
    match &item.shape {
        Shape::Rect { rect, .. } => *rect,
        other => panic!("not a rect: {other:?}"),
    }
}

const ACCENT: &[Emphasis] = &[Emphasis::Accent, Emphasis::Bad];
/// Forms where colour means "named by the title" and nothing else.
const SINGLE_EMPHASIS: &[&str] = &[
    "column",
    "bar",
    "lollipop",
    "dot-plot",
    "bullet",
    "scatter",
    "multi-line",
    "slope",
    "stacked-bar",
];

#[test]
fn k1_accent_only_on_highlighted_marks() {
    for form in SINGLE_EMPHASIS {
        let s = scene(form);
        assert!(
            s.marks().any(|m| m.highlighted),
            "K1 {form}: the fixture highlights nothing"
        );
        for m in s.marks() {
            assert_eq!(
                ACCENT.contains(&m.emphasis),
                m.highlighted,
                "K1 {form}: {:?} mark, highlighted {}",
                m.emphasis,
                m.highlighted
            );
        }
    }
}

#[test]
fn k2_bad_tone_colours_the_highlight_red_and_good_tone_does_not() {
    let s = scene("column");
    let named: Vec<&Item> = s.marks().filter(|m| m.highlighted).collect();
    assert!(
        !named.is_empty() && named.iter().all(|m| m.emphasis == Emphasis::Bad),
        "K2"
    );
    let red = engine().tokens().palette(s.theme).bad;
    assert!(
        named.iter().all(|m| m.color == red),
        "K2: red is the theme's bad colour"
    );
    let mut v = fixture("column");
    v["claim"]["tone"] = "good".into();
    let s = scene_of(&v).unwrap();
    assert!(
        s.marks()
            .filter(|m| m.highlighted)
            .all(|m| m.emphasis == Emphasis::Accent),
        "K2"
    );
    assert!(
        s.marks().all(|m| m.emphasis != Emphasis::Bad),
        "K2: no red without a bad tone"
    );
}

#[test]
fn k3_single_series_without_highlight_is_all_accent() {
    for form in ["column", "bar", "lollipop", "dot-plot", "line", "bullet"] {
        let s = without_highlight(form);
        assert!(
            s.marks().all(|m| m.emphasis == Emphasis::Accent),
            "K3 {form}"
        );
    }
}

#[test]
fn k4_value_labels_take_their_marks_emphasis() {
    for form in ["column", "bar", "lollipop", "dot-plot", "bullet"] {
        let s = scene(form);
        // One value per row: a lollipop's stem and head draw the same row.
        let mut marks: Vec<&Item> = s.marks().collect();
        marks.dedup_by_key(|m| m.datum);
        let values: Vec<&Item> = s.part(Part::Value).collect();
        assert_eq!(marks.len(), values.len(), "K4 {form}: one value per mark");
        // Ranked rows write the named value in ink, as the gallery does;
        // a column's value and a bullet's share of target take their mark's
        // colour.
        let ranked = !matches!(form, "column" | "bullet");
        for (m, v) in marks.iter().zip(&values) {
            if m.highlighted && ranked {
                assert_eq!(
                    v.emphasis,
                    Emphasis::Ink,
                    "K4 {form}: a ranked row's named value is in ink"
                );
            } else if m.highlighted {
                assert_eq!(
                    v.color, m.color,
                    "K4 {form}: a named value takes its mark's colour"
                );
            } else {
                assert_eq!(
                    v.emphasis,
                    Emphasis::Muted,
                    "K4 {form}: other values are muted"
                );
            }
        }
    }
}

#[test]
fn k5_gridlines_are_horizontal_and_there_are_no_axis_lines() {
    for form in tiles_engine::forms::EMBEDDED.iter().map(|(n, _)| *n) {
        let s = scene(form);
        for g in s.part(Part::Grid) {
            let Shape::Line { points, .. } = &g.shape else {
                panic!("K5 {form}: grid is not a line")
            };
            assert!(
                points.iter().all(|p| (p.1 - points[0].1).abs() < 1e-3),
                "K5 {form}: grid not horizontal"
            );
            assert_eq!(g.emphasis, Emphasis::Grid, "K5 {form}");
        }
    }
}

#[test]
fn k6_series_follow_the_fixed_categorical_order() {
    let s = without_highlight("multi-line");
    let e = engine();
    let cat = &e.tokens().palette(s.theme).categorical;
    // Named at their ends, so the lines themselves carry the order.
    let lines: Vec<_> = s
        .marks()
        .filter(|i| matches!(i.shape, Shape::Line { .. }))
        .map(|i| i.color)
        .collect();
    assert_eq!(lines, cat[..3].to_vec(), "K6");
    let names: Vec<_> = s
        .part(Part::Legend)
        .filter(|i| i.text().is_some())
        .map(|i| i.color)
        .collect();
    assert_eq!(names.len(), 3, "K6: each line is named at its end");
    assert!(
        names.iter().all(|c| cat[..3].contains(c)),
        "K6: each name in its line's colour"
    );
}

#[test]
fn m1_bars_carry_their_values_and_have_no_y_axis() {
    for form in [
        "column",
        "bar",
        "lollipop",
        "grouped-bar",
        "stacked-bar",
        "waterfall",
    ] {
        let s = scene(form);
        assert!(!s.y_axis, "M1 {form}: a bar chart has no y axis");
        assert!(
            s.part(Part::Value).count() > 0,
            "M1 {form}: values are written on the bars"
        );
    }
    let s = scene("column");
    let labels = texts(&s, Part::Value);
    assert_eq!(labels, ["412", "431", "298", "447", "463", "489"], "M1");
    // The tokens' offset above the bar end, measured to the label's centre.
    let offset = engine().tokens().marks.value_offset;
    let bar = rect(s.marks().next().unwrap());
    let Shape::Text { y, style, .. } = &s.part(Part::Value).next().unwrap().shape else {
        panic!()
    };
    // Vega rounds its middle-baseline offset to whole pixels.
    let centre = y - (0.30 * style.size).round();
    assert!(
        (bar.top - centre - offset).abs() < 0.01,
        "M1: value the tokens' offset above the bar"
    );
}

#[test]
fn m2_bars_start_at_zero_with_softened_ends() {
    let radius_token = engine().tokens().marks.bar_radius;
    assert!(radius_token > 0.0, "M2: bar ends are softened");
    let s = scene("column");
    let bars: Vec<skia_safe::Rect> = s.marks().map(rect).collect();
    let base = bars[0].bottom;
    assert!(
        bars.iter().all(|b| (b.bottom - base).abs() < 1e-3),
        "M2: one shared baseline"
    );
    // Heights proportional to values means the baseline is zero.
    let values = [412.0_f32, 431.0, 298.0, 447.0, 463.0, 489.0];
    let k = bars[0].height() / values[0];
    for (b, v) in bars.iter().zip(values) {
        assert!((b.height() - v * k).abs() < 0.01, "M2: bars start at zero");
    }
    for m in s.marks() {
        let Shape::Rect {
            radius, corners, ..
        } = m.shape
        else {
            panic!()
        };
        assert_eq!(
            (radius, corners),
            (radius_token, Corners::Top),
            "M2: the tokens' end radius"
        );
        assert_eq!(
            m.grow,
            Grow::FromY(base),
            "N1: bars grow from their baseline"
        );
    }
    let s = scene("bar");
    let lefts: Vec<f32> = s.marks().map(|m| rect(m).left).collect();
    assert!(
        lefts.iter().all(|l| (l - lefts[0]).abs() < 1e-3),
        "M2: horizontal bars share x = 0"
    );
}

#[test]
fn m4_lines_are_firm_with_points_and_areas_are_light_from_zero() {
    let marks = engine().tokens().marks.clone();
    let s = scene("line");
    let line = s
        .marks()
        .find(|m| matches!(m.shape, Shape::Line { .. }))
        .expect("M4: a line");
    let Shape::Line { width, .. } = line.shape else {
        panic!()
    };
    assert!(
        (width - marks.line_width).abs() < 1e-6,
        "M4: the tokens' line width"
    );
    assert_eq!(
        s.marks()
            .filter(|m| matches!(m.shape, Shape::Circle { .. }))
            .count(),
        12,
        "M4: a point per value"
    );
    let s = scene("area");
    let area = s
        .marks()
        .find(|m| matches!(m.shape, Shape::Area { .. }))
        .expect("M4: an area");
    assert!(marks.area_opacity < 0.5, "M4: areas are light");
    assert_eq!(
        f32::from(area.color.a),
        (marks.area_opacity * 255.0).round(),
        "M4: area at the tokens' opacity"
    );
    let zero = texts(&s, Part::AxisLabel).iter().any(|t| t == "0");
    assert!(zero, "M4: the area's axis starts at zero");
}

#[test]
fn ranked_forms_sort_largest_first_and_others_keep_data_order() {
    let s = scene("bar");
    assert_eq!(
        texts(&s, Part::AxisLabel),
        [
            "Northwind",
            "Globex",
            "Acme",
            "Umbrella",
            "Initech",
            "Hooli"
        ],
        "sort"
    );
    let s = scene("column");
    assert_eq!(
        texts(&s, Part::AxisLabel),
        ["W1", "W2", "W3", "W4", "W5", "W6"],
        "sort"
    );
}

#[test]
fn t2_t3_title_and_subtitle_are_set_as_the_playbook_says() {
    let e = engine();
    let t = &e.tokens().text;
    let s = scene("column");
    let title = s.part(Part::Title).next().unwrap();
    let Shape::Text { style, .. } = &title.shape else {
        panic!()
    };
    assert_eq!(
        (style.size, style.weight),
        (t.title_size, t.title_weight),
        "T2"
    );
    assert_eq!(
        texts(&s, Part::Subtitle),
        ["signups by week"],
        "T3: defaults to y by x"
    );
    let sub = s.part(Part::Subtitle).next().unwrap();
    assert_eq!(sub.emphasis, Emphasis::Muted, "T3");
}

#[test]
fn t4_labels_are_small_and_quiet_and_values_small_and_firm() {
    let e = engine();
    let t = &e.tokens().text;
    for form in ["column", "bar", "line", "lollipop"] {
        let s = scene(form);
        for label in s.part(Part::AxisLabel) {
            let Shape::Text { style, .. } = &label.shape else {
                panic!()
            };
            // A lollipop names its average rule in the rule's own grey.
            let quiet = if label.text().is_some_and(|t| t.starts_with("Average ")) {
                Emphasis::Neutral(0)
            } else {
                Emphasis::Muted
            };
            assert_eq!(
                (style.size, label.emphasis),
                (t.label_size, quiet),
                "T4 {form}: labels at the label size, muted"
            );
        }
        // Ranked rows set their values beside the mark at regular weight.
        let weight = if matches!(form, "bar" | "lollipop") {
            t.row_value_weight
        } else {
            t.value_weight
        };
        for value in s.part(Part::Value) {
            let Shape::Text { style, .. } = &value.shape else {
                panic!()
            };
            assert_eq!(
                (style.size, style.weight),
                (t.value_size, weight),
                "T4 {form}: values at the value size, weight {weight}"
            );
        }
    }
}

#[test]
fn t1_t6_text_uses_bundled_weights_and_values_use_tabular_figures_with_separators() {
    for form in tiles_engine::forms::EMBEDDED.iter().map(|(n, _)| *n) {
        for item in &scene(form).items {
            if let Shape::Text { style, .. } = &item.shape {
                assert!([400, 500, 600].contains(&style.drawn_weight()), "T1 {form}");
                if item.part == Part::Value {
                    assert!(style.tabular, "T6 {form}: values use tabular figures");
                }
            }
        }
    }
    assert!(
        texts(&scene("bar"), Part::Value).contains(&"412,500".to_string()),
        "T6"
    );
}

#[test]
fn c5_the_tile_snaps_to_whole_cells() {
    let intent = tiles_intent::parse(&fixture("column").to_string()).unwrap();
    let opts = Options {
        cell_height: Some(17.0),
        ..Options::default()
    };
    let e = engine();
    let m = &e.tokens().card;
    let s = e.scene(&intent, &opts).unwrap();
    assert!(
        (s.height / 17.0 - (s.height / 17.0).round()).abs() < 1e-3,
        "C5: {}",
        s.height
    );
    assert!(
        (s.height - s.card.bottom - m.margin_bottom).abs() < 1e-3,
        "C5: bottom margin"
    );
    assert!(
        (s.card.left - m.margin_x).abs() < 1e-6 && (s.card.top - m.margin_top).abs() < 1e-6,
        "C5: margins"
    );
}

#[test]
fn themes_change_colours_but_not_geometry() {
    let intent = tiles_intent::parse(&fixture("column").to_string()).unwrap();
    let light = engine()
        .scene(
            &intent,
            &Options {
                theme: light(),
                ..Options::default()
            },
        )
        .unwrap();
    let dark = engine()
        .scene(
            &intent,
            &Options {
                theme: dark(),
                ..Options::default()
            },
        )
        .unwrap();
    assert_eq!(light.items.len(), dark.items.len());
    for (a, b) in light.items.iter().zip(&dark.items) {
        assert_eq!(a.shape, b.shape);
        assert_eq!(a.emphasis, b.emphasis);
    }
    assert_ne!(light.items[0].color, dark.items[0].color);
}

#[test]
fn n1_the_entrance_fades_floats_and_ends_on_the_settled_card() {
    let e = engine();
    let intent = tiles_intent::parse(&fixture("column").to_string()).unwrap();
    let s = e.scene(&intent, &Options::default()).unwrap();
    let frames: Vec<_> = e
        .frames(&s, 1.0)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(frames.len(), 10, "N1: 10 frames, 50 ms apart over 450 ms");
    assert!(
        frames[0].rgba.chunks(4).all(|p| p[3] == 0),
        "N1: the first frame is fully faded"
    );
    let settled = e.frame_at(&s, 1.0, Moment::FINAL).unwrap();
    assert_eq!(frames[9], settled, "N1: the last frame is the settled card");
    // Mid-entrance the bars are shorter than settled: count accent-red pixels.
    let red = e.tokens().palette(s.theme).bad;
    let count = |rgba: &[u8]| {
        rgba.chunks(4)
            .filter(|p| p[0] == red.r && p[1] == red.g && p[2] == red.b && p[3] == 255)
            .count()
    };
    let mid = count(&frames[3].rgba);
    let end = count(&settled.rgba);
    assert!(mid < end && end > 0, "N1: marks grow ({mid} < {end})");
}

#[test]
fn the_hit_map_finds_each_bar_and_its_values() {
    let s = scene("column");
    let hits = hit_map(&s, 2.0);
    assert_eq!(hits.len(), 6);
    let w3 = rect(s.marks().nth(2).unwrap());
    #[allow(clippy::cast_possible_truncation)] // Test coordinates.
    let (x, y) = ((w3.center_x() * 2.0) as i32, (w3.center_y() * 2.0) as i32);
    let hit = hit_at(&hits, x, y).expect("the W3 bar is under its centre");
    assert_eq!(hit.tip, "week: W3\nsignups: 298");
    assert_eq!(hit.row, Some(2));
    assert!(hit_at(&hits, 1, 1).is_none(), "nothing in the margin");
    // A point on a line wins over the line around it.
    let s = scene("line");
    let hits = hit_map(&s, 1.0);
    assert_eq!(hits.len(), 12, "one target per point");
}

#[test]
fn a_spotlit_line_answers_hover_on_every_point_it_does_not_draw() {
    let v = gallery("multi-line");
    let rows = v["data"]["rows"].as_array().unwrap();
    let s = scene_of(&v).unwrap();
    assert_eq!(
        s.marks()
            .filter(|m| matches!(m.shape, Shape::Circle { .. }))
            .count(),
        0,
        "the gallery draws no points on a spotlit line"
    );
    let hits = hit_map(&s, 2.0);
    assert_eq!(hits.len(), rows.len(), "one hover target per point");
    let series = v["fields"]["series"].as_str().unwrap();
    for (r, row) in rows.iter().enumerate() {
        let hit = hits
            .iter()
            .find(|h| h.row == Some(r))
            .unwrap_or_else(|| panic!("row {r} has a hover target"));
        let name = row[series].as_str().unwrap();
        assert!(hit.tip.contains(name), "row {r}: {:?}", hit.tip);
        let (x, y) = (
            i32::midpoint(hit.left, hit.right),
            i32::midpoint(hit.top, hit.bottom),
        );
        assert!(hit_at(&hits, x, y).is_some(), "row {r}: hover finds it");
    }
}

#[test]
fn intent_rules_refuse_with_one_line_naming_the_fix() {
    let e = engine();
    let mut v = fixture("column");
    v["form"] = "pie".into();
    let err = scene_of(&v).unwrap_err().to_string();
    assert!(
        err.starts_with("form: \"pie\" is not a form; use one of: column, bar"),
        "{err}"
    );
    let mut v = fixture("column");
    v["highlight"] = serde_json::json!(["W9"]);
    assert!(
        scene_of(&v)
            .unwrap_err()
            .to_string()
            .starts_with("highlight[0]:")
    );
    let mut v = fixture("column");
    v["fields"] = serde_json::json!({"x": "week"});
    let err = scene_of(&v).unwrap_err().to_string();
    assert!(err.contains("needs role y"), "{err}");
    let mut v = fixture("column");
    v["fields"] = serde_json::json!({"x": "week", "y": "week"});
    assert!(scene_of(&v).unwrap_err().to_string().contains("holds text"));
    let mut v = fixture("column");
    v["fields"]["series"] = "week".into();
    assert!(
        scene_of(&v)
            .unwrap_err()
            .to_string()
            .contains("has no role series")
    );
    let rows: Vec<serde_json::Value> = (0..61)
        .map(|i| serde_json::json!({"week": format!("W{i}"), "signups": i}))
        .collect();
    let mut v = fixture("column");
    v["data"]["rows"] = rows.into();
    v.as_object_mut().unwrap().remove("highlight");
    assert!(scene_of(&v).unwrap_err().to_string().contains("categories"));
    let mut v = fixture("stacked-bar");
    v["data"]["rows"][0]["accounts"] = (-5).into();
    assert!(
        scene_of(&v)
            .unwrap_err()
            .to_string()
            .contains("zero or more")
    );
    assert!(!RenderError::Engine("x".into()).to_string().contains('\n'));
    let bad = Options {
        width: 10.0,
        ..Options::default()
    };
    let intent = tiles_intent::parse(&fixture("column").to_string()).unwrap();
    assert!(e.scene(&intent, &bad).is_err());
    let s = e.scene(&intent, &Options::default()).unwrap();
    assert!(e.png(&s, f32::NAN).is_err() && e.png(&s, 100.0).is_err());
}

/// Hostile and odd data: every form draws or refuses, never panics.
#[test]
fn odd_data_draws_or_refuses_without_panicking() {
    let e = engine();
    let long = "W".repeat(120);
    let cases: Vec<Vec<serde_json::Value>> = vec![
        vec![serde_json::json!({"k": "only", "v": 5})],
        vec![
            serde_json::json!({"k": "a", "v": 0}),
            serde_json::json!({"k": "b", "v": 0}),
        ],
        vec![
            serde_json::json!({"k": "a", "v": -1e300}),
            serde_json::json!({"k": "b", "v": 1e300}),
        ],
        vec![
            serde_json::json!({"k": long, "v": 1}),
            serde_json::json!({"k": "漢字 🚀", "v": 2}),
        ],
        vec![
            serde_json::json!({"k": "a", "v": null}),
            serde_json::json!({"k": "b", "v": 3}),
        ],
        (0..2000)
            .map(|i| serde_json::json!({"k": format!("P{i}"), "v": (i % 97)}))
            .collect(),
    ];
    for (name, _) in tiles_engine::forms::EMBEDDED {
        let form = e.form(name).unwrap();
        for rows in &cases {
            let mut fields = serde_json::Map::new();
            for r in &form.roles {
                let col = match r.kind {
                    tiles_engine::forms::RoleKind::Category => "k",
                    tiles_engine::forms::RoleKind::Number => "v",
                };
                fields.insert(r.name.clone(), col.into());
            }
            let v = serde_json::json!({
                "v": 1, "form": name, "title": "Odd data", "claim": {"kind": "rank", "tone": "neutral"},
                "fields": fields, "data": {"rows": rows}
            });
            if let Ok(intent) = tiles_intent::parse(&v.to_string())
                && let Ok(s) = e.scene(&intent, &Options::default())
            {
                assert!(s.height.is_finite() && s.height > 0.0, "{name}");
                e.png(&s, 1.0).unwrap_or_else(|err| panic!("{name}: {err}"));
            }
        }
    }
}

/// The directory that holds what this build sealed: `assets/plain` for a
/// build from source, which takes the tree's forms as it has no `forms/`,
/// and `assets` for a pack build while the house tokens are in the tree
/// (ADR-037). Loaded as it is, and again with the tree's forms beside its
/// tokens, it gives the embedded forms and themes.
#[test]
fn a_dev_assets_directory_loads_like_the_embedded_set() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let dir = if tiles_engine::SEAL_SOURCE == "assets" {
        assets.join("plain")
    } else if assets.join("tokens.json").is_file() {
        assets.clone()
    } else {
        return;
    };
    let with_forms = std::env::temp_dir().join(format!("tiles-dev-assets-{}", std::process::id()));
    std::fs::create_dir_all(with_forms.join("forms")).unwrap();
    std::fs::copy(dir.join("tokens.json"), with_forms.join("tokens.json")).unwrap();
    for entry in std::fs::read_dir(assets.join("forms")).unwrap() {
        let p = entry.unwrap().path();
        std::fs::copy(&p, with_forms.join("forms").join(p.file_name().unwrap())).unwrap();
    }
    let forms = |e: &Engine| {
        let mut names: Vec<String> = e.forms().iter().map(|f| f.name.clone()).collect();
        names.sort_unstable();
        names
    };
    let themes = |e: &Engine| {
        let t = &e.tokens().themes;
        let names: Vec<String> = t.all().map(|p| p.name.clone()).collect();
        (t.default.clone(), names)
    };
    let embedded = engine();
    for d in [&dir, &with_forms] {
        let dev = Engine::with_assets(Some(d)).unwrap();
        assert_eq!(forms(&dev), forms(&embedded), "{}", d.display());
        assert_eq!(themes(&dev), themes(&embedded), "{}", d.display());
    }
    std::fs::remove_dir_all(&with_forms).unwrap();
    assert!(Engine::with_assets(Some(&dir.join("missing"))).is_err());
}

/// A form ships whole: its file is embedded, and it has a fixture intent,
/// its light and dark golden images in the plain set and, while the house
/// tokens are in the tree, the house set (ADR-037), and a verify-tiles
/// feature file.
#[test]
fn every_form_file_is_embedded_with_its_fixture_goldens_and_feature_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut on_disk: Vec<String> = std::fs::read_dir(root.join("assets/forms"))
        .unwrap()
        .filter_map(|e| e.ok()?.path().file_stem()?.to_str().map(String::from))
        .collect();
    on_disk.sort();
    let mut embedded: Vec<String> = tiles_engine::forms::EMBEDDED
        .iter()
        .map(|(n, _)| (*n).to_string())
        .collect();
    embedded.sort();
    assert_eq!(
        on_disk, embedded,
        "a form file is missing from forms::EMBEDDED"
    );
    let skill = root.join("../../.agents/skills/verify-tiles/features");
    let mut sets = vec!["tests/golden-plain"];
    if root.join("assets/tokens.json").is_file() {
        sets.push("tests/golden");
    }
    for name in &embedded {
        assert!(
            root.join(format!("tests/intents/{name}.json")).is_file(),
            "{name}: no fixture intent"
        );
        for set in &sets {
            for theme in ["light", "dark"] {
                assert!(
                    root.join(format!("{set}/{name}-{theme}.png")).is_file(),
                    "{name}: no {theme} golden in {set}"
                );
            }
        }
        assert!(
            skill.join(format!("{name}.md")).is_file(),
            "{name}: no verify-tiles feature file"
        );
    }
}

/// The box a text item covers, from its anchor, width and the font's ascent
/// and descent.
fn text_box(e: &Engine, item: &Item) -> skia_safe::Rect {
    let Shape::Text {
        text,
        x,
        y,
        style,
        anchor,
    } = &item.shape
    else {
        panic!("not text")
    };
    let w = e.text_width(text, *style);
    let left = match anchor {
        tiles_engine::scene::Anchor::Start => *x,
        tiles_engine::scene::Anchor::Middle => x - w / 2.0,
        tiles_engine::scene::Anchor::End => x - w,
    };
    skia_safe::Rect::from_ltrb(left, y - 0.75 * style.size, left + w, y + 0.2 * style.size)
}

fn overlaps(a: &skia_safe::Rect, b: &skia_safe::Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/// Every worst case in `tests/intents/worst-case/` that draws keeps its
/// text on the card, and no two of its axis labels touch.
#[test]
fn worst_case_text_stays_on_the_card_and_axis_labels_apart() {
    let e = engine();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/intents/worst-case");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let v = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let Ok(s) = scene_of(&v) else {
            continue;
        };
        let name = path.file_stem().unwrap().to_string_lossy();
        for item in s.items.iter().filter(|i| i.text().is_some()) {
            let b = text_box(&e, item);
            assert!(
                b.left >= s.card.left - 0.5 && b.right <= s.card.right + 0.5,
                "{name}: {:?} runs off the card",
                item.text()
            );
        }
        let axis: Vec<skia_safe::Rect> = s.part(Part::AxisLabel).map(|i| text_box(&e, i)).collect();
        for (i, a) in axis.iter().enumerate() {
            for b in &axis[i + 1..] {
                assert!(!overlaps(a, b), "{name}: two axis labels touch");
            }
        }
    }
}

/// A label too long for its room keeps the end it starts reading from: a
/// left-to-right name its left with the ellipsis after it, a right-to-left
/// name its right with the ellipsis before it, where the cut lands.
#[test]
fn a_cut_label_keeps_the_end_it_starts_reading_from() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/intents/worst-case/multi-line-rtl.json");
    let v = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let s = scene_of(&v).unwrap();
    let cut: Vec<String> = s
        .items
        .iter()
        .filter_map(|i| i.text().filter(|t| t.contains('…')).map(String::from))
        .collect();
    let arabic = cut
        .iter()
        .find(|t| t.contains('ع'))
        .expect("the Arabic name is cut");
    assert!(arabic.starts_with("…عبد"), "{arabic:?}");
    let latin = cut
        .iter()
        .find(|t| t.contains('A'))
        .expect("the Latin name is cut");
    assert!(
        latin.starts_with("Aleksandra") && latin.ends_with('…'),
        "{latin:?}"
    );
}

/// Two right-to-left names that cut alike each show the part where they
/// differ. An ellipsis on one side of that part marks a cut at the other
/// logical end, so it draws on the cut's side, as in a single cut name.
#[test]
fn right_to_left_names_that_cut_alike_keep_each_ellipsis_at_its_cut() {
    let pairs = [
        [
            "عبد الرحمن بن نور الهدى الإسكندراني",
            "عبد الرحمن بن نور الهدى القاسمي",
        ],
        [
            "عبد الرحمن بن نور الهدى الإسكندراني الكبير",
            "عبد الرحمن بن نور الهدى والإسكندراني الكبير",
        ],
        [
            "عبدالرحمنبننورالهدىالإسكندراني",
            "عبدالرحمنبننورالهدىالإسكندرانية",
        ],
    ];
    for names in pairs {
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("intents/worst-case/multi-line-rtl.json")).unwrap();
        for (i, row) in v["data"]["rows"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            row["who"] = names[i / 2].into();
        }
        let s = scene_of(&v).unwrap();
        let cut: Vec<String> = s
            .items
            .iter()
            .filter_map(|i| i.text().filter(|t| t.contains('…')).map(String::from))
            .collect();
        assert_eq!(cut.len(), 2, "{cut:?}");
        assert_ne!(cut[0], cut[1]);
        for t in &cut {
            let name = names
                .iter()
                .find(|n| t.split('…').all(|part| n.contains(part.trim())))
                .unwrap_or_else(|| panic!("{t:?} is from neither name"));
            if t.matches('…').count() != 1 {
                continue;
            }
            let kept = t.trim_matches('…');
            if t.starts_with('…') {
                assert!(
                    name.starts_with(kept),
                    "{t:?} has its ellipsis away from its cut"
                );
            } else if t.ends_with('…') {
                assert!(
                    name.ends_with(kept),
                    "{t:?} has its ellipsis away from its cut"
                );
            }
        }
    }
}

#[test]
fn value_labels_never_collide_with_axis_labels() {
    let e = engine();
    for form in tiles_engine::forms::EMBEDDED.iter().map(|(n, _)| *n) {
        for s in [scene(form), without_highlight(form)] {
            let axis: Vec<skia_safe::Rect> =
                s.part(Part::AxisLabel).map(|i| text_box(&e, i)).collect();
            for v in s.part(Part::Value) {
                let b = text_box(&e, v);
                for a in &axis {
                    assert!(
                        !overlaps(&b, a),
                        "{form}: value {:?} collides with an axis label",
                        v.text()
                    );
                }
            }
        }
    }
}

#[test]
fn scatter_labels_stay_inside_the_plot_and_points_off_its_edge() {
    let e = engine();
    let s = scene("scatter");
    let labels: Vec<&Item> = s.part(Part::Value).collect();
    assert_eq!(labels.len(), 1, "only the named point is labelled");
    let b = text_box(&e, labels[0]);
    let p = s.plot;
    assert!(
        b.left >= p.left && b.right <= p.right && b.top >= p.top && b.bottom <= p.bottom,
        "the Globex label {b:?} leaves the plot {:?}",
        s.plot
    );
    for m in s.marks() {
        let mb = m.bounds();
        assert!(
            mb.bottom < s.plot.bottom - 1.0 && mb.top > s.plot.top + 1.0,
            "a point sits on the plot edge"
        );
    }
}

#[test]
fn k6_unnamed_series_take_distinct_ordered_neutrals_matching_the_legend() {
    let s = scene("stacked-bar");
    let swatches: Vec<_> = s
        .part(Part::Legend)
        .filter(|i| matches!(i.shape, Shape::Circle { .. }))
        .map(|i| (i.emphasis, i.color))
        .collect();
    assert_eq!(
        swatches.iter().map(|s| s.0).collect::<Vec<_>>(),
        [Emphasis::Neutral(0), Emphasis::Neutral(1), Emphasis::Accent],
        "K6: Starter, Growth, then the named Enterprise"
    );
    assert_ne!(
        swatches[0].1, swatches[1].1,
        "K6: unnamed series are told apart"
    );
    for m in s.marks() {
        assert!(
            swatches.contains(&(m.emphasis, m.color)),
            "K6: every segment matches a legend swatch"
        );
    }
}

#[test]
fn dumbbell_reads_first_to_second_with_the_accent_on_the_second() {
    let s = scene("dumbbell");
    let legend = texts(&s, Part::Legend);
    assert_eq!(
        legend,
        ["before", "after"],
        "the legend reads in the order of the change"
    );
    // Billing is first (largest change); its second dot (after, 18) takes the accent.
    let billing: Vec<&Item> = s.marks().filter(|m| m.datum == Some(0)).collect();
    assert_eq!(billing.len(), 2);
    assert_eq!(
        billing[0].emphasis,
        Emphasis::Grey,
        "the first value is grey"
    );
    assert_eq!(
        billing[1].emphasis,
        Emphasis::Accent,
        "the second value is the finding"
    );
    // The periods are named once, each over its own dot of the named row.
    let names: Vec<&Item> = s.part(Part::Legend).collect();
    let row: Vec<&Item> = s.marks().filter(|m| m.datum == Some(0)).collect();
    for (name, dot) in names.iter().zip(&row) {
        let (Shape::Text { x, .. }, Shape::Circle { cx, .. }) = (&name.shape, &dot.shape) else {
            panic!("a period name over a dot")
        };
        assert!((x - cx).abs() < 0.01, "{:?} sits over its dot", name.text());
    }
    assert_eq!(
        s.part(Part::Value).count(),
        0,
        "the axis carries the values"
    );
}

fn intent(form: &str, fields: &serde_json::Value, rows: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "v": 1, "form": form, "title": "Odd data", "claim": {"kind": "rank", "tone": "neutral"},
        "fields": fields, "data": {"rows": rows}
    })
}

#[test]
fn repeated_categories_draw_one_row_each_with_their_values_summed() {
    let queues = ["Billing", "Tech", "Sales", "Account", "Security"];
    let rows: Vec<serde_json::Value> = (0..2000)
        .map(|i| serde_json::json!({"k": queues[i % 5], "a": 1, "b": 2}))
        .collect();
    for (form, fields) in [
        (
            "dumbbell",
            serde_json::json!({"x": "k", "lo": "a", "hi": "b"}),
        ),
        (
            "bullet",
            serde_json::json!({"x": "k", "y": "a", "target": "b"}),
        ),
    ] {
        let s = scene_of(&intent(form, &fields, &rows)).unwrap();
        // Row labels, without the value axis's ticks.
        let mut labels = texts(&s, Part::AxisLabel);
        labels.retain(|l| !l.starts_with(|c: char| c.is_ascii_digit()));
        labels.sort();
        let mut want: Vec<String> = queues.iter().map(|q| (*q).to_string()).collect();
        want.sort();
        assert_eq!(labels, want, "{form}: one row per category");
        assert!(s.height < 600.0, "{form}: {} px tall", s.height);
        let tips: Vec<String> = s.marks().filter_map(|m| m.tip.clone()).collect();
        assert!(
            tips.iter().any(|t| t.contains("a: 400")),
            "{form}: 400 rows of 1 sum to 400: {tips:?}"
        );
    }
}

#[test]
fn a_highlight_the_form_cannot_colour_is_refused_but_a_big_number_ignores_it() {
    for (form, value) in [("stacked-bar", "Q3"), ("slope", "2025")] {
        let mut v = fixture(form);
        v["highlight"] = serde_json::json!([value]);
        let err = scene_of(&v).unwrap_err().to_string();
        assert!(err.starts_with("highlight[0]:"), "{form}: {err}");
        assert!(!err.contains('\n'));
    }
    let mut v = fixture("big-number");
    let plain = scene_of(&v).unwrap();
    let x = v["fields"]["x"].as_str().unwrap().to_string();
    v["highlight"] = serde_json::json!([v["data"]["rows"][0][&x].clone()]);
    let lit = scene_of(&v).expect("a big number ignores a highlight rather than refusing it");
    assert_eq!(texts(&lit, Part::Value), texts(&plain, Part::Value));
    let mut v = fixture("grouped-bar");
    let x = v["fields"]["x"].as_str().unwrap().to_string();
    v["highlight"] = serde_json::json!([v["data"]["rows"][0][&x].clone()]);
    assert!(
        scene_of(&v).is_ok(),
        "grouped-bar greys the other categories"
    );
}

/// Rows for `n` series over two periods, in the columns q, s and v.
fn series_rows(n: usize) -> Vec<serde_json::Value> {
    (0..n)
        .flat_map(|i| {
            ["Q1", "Q2"].map(|q| serde_json::json!({"q": q, "s": format!("S{i}"), "v": i + 1}))
        })
        .collect()
}

#[test]
fn more_series_than_the_palette_can_tell_apart_are_refused_in_every_theme() {
    let e = engine();
    let limit = e.tokens().palette(Theme::default()).series;
    let shortest = e.tokens().themes.all().map(|p| p.categorical.len()).min();
    assert_eq!(
        Some(limit),
        shortest,
        "ADR-021: the shortest palette sets the limit"
    );
    let fields = serde_json::json!({"x": "q", "y": "v", "series": "s"});
    let draw = |v: &serde_json::Value, theme: Theme| {
        let intent = tiles_intent::parse(&v.to_string())?;
        e.scene(
            &intent,
            &Options {
                theme,
                ..Options::default()
            },
        )
    };
    for &theme in Theme::all() {
        for form in ["grouped-bar", "stacked-bar", "multi-line", "slope"] {
            let s = draw(&intent(form, &fields, &series_rows(limit)), theme)
                .unwrap_or_else(|e| panic!("{form} at {limit} series in {theme:?}: {e}"));
            let mut legend: Vec<Emphasis> = s.marks().map(|m| m.emphasis).collect();
            legend.sort_by_key(|e| format!("{e:?}"));
            legend.dedup();
            assert_eq!(
                legend.len(),
                limit,
                "{form} in {theme:?}: each series has its own colour"
            );
            let err = draw(&intent(form, &fields, &series_rows(limit + 1)), theme)
                .unwrap_err()
                .to_string();
            assert_eq!(
                err,
                format!(
                    "fields.series: {} distinct values, at most {limit} can be told apart; \
                     keep the top {} and group the rest as Other, or pick another form",
                    limit + 1,
                    limit - 1
                ),
                "{form} in {theme:?}"
            );
        }
    }
}

#[test]
fn k1_a_spotlight_on_one_line_draws_the_rest_in_ordered_context_greys_at_any_count() {
    let fields = serde_json::json!({"x": "q", "y": "v", "series": "s"});
    let e = engine();
    for form in ["multi-line", "slope"] {
        let mut v = intent(form, &fields, &series_rows(9));
        v["highlight"] = serde_json::json!(["S3"]);
        let s = scene_of(&v).unwrap_or_else(|e| panic!("{form}: {e}"));
        let context = &e.tokens().palette(s.theme).context;
        let last = context.len() - 1;
        for m in s.marks() {
            let Some(row) = m.datum else { continue };
            let name = v["data"]["rows"][row]["s"].as_str().unwrap();
            let i: usize = name[1..].parse().unwrap();
            let want = match i {
                3 => Emphasis::Accent,
                // The unnamed lines in series order: S0, S1, S2, then S4 on.
                _ => Emphasis::Context((if i < 3 { i } else { i - 1 }).min(last)),
            };
            assert_eq!(
                m.emphasis, want,
                "K1 {form}: {name} takes the accent if named, else its context grey"
            );
        }
    }
    let err = scene_of(&intent("multi-line", &fields, &series_rows(9)))
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("fields.series: 9 distinct values, at most"),
        "{err}"
    );
}

#[test]
fn unnamed_stacks_and_groups_beside_a_highlight_stay_apart_or_are_refused() {
    let neutrals = engine().tokens().palette(Theme::default()).neutrals.len();
    let fields = serde_json::json!({"x": "q", "y": "v", "series": "s"});
    for form in ["grouped-bar", "stacked-bar"] {
        let mut v = intent(form, &fields, &series_rows(neutrals + 1));
        v["highlight"] = serde_json::json!(["S0"]);
        let s = scene_of(&v).unwrap_or_else(|e| panic!("{form}: {e}"));
        let mut unnamed: Vec<Emphasis> = s
            .marks()
            .filter(|m| !m.highlighted)
            .map(|m| m.emphasis)
            .collect();
        unnamed.sort_by_key(|e| format!("{e:?}"));
        unnamed.dedup();
        assert_eq!(
            unnamed.len(),
            neutrals,
            "{form}: each unnamed series has its own neutral"
        );
        let mut v = intent(form, &fields, &series_rows(neutrals + 2));
        v["highlight"] = serde_json::json!(["S0"]);
        assert_eq!(
            scene_of(&v).unwrap_err().to_string(),
            format!(
                "fields.series: {} series outside the highlight, at most {neutrals} can be told \
                 apart; keep the top {} beside the highlight and group the rest as Other, or \
                 pick another form",
                neutrals + 1,
                neutrals - 1
            ),
            "{form}"
        );
    }
}

#[test]
fn long_labels_are_cut_short_and_legends_wrap_inside_the_card() {
    let e = engine();
    let long =
        "Spring campaign for returning merchants in the Benelux region, paid search and social"
            .repeat(2);
    let long = &long[..120];
    let inside = |s: &Scene, form: &str| {
        for part in [Part::AxisLabel, Part::Legend, Part::Value] {
            for t in s.part(part).filter(|t| t.text().is_some()) {
                let b = text_box(&e, t);
                assert!(
                    b.left >= s.card.left && b.right <= s.card.right,
                    "{form}: {:?} leaves the card",
                    t.text()
                );
            }
        }
        assert!(
            s.plot.width() > 150.0,
            "{form}: the plot is {} px wide",
            s.plot.width()
        );
    };
    for form in [
        "bar", "lollipop", "dot-plot", "dumbbell", "bullet", "column",
    ] {
        let mut v = fixture(form);
        v.as_object_mut().unwrap().remove("highlight");
        let x = v["fields"]["x"].as_str().unwrap().to_string();
        v["data"]["rows"][0][&x] = long.into();
        let s = scene_of(&v).unwrap();
        assert!(
            texts(&s, Part::AxisLabel).iter().any(|l| l.ends_with('…')),
            "{form}: the long label is cut short"
        );
        if form != "column" {
            inside(&s, form);
        }
    }
    let mut v = fixture("slope");
    let series = v["fields"]["series"].as_str().unwrap().to_string();
    for row in v["data"]["rows"].as_array_mut().unwrap() {
        if row[&series] == "Search" {
            row[&series] = long.into();
        }
    }
    v["highlight"] = serde_json::json!([long]);
    let s = scene_of(&v).unwrap();
    assert!(
        s.marks().any(|m| m.highlighted),
        "slope: the long series is named"
    );
    inside(&s, "slope");
    let rows: Vec<serde_json::Value> = (0..5)
        .flat_map(|i| {
            ["Q1", "Q2"].map(|q| serde_json::json!({"q": q, "s": format!("A rather long series name {i}"), "v": i + 1}))
        })
        .collect();
    let s = scene_of(&intent(
        "grouped-bar",
        &serde_json::json!({"x": "q", "y": "v", "series": "s"}),
        &rows,
    ))
    .unwrap();
    inside(&s, "grouped-bar");
    let mut rows: Vec<i32> = s
        .part(Part::Legend)
        .filter_map(|i| match i.shape {
            #[allow(clippy::cast_possible_truncation)] // Test coordinates.
            Shape::Text { y, .. } => Some(y.round() as i32),
            _ => None,
        })
        .collect();
    rows.dedup();
    assert!(rows.len() > 1, "the legend wraps onto more rows: {rows:?}");
    let legend_bottom = s
        .part(Part::Legend)
        .map(|i| i.bounds().bottom)
        .fold(0.0, f32::max);
    assert!(
        s.marks().all(|m| m.bounds().top > legend_bottom),
        "the plot starts below the wrapped legend"
    );
}

#[test]
fn forms_refuse_when_no_complete_row_or_pair_remains() {
    let cases = [
        intent(
            "scatter",
            &serde_json::json!({"x": "a", "y": "b"}),
            &[
                serde_json::json!({"a": 1, "b": null}),
                serde_json::json!({"a": null, "b": 2}),
            ],
        ),
        intent(
            "dumbbell",
            &serde_json::json!({"x": "k", "lo": "a", "hi": "b"}),
            &[
                serde_json::json!({"k": "x", "a": 1, "b": null}),
                serde_json::json!({"k": "y", "a": null, "b": 2}),
            ],
        ),
        intent(
            "slope",
            &serde_json::json!({"x": "p", "y": "v", "series": "s"}),
            &[
                serde_json::json!({"p": "2025", "s": "Search", "v": 1}),
                serde_json::json!({"p": "2026", "s": "Email", "v": 2}),
            ],
        ),
        intent(
            "slope",
            &serde_json::json!({"x": "p", "y": "v", "series": "s"}),
            &[serde_json::json!({"p": "2025", "s": "Search", "v": 1})],
        ),
    ];
    for v in &cases {
        let err = scene_of(v).unwrap_err().to_string();
        assert!(err.starts_with("data.rows:"), "{}: {err}", v["form"]);
        assert!(!err.contains('\n'));
    }
}

/// A small seeded xorshift generator, so the label property runs the same
/// cases every time without a new dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }

    /// Words from a campaign-like vocabulary up to about `least` plus a
    /// random share of `spread` characters.
    fn words(&mut self, least: usize, spread: usize) -> String {
        const VOCAB: &[&str] = &[
            "2026",
            "Q3",
            "Spring",
            "campaign",
            "NL",
            "returning",
            "-",
            "Search",
            "Social",
            "all",
            "devices",
            "and",
            "platforms",
            "worldwide",
            "email",
            "brand",
            "promo",
            "retarget",
            "EU",
            "merchants",
            "paid",
            "organic",
        ];
        let chars = least + self.below(spread);
        let mut out = String::new();
        while out.len() < chars {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(VOCAB[self.below(VOCAB.len())]);
        }
        out
    }
}

/// Distinct label sets sharing a long start, a long end, or both, led by
/// the two look-alike pairs from review.
fn label_sets() -> Vec<Vec<String>> {
    let mut sets: Vec<Vec<String>> = vec![
        vec![
            "2026 Q3 Spring campaign NL returning - Search".into(),
            "2026 Q3 Spring campaign NL returning - Social".into(),
        ],
        vec![
            "2026 Q3 Spring campaign NL returning - Search - all devices and platforms worldwide"
                .into(),
            "2026 Q3 Spring campaign NL returning - Social - all devices and platforms worldwide"
                .into(),
        ],
    ];
    let mut rng = Rng(0x7469_6c65_7321);
    for case in 0..24 {
        let prefix = if case % 3 == 1 {
            String::new()
        } else {
            rng.words(20, 40)
        };
        let suffix = if case % 3 == 0 {
            String::new()
        } else {
            rng.words(20, 30)
        };
        let mut set: Vec<String> = Vec::new();
        let size = 2 + rng.below(11);
        while set.len() < size {
            let middle = rng.words(1, 16);
            let label: String = [prefix.as_str(), middle.as_str(), suffix.as_str()]
                .iter()
                .filter(|p| !p.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(120)
                .collect();
            if !set.contains(&label) {
                set.push(label);
            }
        }
        sets.push(set);
    }
    sets
}

#[test]
fn different_categories_never_print_the_same_label() {
    let e = engine();
    let card = &e.tokens().card;
    let colours = e.tokens().palette(Theme::default()).series;
    for (case, labels) in label_sets().iter().enumerate() {
        let bar: Vec<serde_json::Value> = labels
            .iter()
            .enumerate()
            .map(|(i, l)| serde_json::json!({"k": l, "v": i + 1}))
            .collect();
        let grouped: Vec<serde_json::Value> = labels
            .iter()
            .take(colours)
            .flat_map(|l| ["Q1", "Q2"].map(|q| serde_json::json!({"q": q, "s": l, "v": 3})))
            .collect();
        let charts = [
            (
                intent("bar", &serde_json::json!({"x": "k", "y": "v"}), &bar),
                Part::AxisLabel,
                labels.len(),
            ),
            (
                intent(
                    "grouped-bar",
                    &serde_json::json!({"x": "q", "y": "v", "series": "s"}),
                    &grouped,
                ),
                Part::Legend,
                labels.len().min(colours),
            ),
        ];
        for width in [320.0, 640.0, 1100.0] {
            for (v, part, shown) in &charts {
                let parsed = tiles_intent::parse(&v.to_string()).unwrap();
                let opts = Options {
                    width,
                    ..Options::default()
                };
                let s = e.scene(&parsed, &opts).unwrap();
                let max = 0.35 * (s.card.width() - 2.0 * (card.border + card.padding));
                let printed: Vec<&Item> = s
                    .part(*part)
                    .filter(|t| t.text().is_some_and(|l| !["Q1", "Q2"].contains(&l)))
                    .collect();
                let texts: Vec<&str> = printed.iter().filter_map(|t| t.text()).collect();
                let at = format!("case {case}, {} at {width} px: {texts:?}", v["form"]);
                assert_eq!(texts.len(), *shown, "{at}");
                let distinct: std::collections::HashSet<&&str> = texts.iter().collect();
                assert_eq!(distinct.len(), texts.len(), "{at}: two labels print alike");
                for item in &printed {
                    assert!(
                        text_box(&e, item).width() <= max + 0.01,
                        "{at}: {:?} is wider than {max} px",
                        item.text()
                    );
                }
                if case < 2 {
                    for word in ["Search", "Social"] {
                        assert!(
                            texts.iter().any(|t| t.contains(word)),
                            "{at}: the differing word {word} shows"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn several_named_series_cannot_share_the_accent() {
    let fields = serde_json::json!({"x": "q", "y": "v", "series": "s"});
    for form in ["grouped-bar", "stacked-bar", "multi-line", "slope"] {
        let mut v = intent(form, &fields, &series_rows(4));
        v["highlight"] = serde_json::json!(["S0", "S1"]);
        assert_eq!(
            scene_of(&v).unwrap_err().to_string(),
            format!(
                "highlight: 2 series named on {form}, only 1 can take the accent; name one, or \
                 drop highlight to give each series its own colour"
            ),
        );
    }
}

/// Every series' colour as drawn: the legend's swatches, or each line's
/// colour where a form has no legend.
fn drawn_series(s: &Scene, v: &serde_json::Value) -> Vec<(String, tiles_engine::color::Rgba)> {
    let swatches: Vec<_> = s
        .part(Part::Legend)
        .filter(|i| matches!(i.shape, Shape::Circle { .. }))
        .map(|i| i.color)
        .collect();
    if swatches.is_empty() {
        s.marks()
            .filter(|m| matches!(m.shape, Shape::Line { .. }))
            .filter_map(|m| {
                let row = m.datum?;
                Some((v["data"]["rows"][row]["s"].as_str()?.to_string(), m.color))
            })
            .collect()
    } else {
        texts(s, Part::Legend).into_iter().zip(swatches).collect()
    }
}

#[test]
fn only_the_rest_group_ever_shares_a_series_colour() {
    let e = engine();
    let mut rng = Rng(0x7365_7269_6573);
    let forms = ["grouped-bar", "stacked-bar", "multi-line", "slope"];
    let fields = serde_json::json!({"x": "q", "y": "v", "series": "s"});
    let (mut drawn, mut refused) = (0, 0);
    for case in 0..200 {
        let form = forms[rng.below(forms.len())];
        let n = 1 + rng.below(10);
        let mut v = intent(form, &fields, &series_rows(n));
        let mut named: Vec<String> = Vec::new();
        for _ in 0..rng.below(4) {
            let pick = if rng.below(6) == 0 {
                "Q1".to_string()
            } else {
                format!("S{}", rng.below(n))
            };
            if !named.contains(&pick) {
                named.push(pick);
            }
        }
        v["highlight"] = serde_json::json!(named);
        v["claim"]["tone"] = ["good", "bad", "neutral"][rng.below(3)].into();
        let theme = [light(), dark()][rng.below(2)];
        let at = format!("case {case}: {form}, {n} series, highlight {named:?}");
        let parsed = tiles_intent::parse(&v.to_string()).unwrap();
        let opts = Options {
            theme,
            ..Options::default()
        };
        match e.scene(&parsed, &opts) {
            Ok(s) => {
                drawn += 1;
                let quietest = *e.tokens().palette(theme).context.last().unwrap();
                let series = drawn_series(&s, &v);
                assert!(!series.is_empty(), "{at}: no series drawn");
                for (i, (a, ca)) in series.iter().enumerate() {
                    for (b, cb) in &series[i + 1..] {
                        if a != b && ca == cb {
                            assert!(
                                *ca == quietest
                                    && ["multi-line", "slope"].contains(&form)
                                    && !named.contains(a)
                                    && !named.contains(b),
                                "{at}: {a} and {b} share a colour outside the rest group"
                            );
                        }
                    }
                }
            }
            Err(err) => {
                refused += 1;
                let err = err.to_string();
                let key = err.split_once(": ").map_or("", |(k, _)| k);
                assert!(
                    key == "fields.series" || key == "highlight" || key.starts_with("highlight["),
                    "{at}: {err} names no key"
                );
                assert!(!err.contains('\n'), "{at}");
            }
        }
    }
    assert!(
        drawn > 20 && refused > 20,
        "{drawn} drawn, {refused} refused"
    );
}

/// A gallery case's intent: a gallery card's own rows, in
/// `tests/intents/gallery/` (ADR-059).
fn gallery(case: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/intents/gallery")
        .join(format!("{case}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn t7_numbers_carry_the_measures_unit() {
    let s = scene_of(&gallery("slope")).unwrap();
    let values = texts(&s, Part::Value);
    assert!(
        values.contains(&"Partner  31%".to_string()),
        "T7: {values:?}"
    );
    assert!(values.contains(&"34%".to_string()), "T7: {values:?}");
    let s = scene_of(&gallery("waterfall")).unwrap();
    let values = texts(&s, Part::Value);
    assert!(values.contains(&"+€1.1M".to_string()), "T7: {values:?}");
    assert!(
        values.contains(&"\u{2212}€0.2M".to_string()),
        "T7: {values:?}"
    );
    let s = scene_of(&gallery("multi-line")).unwrap();
    let ticks = texts(&s, Part::AxisLabel);
    assert!(
        ticks.contains(&"40%".to_string()),
        "T7: axis ticks carry it, {ticks:?}"
    );
    let s = scene_of(&gallery("column-highlight")).unwrap();
    let ticks = texts(&s, Part::AxisLabel);
    assert!(
        ticks.contains(&"3k".to_string()) && ticks.contains(&"0".to_string()),
        "T7: whole-thousand ticks read compact, {ticks:?}"
    );
    // An unknown unit draws the values plain, and the intent says so.
    let mut v = fixture("column");
    v["unit"] = "yen".into();
    let read = tiles_intent::parse(&v.to_string()).unwrap();
    assert_eq!(read.unit, None, "T7: an unknown unit is left out");
    assert!(read.notes[0].contains("\"yen\" is not one it writes"));
    let mut v = fixture("column");
    v["unit"] = "$".into();
    let read = tiles_intent::parse(&v.to_string()).unwrap();
    assert_eq!(
        read.unit,
        Some(tiles_intent::Unit::Usd),
        "T7: $ reads as usd"
    );
    assert_eq!(read.notes, ["Tiles read unit \"$\" as usd."]);
    // A scatter's unit is its y measure's; seats stay a plain count.
    let mut v = fixture("scatter");
    v["unit"] = "percent".into();
    let s = scene_of(&v).unwrap();
    let globex = s
        .marks()
        .find(|m| m.highlighted)
        .and_then(|m| m.tip.clone())
        .expect("T7: Globex has a tooltip");
    assert_eq!(
        globex, "seats: 410\nfeatures: 6%\naccount: Globex",
        "T7: the tooltip's y carries the unit"
    );
    let ticks = texts(&s, Part::AxisLabel);
    assert!(
        ticks.iter().any(|t| t.ends_with('%')) && ticks.iter().any(|t| !t.ends_with('%')),
        "T7: y ticks carry it, x ticks do not, {ticks:?}"
    );
    // A big number in a share carries it, and writes only the number, as
    // the gallery's big number does (ADR-015).
    let rows: Vec<serde_json::Value> = [("Q1", 28), ("Q2", 30), ("Q3", 31)]
        .iter()
        .map(|(q, v)| serde_json::json!({"q": q, "v": v}))
        .collect();
    let mut v = intent(
        "big-number",
        &serde_json::json!({"x": "q", "y": "v"}),
        &rows,
    );
    v["unit"] = "percent".into();
    let s = scene_of(&v).unwrap();
    assert_eq!(texts(&s, Part::Value), ["31%"], "T7");
    assert_eq!(
        texts(&s, Part::AxisLabel),
        ["Q1"],
        "T7: the sparkline names where it starts"
    );
}

#[test]
fn a_monthly_big_number_names_its_start_as_months_ago() {
    let rows: Vec<serde_json::Value> = (1..=12)
        .map(|m| serde_json::json!({"m": format!("2026-{m:02}"), "v": 100 + m}))
        .collect();
    let v = intent(
        "big-number",
        &serde_json::json!({"x": "m", "y": "v"}),
        &rows,
    );
    let s = scene_of(&v).unwrap();
    assert_eq!(texts(&s, Part::AxisLabel), ["11 months ago"]);
    let start = s.part(Part::AxisLabel).next().unwrap();
    let line = s
        .marks()
        .find_map(|m| match &m.shape {
            Shape::Line { points, .. } => Some(points.clone()),
            _ => None,
        })
        .unwrap();
    let Shape::Text { x, y, .. } = &start.shape else {
        panic!()
    };
    assert!(
        (x - line[0].0).abs() < 0.01,
        "it starts where the line does"
    );
    assert!(*y > line[0].1, "it sits under the line's first point");
    assert!(
        line.iter().all(|p| p.0 < s.plot.right),
        "the last point stops short of the edge"
    );
}

#[test]
fn t8_monthly_periods_read_as_a_thinned_time_axis() {
    let s = scene_of(&gallery("multi-line")).unwrap();
    let months: Vec<String> = texts(&s, Part::AxisLabel)
        .into_iter()
        .filter(|t| !t.ends_with('%'))
        .collect();
    assert_eq!(months, ["Oct 25", "Jan 26", "Apr 26", "Jul 26"], "T8");
    // Month names stay as written.
    let s = scene("line");
    assert!(
        texts(&s, Part::AxisLabel).contains(&"Jun".to_string()),
        "T8"
    );
    // Columns keep one label per band, relabelled but never thinned.
    let rows: Vec<serde_json::Value> = (1..=6)
        .map(|m| serde_json::json!({"m": format!("2026-{m:02}"), "v": 100 + m}))
        .collect();
    let v = intent("column", &serde_json::json!({"x": "m", "y": "v"}), &rows);
    let months = texts(&scene_of(&v).unwrap(), Part::AxisLabel);
    assert_eq!(
        months,
        ["Jan 26", "Feb 26", "Mar 26", "Apr 26", "May 26", "Jun 26"],
        "T8: a column per month keeps its label"
    );
}

#[test]
fn negative_ranked_values_stay_on_the_card() {
    let e = engine();
    for values in [[-50, -120, -300], [-900, -500, 20]] {
        let rows: Vec<serde_json::Value> = values
            .iter()
            .enumerate()
            .map(|(i, v)| serde_json::json!({"k": format!("Row {i}"), "v": v}))
            .collect();
        for form in ["bar", "lollipop"] {
            let v = intent(form, &serde_json::json!({"x": "k", "y": "v"}), &rows);
            let s = scene_of(&v).unwrap();
            for t in s.part(Part::Value) {
                let b = text_box(&e, t);
                assert!(
                    b.right <= s.card.right,
                    "{form} {values:?}: {:?} ends at {} past the card's {}",
                    t.text(),
                    b.right,
                    s.card.right
                );
            }
        }
    }
}

#[test]
fn m7_a_long_run_of_columns_gets_an_axis_and_writes_only_named_values() {
    let s = scene_of(&gallery("column-highlight")).unwrap();
    assert!(s.y_axis, "M7: twelve columns get a y axis");
    let values = texts(&s, Part::Value);
    assert_eq!(
        values,
        ["3,420", "Checkout bug, 4 to 9 Nov"],
        "M7: only the named value, and the note"
    );
    let s = scene("column");
    assert!(!s.y_axis, "M7: six columns carry their values instead");
    assert_eq!(texts(&s, Part::Value).len(), 6, "M7");
}

#[test]
fn m4_m8_a_named_slope_line_leads_by_weight_as_a_named_multi_line_does() {
    let marks = engine().tokens().marks.clone();
    let s = scene("slope");
    let (mut lines, mut dots) = (0, 0);
    for m in s.marks() {
        match m.shape {
            Shape::Line { width, .. } => {
                lines += 1;
                let want = if m.highlighted {
                    marks.line_width_named
                } else {
                    marks.line_width_rest
                };
                assert!((width - want).abs() < 1e-6, "M4 slope: line width {width}");
            }
            Shape::Circle { r, .. } => {
                dots += 1;
                let want = if m.highlighted {
                    marks.point_radius_highlight
                } else {
                    marks.point_radius
                };
                assert!((r - want).abs() < 1e-6, "M4 slope: end dot radius {r}");
            }
            _ => {}
        }
    }
    assert!(
        lines > 1 && dots == 2 * lines,
        "M4 slope: {lines} lines, {dots} dots"
    );
    let values: Vec<&Item> = s.part(Part::Value).collect();
    assert!(
        values.iter().any(|v| v.emphasis == Emphasis::Accent),
        "M8 slope: the named line's labels"
    );
    for v in values {
        let Shape::Text { style, .. } = &v.shape else {
            panic!()
        };
        if v.emphasis == Emphasis::Accent {
            assert_eq!(style.weight, 600, "M8 slope: {:?} at weight 600", v.text());
        } else {
            assert_eq!(v.emphasis, Emphasis::Muted, "M8 slope: the rest are muted");
        }
    }
    // With nothing named, every line and dot keeps one size.
    let s = without_highlight("slope");
    for m in s.marks() {
        match m.shape {
            Shape::Line { width, .. } => assert!((width - marks.line_width).abs() < 1e-6),
            Shape::Circle { r, .. } => assert!((r - 60f32.sqrt() / 2.0).abs() < 1e-6),
            _ => {}
        }
    }
}

#[test]
fn m8_several_lines_are_named_at_their_ends_without_a_legend() {
    let e = engine();
    let s = scene_of(&gallery("multi-line")).unwrap();
    assert_eq!(
        s.part(Part::Legend)
            .filter(|i| matches!(i.shape, Shape::Circle { .. }))
            .count(),
        0,
        "M8: no legend swatches"
    );
    let names: Vec<&Item> = s.part(Part::Legend).collect();
    assert_eq!(names.len(), 5, "M8: one name per line");
    let accent = e.tokens().palette(s.theme).accent;
    for n in &names {
        let b = text_box(&e, n);
        assert!(
            b.left >= s.plot.right,
            "M8: {:?} sits right of the lines",
            n.text()
        );
        assert!(
            b.right <= s.card.right,
            "M8: {:?} stays on the card",
            n.text()
        );
        let Shape::Text { style, .. } = &n.shape else {
            panic!()
        };
        if n.text() == Some("Germany") {
            assert_eq!(
                (n.color, style.weight),
                (accent, 600),
                "M8: the named line's name"
            );
        } else {
            assert_eq!(n.emphasis, Emphasis::Muted, "M8: the rest are muted");
        }
    }
    for (i, a) in names.iter().enumerate() {
        for b in &names[i + 1..] {
            assert!(
                !overlaps(&text_box(&e, a), &text_box(&e, b)),
                "M8: names overlap"
            );
        }
    }
    // Too many names to fit down the plot: a legend instead.
    let rows: Vec<serde_json::Value> = (0..20)
        .flat_map(|i| {
            ["Q1", "Q2"].map(|q| serde_json::json!({"q": q, "s": format!("S{i}"), "v": i}))
        })
        .collect();
    let mut v = intent(
        "multi-line",
        &serde_json::json!({"x": "q", "y": "v", "series": "s"}),
        &rows,
    );
    v["highlight"] = serde_json::json!(["S3"]);
    let s = scene_of(&v).unwrap();
    assert!(
        s.part(Part::Legend)
            .any(|i| matches!(i.shape, Shape::Circle { .. })),
        "M8: twenty names fall back to a legend"
    );
    // Lines that all end at the bottom keep their names down the plot.
    let rows: Vec<serde_json::Value> = ["A", "B", "C", "D"]
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            [("Q1", 100 - 10 * i), ("Q2", 0)]
                .map(|(q, v)| serde_json::json!({"q": q, "s": s, "v": v}))
        })
        .collect();
    let v = intent(
        "multi-line",
        &serde_json::json!({"x": "q", "y": "v", "series": "s"}),
        &rows,
    );
    let s = scene_of(&v).unwrap();
    let names: Vec<&Item> = s.part(Part::Legend).collect();
    assert_eq!(names.len(), 4, "M8");
    for n in &names {
        let b = text_box(&e, n);
        assert!(
            b.center_y() <= s.plot.bottom + 1.0,
            "M8: {:?} sits below the plot at {}",
            n.text(),
            b.bottom
        );
    }
    for (i, a) in names.iter().enumerate() {
        for b in &names[i + 1..] {
            assert!(
                !overlaps(&text_box(&e, a), &text_box(&e, b)),
                "M8: names overlap"
            );
        }
    }
}

/// The line fixture run backwards and named at June, so the line's earlier,
/// higher stretch runs left of the named point.
fn falling_line() -> serde_json::Value {
    let mut v = fixture("line");
    let rows = v["data"]["rows"].as_array_mut().unwrap();
    let teams: Vec<serde_json::Value> = rows.iter().rev().map(|r| r["teams"].clone()).collect();
    for (row, t) in rows.iter_mut().zip(teams) {
        row["teams"] = t;
    }
    v["highlight"] = serde_json::json!(["Jun"]);
    v
}

/// The `n` consecutive texts that read as `note`.
fn note_lines<'a>(texts: &[&'a Item], note: &str, n: usize) -> Option<Vec<&'a Item>> {
    texts
        .windows(n)
        .find(|w| {
            w.iter()
                .filter_map(|i| i.text())
                .collect::<Vec<_>>()
                .join(" ")
                == note
        })
        .map(<[&Item]>::to_vec)
}

/// The column fixture named at a tall W2 among short columns, so a note has
/// room above its neighbours.
fn tall_column() -> serde_json::Value {
    let mut v = fixture("column");
    let rows = v["data"]["rows"].as_array_mut().unwrap();
    for (row, n) in rows.iter_mut().zip([140, 480, 120, 130, 110, 150]) {
        row["signups"] = n.into();
    }
    v["highlight"] = serde_json::json!(["W2"]);
    v
}

/// The card's height for the same intent with its note left out.
fn height_without_note(e: &Engine, v: &serde_json::Value, opts: &Options) -> f32 {
    let mut bare = v.clone();
    bare.as_object_mut().unwrap().remove("note");
    let intent = tiles_intent::parse(&bare.to_string()).unwrap();
    e.scene(&intent, opts).unwrap().height
}

/// Whether any drawn mark or guide line, such as a dot plot's track, passes
/// through the inside of `area`.
fn on_a_line(scene: &Scene, area: &skia_safe::Rect) -> bool {
    // A lollipop's average rule is crossed on the note's knockout, as a
    // gridline is.
    let rule = |i: &&Item| i.emphasis == Emphasis::Neutral(0);
    crosses(
        scene
            .marks()
            .chain(scene.part(Part::Guide).filter(|i| !rule(i))),
        area,
    )
}

/// Whether any line among `items` passes through the inside of `area`.
fn crosses<'a>(mut items: impl Iterator<Item = &'a Item>, area: &skia_safe::Rect) -> bool {
    items.any(|m| {
        let Shape::Line { points, .. } = &m.shape else {
            return false;
        };
        points.windows(2).any(|seg| {
            let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
            (0..=100_u8).any(|k| {
                let at = f32::from(k) / 100.0;
                let (x, y) = (x0 + (x1 - x0) * at, y0 + (y1 - y0) * at);
                area.left < x && x < area.right && area.top < y && y < area.bottom
            })
        })
    })
}

#[test]
#[allow(clippy::too_many_lines)] // One table of cases and one check per rule.
fn m9_a_note_sits_beside_its_mark_on_the_card_and_clear_of_other_text() {
    let e = engine();
    let wide = Options::default().width;
    // Each case and the lines its note takes, or 0 for a refusal.
    let mut cases = vec![
        (gallery("column-highlight"), wide, 1),
        (gallery("dumbbell"), wide, 1),
    ];
    for (form, lines) in [
        ("column", 1),
        ("bar", 2),
        ("lollipop", 1),
        ("area", 1),
        ("multi-line", 1),
    ] {
        let mut v = fixture(form);
        v["note"] = "After the launch".into();
        cases.push((v, wide, lines));
    }
    let mut dots = fixture("dot-plot");
    dots["note"] = "After the launch".into();
    for width in [280.0, 340.0, 400.0, wide] {
        cases.push((dots.clone(), width, 1));
    }
    let mut falling = falling_line();
    falling["note"] = "After the launch".into();
    cases.push((falling, wide, 1));
    // The longest note an intent may carry, from the narrowest card up: one
    // line where it fits, two where only two do, refused where neither does.
    let long = "Teams doubled once the shared workspace launched";
    assert_eq!(long.chars().count(), 48);
    let mut line = fixture("line");
    line["highlight"] = serde_json::json!(["Jun"]);
    for (mut v, lines) in [
        (line, [0, 0, 2, 1]),
        (fixture("column"), [0, 0, 0, 0]),
        (tall_column(), [0, 2, 2, 1]),
        (fixture("bar"), [0, 0, 0, 0]),
        (fixture("lollipop"), [0, 2, 1, 1]),
        (fixture("dot-plot"), [0, 2, 2, 1]),
        (fixture("dumbbell"), [0, 0, 1, 1]),
        (fixture("multi-line"), [0, 0, 0, 2]),
    ] {
        v["note"] = long.into();
        for (width, n) in [280.0, 340.0, 400.0, wide].into_iter().zip(lines) {
            cases.push((v.clone(), width, n));
        }
    }
    for (v, width, expected) in cases {
        let form = &v["form"];
        let note = v["note"].as_str().unwrap().to_string();
        let intent = tiles_intent::parse(&v.to_string()).unwrap();
        let opts = Options {
            width,
            ..Options::default()
        };
        let s = match e.scene(&intent, &opts) {
            Ok(s) => s,
            Err(RenderError::Intent(err)) if expected == 0 => {
                assert_eq!(err.key, "note", "M9 {form} at {width}: {err}");
                assert!(
                    err.problem.contains(&format!("{width} px")) && err.fix.contains("shorten"),
                    "M9 {form} at {width}: the refusal names the width and the fix, {err}"
                );
                continue;
            }
            Err(err) => panic!("M9 {form} at {width}: {err}"),
        };
        assert_ne!(expected, 0, "M9 {form} at {width}: expected a refusal");
        assert!(
            (s.height - height_without_note(&e, &v, &opts)).abs() < 1e-3,
            "M9 {form} at {width}: a note never changes the card's height"
        );
        let texts: Vec<&Item> = s.items.iter().filter(|i| i.text().is_some()).collect();
        let lines = note_lines(&texts, &note, expected)
            .unwrap_or_else(|| panic!("M9 {form} at {width}: the note on {expected} line(s)"));
        let index = |i: &Item| s.items.iter().position(|o| std::ptr::eq(o, i)).unwrap();
        let knockouts: Vec<usize> = (0..s.items.len())
            .filter(|&k| s.items[k].part == Part::Knockout)
            .collect();
        assert_eq!(
            knockouts.len(),
            lines.len(),
            "M9 {form} at {width}: one knockout per note line"
        );
        for item in &lines {
            let b = text_box(&e, item);
            // The line's knockout: card colour, around its text, over every
            // gridline, guide and fill, under the note, and clear of every
            // mark and other text.
            let covers = |k: &&usize| {
                let Shape::Rect { rect, .. } = s.items[**k].shape else {
                    return false;
                };
                let need = b.with_outset((1.5, 1.5));
                rect.left <= need.left
                    && rect.top <= need.top
                    && rect.right >= need.right
                    && rect.bottom >= need.bottom
            };
            let &k = knockouts.iter().find(covers).unwrap_or_else(|| {
                panic!("M9 {form} at {width}: a knockout covers the note's text")
            });
            let knockout = &s.items[k];
            let patch = knockout.bounds();
            assert_eq!(
                (knockout.emphasis, knockout.color),
                (Emphasis::Card, e.tokens().palette(s.theme).card),
                "M9 {form} at {width}: the knockout takes the card's colour"
            );
            assert!(
                k < index(lines[0]),
                "M9 {form} at {width}: the knockouts sit under the note"
            );
            assert!(
                s.items[k..].iter().all(|i| {
                    !matches!(i.part, Part::Grid | Part::Guide)
                        && !matches!(i.shape, Shape::Area { .. })
                        || !overlaps(&i.bounds(), &patch)
                }),
                "M9 {form} at {width}: the knockout sits over every gridline, guide and fill"
            );
            for mark in s
                .marks()
                .filter(|m| matches!(m.shape, Shape::Rect { .. } | Shape::Circle { .. }))
            {
                assert!(
                    !overlaps(&patch, &mark.bounds()),
                    "M9 {form} at {width}: a mark touches the knockout"
                );
            }
            assert!(
                !crosses(s.marks(), &patch),
                "M9 {form} at {width}: a mark line crosses the knockout"
            );
            for other in texts
                .iter()
                .filter(|o| !lines.iter().any(|l| std::ptr::eq(*l, **o)))
            {
                assert!(
                    !overlaps(&patch, &text_box(&e, other)),
                    "M9 {form} at {width}: the knockout covers {:?}",
                    other.text()
                );
            }
            assert!(
                b.left >= s.card.left && b.right <= s.card.right && b.top >= s.card.top,
                "M9 {form} at {width}: the note stays on the card"
            );
            // A callout keeps a row value's distance from every mark and
            // line; a stand-in sits where its value would.
            let pad = if item.emphasis == Emphasis::Muted {
                e.tokens().marks.row_value_offset - 0.5
            } else {
                0.0
            };
            let clear = b.with_outset((pad, pad));
            for mark in s
                .marks()
                .filter(|m| matches!(m.shape, Shape::Rect { .. } | Shape::Circle { .. }))
            {
                assert!(
                    !overlaps(&clear, &mark.bounds()),
                    "M9 {form} at {width}: the note touches a mark"
                );
            }
            assert!(
                !on_a_line(&s, &clear),
                "M9 {form} at {width}: the note touches a line"
            );
            for other in texts
                .iter()
                .filter(|o| !lines.iter().any(|l| std::ptr::eq(*l, **o)))
            {
                assert!(
                    !overlaps(&b, &text_box(&e, other)),
                    "M9 {form} at {width}: the note overlaps {:?}",
                    other.text()
                );
            }
        }
    }
    let mut v = fixture("stacked-bar");
    v["note"] = "Card leads".into();
    let err = scene_of(&v).unwrap_err().to_string();
    assert!(
        err.starts_with("note: form stacked-bar does not draw a note"),
        "{err}"
    );
}

/// One intent per form that draws a note, each naming a mark.
fn note_forms() -> Vec<serde_json::Value> {
    let mut line = fixture("line");
    line["highlight"] = serde_json::json!(["Jun"]);
    vec![
        fixture("column"),
        tall_column(),
        fixture("bar"),
        fixture("lollipop"),
        fixture("dot-plot"),
        line,
        fixture("area"),
        fixture("multi-line"),
        fixture("dumbbell"),
        gallery("column-highlight"),
        gallery("dumbbell"),
    ]
}

/// Whether `note` stands where a dumbbell's named row writes its second
/// value: 9 px from that dot's centre, within its row. The 9 px is the
/// layout's own gallery offset, the same in every theme; no token sets it.
fn beside_second_dot(s: &Scene, note: &Item) -> bool {
    let Some((cx, cy)) = s
        .marks()
        .filter(|m| m.highlighted)
        .nth(1)
        .and_then(|m| match m.shape {
            Shape::Circle { cx, cy, .. } => Some((cx, cy)),
            _ => None,
        })
    else {
        return false;
    };
    let Shape::Text { x, y, .. } = &note.shape else {
        return false;
    };
    // Half the step to the nearest other row.
    let half = s
        .marks()
        .filter_map(|m| match m.shape {
            Shape::Circle { cy: other, .. } if (other - cy).abs() > 0.5 => Some((other - cy).abs()),
            _ => None,
        })
        .fold(f32::MAX, f32::min)
        / 2.0;
    ((x - cx).abs() - 9.0).abs() < 0.01 && (y - cy).abs() < half
}

/// The largest horizontal or vertical gap between two boxes, 0 if they touch.
fn gap_between(a: &skia_safe::Rect, b: &skia_safe::Rect) -> f32 {
    let dx = (a.left - b.right).max(b.left - a.right).max(0.0);
    let dy = (a.top - b.bottom).max(b.top - a.bottom).max(0.0);
    dx.max(dy)
}

#[test]
fn m9_a_free_note_is_a_muted_label_and_only_a_stand_in_takes_value_type() {
    let e = engine();
    let t = &e.tokens().text;
    let (mut free, mut stand_ins) = (0, 0);
    for bare in note_forms() {
        let form = bare["form"].clone();
        for width in [280.0, 340.0, 400.0, 640.0] {
            let opts = Options {
                width,
                ..Options::default()
            };
            let scene = |v: &serde_json::Value| {
                e.scene(&tiles_intent::parse(&v.to_string()).unwrap(), &opts)
            };
            let values = |s: &Scene| s.part(Part::Value).count();
            let mut plain = bare.clone();
            plain.as_object_mut().unwrap().remove("note");
            let without = values(&scene(&plain).unwrap());
            let notes = [
                bare["note"].as_str().unwrap_or("After the launch"),
                "Teams doubled once the shared workspace launched",
            ];
            for note in notes {
                let mut v = bare.clone();
                v["note"] = note.into();
                let Ok(s) = scene(&v) else { continue };
                let texts: Vec<&Item> = s.items.iter().filter(|i| i.text().is_some()).collect();
                let lines = note_lines(&texts, note, 1)
                    .or_else(|| note_lines(&texts, note, 2))
                    .unwrap_or_else(|| panic!("M9 {form} at {width}: {note:?} is drawn"));
                // A stand-in takes a value's place, so the card has no more
                // values; a dumbbell writes none, so its stand-in is the one
                // set beside the named row's second dot.
                let stand_in = values(&s) - lines.len() < without
                    || (form == "dumbbell" && beside_second_dot(&s, lines[0]));
                for item in lines {
                    let Shape::Text { style, .. } = &item.shape else {
                        panic!()
                    };
                    if stand_in {
                        stand_ins += 1;
                        assert_eq!(
                            (item.emphasis, style.size, style.weight),
                            (Emphasis::Ink, t.value_size, t.value_weight),
                            "M9 {form} at {width}: a stand-in is set as the value"
                        );
                    } else {
                        free += 1;
                        assert_eq!(
                            (item.emphasis, style.size, style.weight),
                            (Emphasis::Muted, t.label_size, t.label_weight),
                            "M9 {form} at {width}: {note:?} is a muted label"
                        );
                    }
                }
            }
        }
    }
    assert!(
        stand_ins > 0 && free > 0,
        "M9: {stand_ins} stand-ins, {free} callouts"
    );
}

#[test]
fn m9_a_dot_plot_note_sits_by_its_dot() {
    let e = engine();
    let pad = e.tokens().marks.label_padding;
    let mut v = fixture("dot-plot");
    v["note"] = "After the launch".into();
    let intent = tiles_intent::parse(&v.to_string()).unwrap();
    for width in [280.0, 340.0, 400.0, 640.0] {
        let opts = Options {
            width,
            ..Options::default()
        };
        let s = e.scene(&intent, &opts).unwrap();
        let dot = s
            .marks()
            .find(|m| m.highlighted && matches!(m.shape, Shape::Circle { .. }))
            .expect("the named dot")
            .bounds();
        // The dot with the value written beside it.
        let mut named = dot;
        for value in s.part(Part::Value) {
            let b = text_box(&e, value);
            if value.text() != Some("After the launch") && b.top < dot.bottom && dot.top < b.bottom
            {
                named.join(b);
            }
        }
        let note = s
            .items
            .iter()
            .find(|i| i.text() == Some("After the launch"))
            .expect("the note");
        let gap = gap_between(&text_box(&e, note), &named);
        assert!(
            gap <= 3.0 * pad,
            "M9 dot-plot at {width}: the note is {gap} px from its dot and value"
        );
    }
}

#[test]
fn m9_a_note_never_moves_the_chart() {
    let e = engine();
    let forms = note_forms();
    let geometry = |s: &Scene| {
        let drawn: Vec<Shape> = s
            .items
            .iter()
            .filter(|i| matches!(i.part, Part::Mark | Part::Guide | Part::Grid | Part::Hit))
            .map(|i| i.shape.clone())
            .collect();
        (s.height, s.plot, drawn)
    };
    for bare in forms {
        let form = &bare["form"];
        let mut placed = 0;
        for width in [280.0, 340.0, 400.0, 640.0] {
            let opts = Options {
                width,
                ..Options::default()
            };
            let scene = |v: &serde_json::Value| {
                e.scene(&tiles_intent::parse(&v.to_string()).unwrap(), &opts)
            };
            let without = geometry(&scene(&bare).unwrap());
            for note in [
                "After the launch",
                "Teams doubled once the shared workspace launched",
            ] {
                let mut v = bare.clone();
                v["note"] = note.into();
                let Ok(s) = scene(&v) else { continue };
                placed += 1;
                assert!(
                    geometry(&s) == without,
                    "M9 {form} at {width}: {note:?} moved the chart"
                );
            }
        }
        assert!(
            placed >= 2,
            "M9 {form}: a note places at two widths or more"
        );
    }
}

#[test]
fn m10_a_dumbbell_leads_with_the_row_its_title_names() {
    let s = scene_of(&gallery("dumbbell")).unwrap();
    let rows = texts(&s, Part::AxisLabel);
    assert_eq!(rows[0], "APAC", "M10: the named exception first, {rows:?}");
    assert_eq!(rows[1], "EMEA North", "M10: then the largest gap, {rows:?}");
}

/// The box of the mark a note is about: the named point where a period is
/// named, else the named columns, bars, stems and dots, else the end of the
/// named line.
#[allow(clippy::float_cmp)] // A point laid over its own shares its exact centre.
fn named_mark(e: &Engine, s: &Scene) -> skia_safe::Rect {
    let r = e.tokens().marks.point_radius_highlight;
    let around = |x: f32, y: f32| skia_safe::Rect::from_ltrb(x - r, y - r, x + r, y + r);
    let named: Vec<&Item> = s.marks().filter(|m| m.highlighted).collect();
    // A named period lays a larger point over its own.
    let circles: Vec<(f32, f32, f32)> = named
        .iter()
        .filter_map(|m| match m.shape {
            Shape::Circle { cx, cy, r } => Some((cx, cy, r)),
            _ => None,
        })
        .collect();
    if let Some(&(x, y, _)) = circles.iter().find(|(x, y, pr)| {
        (pr - r).abs() < 1e-3
            && circles
                .iter()
                .any(|(x2, y2, r2)| x == x2 && y == y2 && r2 < pr)
    }) {
        return around(x, y);
    }
    let solid: Vec<skia_safe::Rect> = named
        .iter()
        .filter(|m| match &m.shape {
            Shape::Rect { .. } | Shape::Circle { .. } => true,
            // A lollipop's stem.
            Shape::Line { points, .. } => points.len() == 2,
            _ => false,
        })
        .map(|m| m.bounds())
        .collect();
    if let Some(first) = solid.first() {
        // By hand: a stem's box has no height, and `join` skips empty boxes.
        return solid.iter().fold(*first, |a, b| {
            skia_safe::Rect::from_ltrb(
                a.left.min(b.left),
                a.top.min(b.top),
                a.right.max(b.right),
                a.bottom.max(b.bottom),
            )
        });
    }
    named
        .iter()
        .find_map(|m| match &m.shape {
            Shape::Line { points, .. } => points.last().map(|&(x, y)| around(x, y)),
            _ => None,
        })
        .expect("a named mark")
}

/// The straight-line distance between two boxes, 0 where they touch.
fn distance(a: &skia_safe::Rect, b: &skia_safe::Rect) -> f32 {
    let dx = (a.left - b.right).max(b.left - a.right).max(0.0);
    let dy = (a.top - b.bottom).max(b.top - a.bottom).max(0.0);
    dx.hypot(dy)
}

#[test]
#[allow(clippy::many_single_char_names)] // Test geometry.
fn m9_a_callout_reads_as_its_own_marks() {
    let e = engine();
    let t = e.tokens();
    let line_h = (t.text.label_size * 1.3).round();
    for bare in note_forms() {
        let form = bare["form"].clone();
        for width in [280.0, 340.0, 400.0, 640.0] {
            let opts = Options {
                width,
                ..Options::default()
            };
            for note in [
                bare["note"].as_str().unwrap_or("After the launch"),
                "Teams doubled once the shared workspace launched",
            ] {
                let mut v = bare.clone();
                v["note"] = note.into();
                let Ok(s) = e.scene(&tiles_intent::parse(&v.to_string()).unwrap(), &opts) else {
                    continue;
                };
                let texts: Vec<&Item> = s.items.iter().filter(|i| i.text().is_some()).collect();
                let lines = note_lines(&texts, note, 1)
                    .or_else(|| note_lines(&texts, note, 2))
                    .unwrap_or_else(|| panic!("M9 {form} at {width}: {note:?} is drawn"));
                if lines[0].emphasis != Emphasis::Muted {
                    continue; // A stand-in sits where its value would.
                }
                let mut b = text_box(&e, lines[0]);
                for l in &lines {
                    b.join(text_box(&e, l));
                }
                let mark = named_mark(&e, &s);
                let d = distance(&b, &mark);
                assert!(
                    d <= 3.0 * line_h,
                    "M9 {form} at {width}: {note:?} is {d} px from its mark"
                );
                // No row label, tick or other mark's value sits as near the
                // note as its own mark does.
                for other in &texts {
                    let ob = text_box(&e, other);
                    let own_value = other.part == Part::Value
                        && distance(&ob, &mark) <= t.marks.label_padding + 1.0;
                    let label = matches!(other.part, Part::AxisLabel | Part::Value);
                    if lines.iter().any(|l| std::ptr::eq(*l, *other)) || own_value || !label {
                        continue;
                    }
                    assert!(
                        distance(&b, &ob) > d,
                        "M9 {form} at {width}: {note:?} reads as {:?}'s, {} px from it and {d} px from its mark",
                        other.text(),
                        distance(&b, &ob)
                    );
                }
            }
        }
    }
}

#[test]
fn m11_a_bullet_grades_its_target_in_three_bands_under_a_dark_bar() {
    let s = scene_of(&gallery("bullet")).unwrap();
    let bands: Vec<skia_safe::Rect> = s
        .part(Part::Guide)
        .filter(|g| matches!(g.shape, Shape::Rect { .. }))
        .map(rect)
        .collect();
    let bars: Vec<&Item> = s.marks().collect();
    assert_eq!(bands.len(), 3 * bars.len(), "M11: three bands a row");
    let zero = bands[0].left;
    for (row, bar) in bands.chunks(3).zip(&bars) {
        // Under 70%, 70 to 90%, 90 to 115% of the target.
        let w = |r: &skia_safe::Rect| r.right - zero;
        assert!((w(&row[0]) / w(&row[2]) - 0.7 / 1.15).abs() < 0.01, "M11");
        assert!((w(&row[1]) / w(&row[2]) - 0.9 / 1.15).abs() < 0.01, "M11");
        let b = rect(bar);
        assert!(
            b.height() < row[0].height() / 2.0,
            "M11: the bar is thin on its bands"
        );
        let want = if bar.highlighted {
            Emphasis::Bad
        } else {
            Emphasis::Muted
        };
        assert_eq!(bar.emphasis, want, "M11: named rows red, the rest dark");
    }
    assert_eq!(
        texts(&s, Part::Value),
        ["109%", "102%", "107%", "76%", "68%"],
        "M11: each row writes its share of target"
    );
    let ticks = texts(&s, Part::AxisLabel);
    assert!(
        ticks.contains(&"3k".to_string()),
        "M11: a value axis, {ticks:?}"
    );
}

#[test]
fn m12_shares_stack_to_the_plot_width_with_their_shares_inside() {
    let s = scene_of(&gallery("stacked-100-bar")).unwrap();
    let rows: Vec<String> = s
        .part(Part::AxisLabel)
        .filter_map(|i| i.text().map(String::from))
        .filter(|t| !t.ends_with('%'))
        .collect();
    assert_eq!(
        rows,
        ["France", "Spain", "Belgium", "Germany", "Netherlands"],
        "M12: rows run from the named series' largest share"
    );
    let segments: Vec<skia_safe::Rect> = s.marks().map(rect).collect();
    for bar in segments.chunks(4) {
        assert!((bar[0].left - s.plot.left).abs() < 0.01, "M12: from 0%");
        assert!((bar[3].right - s.plot.right).abs() < 0.01, "M12: to 100%");
        for pair in bar.windows(2) {
            let seam = pair[1].left - pair[0].right;
            assert!((seam - 1.5).abs() < 0.01, "M12: a 1.5 px seam, {seam}");
        }
    }
    let marks: Vec<&Item> = s.marks().collect();
    for v in s.part(Part::Value) {
        let Shape::Text { x, y, .. } = &v.shape else {
            panic!()
        };
        let home = marks
            .iter()
            .find(|m| {
                let r = rect(m);
                r.left < *x && *x < r.right && r.top < *y - 3.0 && *y - 3.0 < r.bottom
            })
            .expect("M12: each share sits inside its segment");
        let want = if home.highlighted {
            Emphasis::Card
        } else {
            Emphasis::Ink
        };
        assert_eq!(
            v.emphasis, want,
            "M12: card type on the accent, ink on grey"
        );
    }
    // Other's 4 to 6% go unwritten.
    assert_eq!(
        s.part(Part::Value).count(),
        15,
        "M12: shares under 10% stay unwritten"
    );
    let mut v = gallery("stacked-100-bar");
    v["data"]["rows"][0]["share"] = (-1).into();
    let err = scene_of(&v).unwrap_err().to_string();
    assert!(err.contains("needs values of zero or more"), "{err}");
}

#[test]
fn a_lollipop_marks_the_average_with_a_dashed_rule() {
    let s = scene("lollipop");
    let dashes: Vec<&Item> = s
        .part(Part::Guide)
        .filter(|g| g.emphasis == Emphasis::Neutral(0))
        .collect();
    assert!(dashes.len() > 10, "a dashed rule, {} dashes", dashes.len());
    let xs: Vec<f32> = dashes.iter().map(|d| d.bounds().left).collect();
    assert!(
        xs.iter().all(|x| (x - xs[0]).abs() < 0.01),
        "one vertical rule"
    );
    // The fixture's mean failures: (41 + 77 + 132 + 58 + 23 + 12) / 6 = 57.
    assert!(
        texts(&s, Part::AxisLabel).contains(&"Average 57".to_string()),
        "named with the rows' mean"
    );
    let mut v = fixture("lollipop");
    for row in v["data"]["rows"].as_array_mut().unwrap() {
        row["all"] = 103.into();
    }
    v["fields"]["average"] = "all".into();
    let s = scene_of(&v).unwrap();
    assert!(
        texts(&s, Part::AxisLabel).contains(&"Average 103".to_string()),
        "or with the average the intent gives"
    );
}

#[test]
fn a_lollipop_keeps_an_average_past_its_rows_on_the_scale() {
    let e = engine();
    let mut v = fixture("lollipop");
    for row in v["data"]["rows"].as_array_mut().unwrap() {
        row["all"] = 200.into();
    }
    v["fields"]["average"] = "all".into();
    let s = scene_of(&v).unwrap();
    let rule = s
        .part(Part::Guide)
        .find(|g| g.emphasis == Emphasis::Neutral(0))
        .expect("an average rule");
    assert!(
        rule.bounds().left <= s.plot.right + 0.01,
        "M13: the rule stands on the scale"
    );
    let name = s
        .part(Part::AxisLabel)
        .find(|t| t.text() == Some("Average 200"))
        .expect("the rule is named");
    let b = text_box(&e, name);
    assert!(
        b.left >= s.card.left && b.right <= s.card.right,
        "M13: the name stays on the card"
    );
}

#[test]
fn m13_a_one_row_lollipop_draws_no_average() {
    let mut v = fixture("lollipop");
    v["data"]["rows"].as_array_mut().unwrap().truncate(1);
    v.as_object_mut().unwrap().remove("highlight");
    let s = scene_of(&v).unwrap();
    assert!(
        s.part(Part::Guide)
            .all(|g| g.emphasis != Emphasis::Neutral(0)),
        "M13: one row is its own average, so no rule"
    );
    assert!(
        texts(&s, Part::AxisLabel)
            .iter()
            .all(|t| !t.starts_with("Average")),
        "M13: and no name"
    );
}

#[test]
fn m13_a_one_row_lollipop_draws_the_average_the_intent_sends() {
    let mut v = fixture("lollipop");
    v["data"]["rows"].as_array_mut().unwrap().truncate(1);
    v.as_object_mut().unwrap().remove("highlight");
    v["data"]["rows"][0]["all"] = 103.into();
    v["fields"]["average"] = "all".into();
    let s = scene_of(&v).unwrap();
    assert!(
        s.part(Part::Guide)
            .any(|g| g.emphasis == Emphasis::Neutral(0)),
        "M13: an average the intent sends is drawn on one row"
    );
    assert!(
        texts(&s, Part::AxisLabel).contains(&"Average 103".to_string()),
        "M13: and named"
    );
}

/// The numbers under the plot, left to right, with `k` and `M` read out.
fn value_ticks(s: &Scene) -> Vec<f64> {
    s.part(Part::AxisLabel)
        .filter(|t| t.bounds().bottom > s.plot.bottom)
        .filter_map(|t| {
            let text = t.text()?.replace(',', "");
            let (digits, times) = match text.chars().last()? {
                'k' => (&text[..text.len() - 1], 1e3),
                'M' => (&text[..text.len() - 1], 1e6),
                _ => (text.as_str(), 1.0),
            };
            let n: String = digits
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect();
            Some(n.parse::<f64>().ok()? * times)
        })
        .collect()
}

#[test]
fn a_value_axis_on_a_narrow_card_keeps_an_even_step() {
    let e = engine();
    let mut lollipop = fixture("lollipop");
    lollipop["data"]["rows"][2]["failures"] = 115.into();
    let forms = [
        lollipop,
        fixture("dumbbell"),
        fixture("bullet"),
        fixture("scatter"),
    ];
    let mut width = tiles_engine::MIN_WIDTH;
    while width <= 400.0 {
        for v in &forms {
            let intent = tiles_intent::parse(&v.to_string()).unwrap();
            let opts = Options {
                width,
                ..Options::default()
            };
            let s = e.scene(&intent, &opts).unwrap();
            let ticks = value_ticks(&s);
            assert!(ticks.len() >= 2, "{} at {width}: {ticks:?}", s.form);
            let step = ticks[1] - ticks[0];
            assert!(
                ticks.windows(2).all(|w| (w[1] - w[0] - step).abs() < 1e-9),
                "{} at {width} px reads {ticks:?}, an uneven step",
                s.form
            );
        }
        width += 2.0;
    }
}

#[test]
fn m13_a_lollipop_value_stays_clear_of_the_average_rule() {
    let e = engine();
    let v = serde_json::from_str(include_str!("intents/worst-case/lollipop-worst.json")).unwrap();
    let s = scene_of(&v).unwrap();
    let rule = s
        .part(Part::Guide)
        .find(|g| g.emphasis == Emphasis::Neutral(0))
        .expect("an average rule")
        .bounds();
    let name = s
        .part(Part::AxisLabel)
        .find(|t| t.text().is_some_and(|t| t.starts_with("Average")))
        .map(|t| text_box(&e, t))
        .expect("the rule is named");
    for value in s.part(Part::Value) {
        let b = text_box(&e, value);
        assert!(
            b.right < rule.left - 1.0 || b.left > rule.right + 1.0,
            "M13: {:?} crosses the average rule",
            value.text()
        );
        assert!(
            !overlaps(&b, &name),
            "M13: {:?} touches the rule's name",
            value.text()
        );
    }
}

#[test]
fn m13_a_lollipop_value_clears_an_average_above_its_rows() {
    let e = engine();
    let mut v = fixture("lollipop");
    v.as_object_mut().unwrap().remove("highlight");
    v["fields"]["average"] = "all".into();
    v["data"]["rows"] = serde_json::json!([
        {"step": "Fetch", "failures": 96, "all": 100},
        {"step": "Auth", "failures": 40, "all": 100},
    ]);
    let intent = tiles_intent::parse(&v.to_string()).unwrap();
    let opts = Options {
        width: 400.0,
        ..Options::default()
    };
    let s = e.scene(&intent, &opts).unwrap();
    let rule = s
        .part(Part::Guide)
        .find(|g| g.emphasis == Emphasis::Neutral(0))
        .expect("an average rule")
        .bounds();
    for value in s.part(Part::Value) {
        let b = text_box(&e, value);
        assert!(
            b.right < rule.left - 1.0 || b.left > rule.right + 1.0,
            "M13: {:?} crosses the average rule",
            value.text()
        );
        assert!(
            b.right <= s.card.right,
            "M13: {:?} leaves the card",
            value.text()
        );
    }
}

#[test]
fn m12_a_share_that_only_rounds_to_an_end_reads_under_1_or_over_99() {
    let rows: Vec<serde_json::Value> = [
        ("A", "Card", 1_284_000.5),
        ("A", "Cash", 42.5),
        ("B", "Card", 50.0),
        ("B", "Cash", 50.0),
        ("C", "Card", 100.0),
        ("C", "Cash", 0.0),
    ]
    .iter()
    .map(|(m, w, v)| serde_json::json!({"m": m, "w": w, "v": v}))
    .collect();
    let fields = serde_json::json!({"x": "m", "y": "v", "series": "w"});
    let s = scene_of(&intent("stacked-100-bar", &fields, &rows)).unwrap();
    assert_eq!(
        texts(&s, Part::Value),
        [">99", "50", "50", "100"],
        "M12: a share short of 100 never reads 100"
    );
    let tips: Vec<&str> = s.marks().filter_map(|m| m.tip.as_deref()).collect();
    assert!(
        tips.iter().any(|t| t.ends_with("share: <1%")),
        "M12: a share above 0 never reads 0, {tips:?}"
    );
}

#[test]
fn one_long_category_name_hides_only_its_neighbours() {
    let names = [
        "Benachrichtigungseinstellungen",
        "North",
        "South",
        "East",
        "West",
        "Online",
    ];
    let rows: Vec<serde_json::Value> = names
        .iter()
        .zip(1..)
        .map(|(n, v)| serde_json::json!({"n": n, "v": v}))
        .collect();
    let fields = serde_json::json!({"x": "n", "y": "v"});
    for form in ["column", "line"] {
        let s = scene_of(&intent(form, &fields, &rows)).unwrap();
        let shown: Vec<String> = texts(&s, Part::AxisLabel)
            .into_iter()
            .filter(|t| names.contains(&t.as_str()))
            .collect();
        assert!(
            shown.len() >= 5,
            "{form}: a long name leaves the short ones named, {shown:?}"
        );
    }
}

#[test]
fn a_big_number_counts_months_ago_from_its_first_to_its_last_period() {
    let quarters: Vec<serde_json::Value> = ["2025-01", "2025-04", "2025-07", "2025-10", "2026-01"]
        .iter()
        .enumerate()
        .map(|(i, m)| serde_json::json!({"m": m, "v": 100 + i}))
        .collect();
    let mut gap: Vec<serde_json::Value> = (1..=13)
        .map(|i| {
            let (y, m) = (2025 + (i - 1) / 12, (i - 1) % 12 + 1);
            serde_json::json!({"m": format!("{y}-{m:02}"), "v": 100 + i})
        })
        .collect();
    gap[5]["v"] = serde_json::Value::Null;
    for rows in [quarters, gap] {
        let v = intent(
            "big-number",
            &serde_json::json!({"x": "m", "y": "v"}),
            &rows,
        );
        let s = scene_of(&v).unwrap();
        assert_eq!(texts(&s, Part::AxisLabel), ["12 months ago"]);
    }
}

fn target_bar(s: &Scene) -> (Vec<&Item>, Vec<&Item>) {
    let track = s
        .part(Part::Guide)
        .filter(|g| g.emphasis == Emphasis::Track)
        .collect();
    let fill = s
        .marks()
        .filter(|m| matches!(m.shape, Shape::Rect { .. }))
        .collect();
    (track, fill)
}

#[test]
fn a_big_number_with_a_target_fills_a_bar_by_its_share_of_it() {
    let s = scene("big-number");
    let (track, fill) = target_bar(&s);
    assert_eq!((track.len(), fill.len()), (1, 1), "one track, one fill");
    let (track, fill) = (rect(track[0]), rect(fill[0]));
    assert!((track.height() - 6.0).abs() < 0.01, "a 6 px track");
    assert!((track.left - fill.left).abs() < 0.01 && (track.top - fill.top).abs() < 0.01);
    // The fixture's last 4.82M of the 5.00M target.
    assert!(
        (fill.width() - track.width() * 4.82 / 5.0).abs() < 0.5,
        "the fill is the share of target"
    );
    assert_eq!(
        s.marks()
            .find(|m| matches!(m.shape, Shape::Rect { .. }))
            .unwrap()
            .emphasis,
        Emphasis::Accent
    );
    let line = s
        .marks()
        .find_map(|m| match &m.shape {
            Shape::Line { points, .. } => Some(points.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        line.iter().all(|p| p.1 > track.bottom),
        "the sparkline runs under the bar"
    );
    let mut v = fixture("big-number");
    v["claim"]["tone"] = "bad".into();
    let s = scene_of(&v).unwrap();
    assert_eq!(
        target_bar(&s).1[0].emphasis,
        Emphasis::Bad,
        "red for a bad tone"
    );

    let mut v = fixture("big-number");
    v["fields"].as_object_mut().unwrap().remove("target");
    let s = scene_of(&v).unwrap();
    let (track, fill) = target_bar(&s);
    assert!(track.is_empty() && fill.is_empty(), "no target, no bar");
}

#[test]
fn a_dumbbell_legend_keys_the_second_period_in_the_accent_its_dots_take() {
    let rows: Vec<serde_json::Value> = [
        ("Billing", 46, 18),
        ("Tech", 30, 31),
        ("Sales", 22, 23),
        ("Account", 25, 24),
    ]
    .iter()
    .map(|(k, a, b)| serde_json::json!({"k": k, "before": a, "after": b}))
    .collect();
    let mut v = intent(
        "dumbbell",
        &serde_json::json!({"x": "k", "lo": "before", "hi": "after"}),
        &rows,
    );
    v["claim"]["tone"] = "bad".into();
    v["highlight"] = serde_json::json!(["Billing"]);
    let s = scene_of(&v).unwrap();
    let swatches: Vec<&Item> = s
        .part(Part::Legend)
        .filter(|i| i.text().is_none())
        .collect();
    assert_eq!(swatches.len(), 2, "no row has room, so a legend");
    assert_eq!(
        swatches[1].emphasis,
        Emphasis::Accent,
        "M10: keyed as most second dots are drawn"
    );
}

#[test]
fn a_dumbbell_names_its_periods_only_where_they_stay_on_the_card() {
    let e = engine();
    let rows = [
        serde_json::json!({"k": "A", "start": 10, "after_launch_week": 60}),
        serde_json::json!({"k": "B", "start": 20, "after_launch_week": 25}),
    ];
    let v = intent(
        "dumbbell",
        &serde_json::json!({"x": "k", "lo": "start", "hi": "after_launch_week"}),
        &rows,
    );
    let s = scene_of(&v).unwrap();
    for name in s.part(Part::Legend).filter(|i| i.text().is_some()) {
        let b = text_box(&e, name);
        assert!(
            b.left >= s.card.left && b.right <= s.card.right,
            "M10: {:?} stays on the card",
            name.text()
        );
    }
}

/// A model's slip draws a card with a note instead of a refusal wherever
/// the card shows no wrong number (ADR-020), and the card path's `mend`
/// leaves nothing for the form rules to refuse.
#[test]
fn mend_corrects_what_draws_no_wrong_number() {
    let engine = engine();
    let mended = |v: &serde_json::Value| {
        let mut intent = tiles_intent::parse(&v.to_string()).unwrap();
        engine.mend(&mut intent);
        let scene = engine.scene(&intent, &Options::default());
        (intent, scene)
    };
    // The live dashboard's first refusal: the headline value named as the
    // highlight on a line over months highlights that value's month.
    let mut v = fixture("line");
    let y = v["fields"]["y"].as_str().unwrap().to_string();
    let x = v["fields"]["x"].as_str().unwrap().to_string();
    let last = v["data"]["rows"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    v["highlight"] = serde_json::json!([last[&y].to_string()]);
    let (intent, scene) = mended(&v);
    let month = last[&x].as_str().unwrap();
    assert_eq!(intent.highlight, [month]);
    assert!(
        intent.notes[0].contains("is a value, not a category"),
        "{:?}",
        intent.notes
    );
    assert!(
        !intent.notes[0].contains(month),
        "names no value from the rows"
    );
    assert!(scene.is_ok());
    let mut v = fixture("column");
    let y = v["fields"]["y"].as_str().unwrap().to_string();
    let first = v["data"]["rows"][0][&y].as_f64().unwrap();
    v["data"]["rows"][0][&y] = first.to_string().into();
    let (intent, scene) = mended(&v);
    assert_eq!(
        intent.table.rows[0][intent.column("y").unwrap()].number(),
        Some(first)
    );
    assert_eq!(
        intent.notes,
        [format!(
            "Tiles read 1 text value in column \"{y}\" as numbers."
        )]
    );
    assert!(scene.is_ok());
    // Digits that are not a plain number are refused, not drawn as a gap
    // that moves a scorecard's headline to the period before.
    let mut v = fixture("big-number");
    let y = v["fields"]["y"].as_str().unwrap().to_string();
    let last = v["data"]["rows"].as_array().unwrap().len() - 1;
    v["data"]["rows"][last][&y] = "48,600".into();
    let (_, scene) = mended(&v);
    assert!(
        scene.unwrap_err().to_string().contains("holds text in row"),
        "refused"
    );
    // A highlight that names nothing is left out; an unknown role, a note
    // the form does not draw, and text where numbers go are corrected.
    let mut v = fixture("column");
    v["highlight"] = serde_json::json!(["W9"]);
    v["fields"]["colour"] = "week".into();
    let y = v["fields"]["y"].as_str().unwrap().to_string();
    v["data"]["rows"][0][&y] = "n/a".into();
    let (intent, scene) = mended(&v);
    assert_eq!(intent.highlight, Vec::<String>::new());
    assert!(!intent.fields.contains_key("colour"));
    assert_eq!(intent.notes.len(), 3, "{:?}", intent.notes);
    assert!(scene.is_ok());
    let mut v = fixture("big-number");
    v["note"] = "Record".into();
    v["highlight"] = serde_json::json!(["Q3 26"]);
    let (intent, scene) = mended(&v);
    assert_eq!((intent.note, intent.highlight.len()), (None, 0));
    assert!(scene.is_ok());
    // Without mend the form rules still refuse.
    let mut v = fixture("column");
    v["highlight"] = serde_json::json!(["W9"]);
    assert!(scene_of(&v).is_err());
}

/// Numbers sent as text meet the percent and pre-scaled guards as the
/// numbers they spell, so reading them draws no wrong number (ADR-020).
#[test]
fn text_numbers_meet_the_percent_and_prescaled_guards() {
    let engine = engine();
    let mended = |v: &serde_json::Value| {
        let mut intent = tiles_intent::parse(&v.to_string()).unwrap();
        engine.mend(&mut intent);
        let scene = engine.scene(&intent, &Options::default());
        (intent, scene)
    };
    let card = |title: &str, unit: &str, rows: serde_json::Value| {
        serde_json::json!({
            "v": 1, "form": "big-number", "title": title,
            "claim": { "kind": "trend", "tone": "good" },
            "fields": { "x": "month", "y": "users" }, "unit": unit,
            "data": { "rows": rows }
        })
    };
    let v = card(
        "Monthly active users",
        "percent",
        serde_json::json!([{"month": "Sep", "users": "47800"}, {"month": "Oct", "users": "48600"}]),
    );
    let (intent, scene) = mended(&v);
    assert_eq!(intent.unit, None);
    assert_eq!(
        intent.notes,
        [
            "The measure reaches 1,000 or more, so Tiles drew it as a count, not a percentage.",
            "Tiles read 2 text values in column \"users\" as numbers."
        ]
    );
    assert!(scene.is_ok());
    let v = card(
        "Revenue reached $128K",
        "usd",
        serde_json::json!([{"month": "Sep", "users": "120"}, {"month": "Oct", "users": "128"}]),
    );
    let e = tiles_intent::parse(&v.to_string()).unwrap_err();
    assert!(e.to_string().contains("values look pre-scaled"), "{e}");
}
