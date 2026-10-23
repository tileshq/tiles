//! Golden image tests (ADR-006). Each case renders a PNG and compares its
//! pixels with the committed baseline for the tokens the build carries
//! (ADR-037): `tests/golden/` for the house design a build takes from a
//! design pack, `tests/golden-plain/` for the plain theme a build from
//! source seals.
//!
//! A failure writes the actual image and a diff image to `golden-diff/` in
//! Cargo's test scratch directory. `TILES_BLESS=1` rewrites the baselines; use
//! it only for an intended visual change, and show the diff in the PR.

use std::fmt::Write as _;
use std::path::PathBuf;

use tiles_engine::{Engine, Options, Theme};

use skia_safe::{AlphaType, ColorType, Data, EncodedImageFormat, Image, ImageInfo, images};

/// Largest per-channel difference allowed between machines (ADR-006).
const CHANNEL_TOLERANCE: u8 = 1;

struct Pixels {
    width: i32,
    height: i32,
    rgba: Vec<u8>,
}

fn info(width: i32, height: i32) -> ImageInfo {
    ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    )
}

fn decode(png: &[u8]) -> Pixels {
    let image = Image::from_encoded(Data::new_copy(png)).expect("baseline should be a PNG");
    let (width, height) = (image.width(), image.height());
    let row_bytes = usize::try_from(width).expect("width fits usize") * 4;
    let mut rgba = vec![0; row_bytes * usize::try_from(height).expect("height fits usize")];
    assert!(
        image.read_pixels(
            &info(width, height),
            &mut rgba,
            row_bytes,
            (0, 0),
            skia_safe::image::CachingHint::Disallow,
        ),
        "skia should read the decoded pixels"
    );
    Pixels {
        width,
        height,
        rgba,
    }
}

fn encode(pixels: &Pixels) -> Vec<u8> {
    let row_bytes = usize::try_from(pixels.width).expect("width fits usize") * 4;
    let image = images::raster_from_data(
        &info(pixels.width, pixels.height),
        Data::new_copy(&pixels.rgba),
        row_bytes,
    )
    .expect("skia should wrap the diff pixels");
    image
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("skia should encode the diff")
        .as_bytes()
        .to_vec()
}

/// The baselines for this build's tokens.
fn golden_dir() -> PathBuf {
    let dir = if tiles_engine::SEAL_SOURCE == "assets" {
        "tests/golden-plain"
    } else {
        "tests/golden"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir)
}

fn assert_golden(name: &str, actual_png: &[u8]) {
    let baseline = golden_dir().join(format!("{name}.png"));
    if std::env::var("TILES_BLESS").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(baseline.parent().expect("a baseline has a directory"))
            .expect("the baseline directory should be writable");
        std::fs::write(&baseline, actual_png).expect("baseline should be writable");
        return;
    }
    let expected_png = std::fs::read(&baseline)
        .unwrap_or_else(|e| panic!("no baseline at {}: {e}", baseline.display()));
    let expected = decode(&expected_png);
    let actual = decode(actual_png);
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height),
        "{name}: size differs from the baseline"
    );
    let diff: Vec<u8> = actual
        .rgba
        .iter()
        .zip(&expected.rgba)
        .map(|(a, e)| a.abs_diff(*e))
        .collect();
    let worst = diff.iter().copied().max().unwrap_or(0);
    if worst > CHANNEL_TOLERANCE {
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("golden-diff");
        std::fs::create_dir_all(&out).expect("diff directory should be writable");
        std::fs::write(out.join(format!("{name}.actual.png")), actual_png)
            .expect("actual image should be writable");
        // Show every differing channel at full strength, keep alpha opaque.
        let visible: Vec<u8> = diff
            .chunks(4)
            .flat_map(|px| {
                let lit = |c: u8| if c > CHANNEL_TOLERANCE { 255 } else { 0 };
                [lit(px[0]), lit(px[1]), lit(px[2]), 255]
            })
            .collect();
        std::fs::write(
            out.join(format!("{name}.diff.png")),
            encode(&Pixels {
                width: actual.width,
                height: actual.height,
                rgba: visible,
            }),
        )
        .expect("diff image should be writable");
        panic!(
            "{name}: a channel differs by {worst} (tolerance {CHANNEL_TOLERANCE}); see {}",
            out.display()
        );
    }
}

#[test]
fn blank_card_matches_its_baseline() {
    let png = tiles_engine::blank_png(4, 4).expect("skia should draw a 4x4 surface");
    assert_golden("blank-4x4", &png);
}

/// Every shipped form, in every theme, at scale 1 (ADR-006, ADR-021). The
/// intents are in `tests/intents/`, one per form.
#[test]
fn every_form_matches_its_baselines_in_every_theme() {
    let engine = Engine::new().expect("the embedded assets should load");
    let mut drawn = 0;
    for form in engine.forms() {
        let json = intent_json(&form.name);
        let intent = tiles_intent::parse(&json).unwrap_or_else(|e| panic!("{}: {e}", form.name));
        for &theme in Theme::all() {
            let opts = Options {
                theme,
                ..Options::default()
            };
            let scene = engine
                .scene(&intent, &opts)
                .unwrap_or_else(|e| panic!("{}: {e}", form.name));
            let png = engine.png(&scene, 1.0).expect("the card should draw");
            assert_golden(&format!("{}-{}", form.name, theme.name()), &png);
            drawn += 1;
        }
    }
    assert_eq!(drawn, Theme::all().len() * engine.forms().len());
}

/// The worst cases' names, `<form>-<case>`, sorted, after checking every
/// form has a `<form>-worst` case.
fn worst_case_names(engine: &Engine) -> Vec<String> {
    let dir = worst_case_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the worst cases should be readable")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for form in engine.forms() {
        assert!(
            names.contains(&format!("{}-worst", form.name)),
            "{}: no worst case in {}",
            form.name,
            dir.display()
        );
    }
    names
}

fn worst_case_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/intents/worst-case")
}

fn worst_case_json(name: &str) -> String {
    std::fs::read_to_string(worst_case_dir().join(format!("{name}.json"))).unwrap()
}

/// The worst cases a form can be sent, drawn for a person to look at
/// (break-ui's catalog): a title past the 44-character wrap, labels in CJK,
/// Arabic, emoji and a ZWJ sequence, a 30-letter word, and values from
/// -1,284 to 12,345,678.9. The intents are in `tests/intents/worst-case/`,
/// named `<form>-<case>`; each draws in the default theme, or is refused
/// with the message [`WORST_REFUSED`] records for it. They draw with Inter
/// alone, as on a machine without the macOS system fonts, so these
/// baselines hold on every machine; the next test draws them with the
/// system fonts (ADR-050).
#[test]
fn every_worst_case_matches_its_baseline_or_its_refusal() {
    let engine = Engine::new()
        .and_then(Engine::inter_only)
        .expect("the embedded assets should load");
    let opts = Options::default();
    let mut failures = Vec::new();
    for name in &worst_case_names(&engine) {
        let drawn = tiles_intent::parse(&worst_case_json(name))
            .map_err(tiles_engine::RenderError::from)
            .and_then(|intent| engine.scene(&intent, &opts));
        let refused = WORST_REFUSED.iter().find(|(n, _)| n == name).map(|r| r.1);
        match (drawn, refused) {
            (Ok(scene), None) => {
                let png = engine.png(&scene, 1.0).expect("the card should draw");
                assert_golden(&format!("worst-case/{name}-{}", opts.theme.name()), &png);
            }
            (Err(e), Some(want)) if e.to_string() == want => {}
            (Err(e), _) => failures.push(format!("{name}: refused: {e}")),
            (Ok(_), Some(_)) => failures.push(format!("{name}: drew, but is recorded as refused")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The worst cases holding a character Inter lacks (CJK, Arabic, emoji),
/// drawn with the macOS system fonts behind Inter (ADR-050), against
/// `worst-case-macos/`. Those fonts change between macOS releases, so only a
/// machine whose fonts match the ones the baselines were drawn with,
/// recorded in `worst-case-macos/fonts.txt`, checks them. Any other machine,
/// Linux CI among them, says so and checks nothing here; the test above
/// still checks the same cases drawn with Inter alone.
#[test]
fn worst_cases_beyond_inter_match_their_macos_baselines() {
    let engine = Engine::new().expect("the embedded assets should load");
    if engine.system_fonts().is_empty() {
        eprintln!(
            "no macOS system fonts here: worst cases beyond Inter are checked with Inter alone"
        );
        return;
    }
    let fonts = engine
        .system_fonts()
        .iter()
        .fold(String::new(), |mut out, f| {
            let _ = writeln!(out, "{} {} {}", f.family, f.weight, f.head);
            out
        });
    let record = golden_dir().join("worst-case-macos/fonts.txt");
    if std::env::var("TILES_BLESS").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(record.parent().expect("a record has a directory"))
            .expect("the baseline directory should be writable");
        std::fs::write(&record, &fonts).expect("the font record should be writable");
    } else if std::fs::read_to_string(&record).unwrap_or_default() != fonts {
        eprintln!(
            "this machine's system fonts differ from those in {}: worst cases beyond Inter \
             are checked with Inter alone. This machine has:\n{fonts}",
            record.display()
        );
        return;
    }
    let opts = Options::default();
    let mut drawn = 0;
    for name in &worst_case_names(&engine) {
        let json = worst_case_json(name);
        if !engine.beyond_inter(&json) {
            continue;
        }
        let Ok(scene) = tiles_intent::parse(&json)
            .map_err(tiles_engine::RenderError::from)
            .and_then(|intent| engine.scene(&intent, &opts))
        else {
            continue;
        };
        let png = engine.png(&scene, 1.0).expect("the card should draw");
        assert_golden(
            &format!("worst-case-macos/{name}-{}", opts.theme.name()),
            &png,
        );
        drawn += 1;
    }
    assert!(drawn > 0, "no worst case holds a character beyond Inter");
}

/// Every form refuses a worst case sent with no rows.
#[test]
fn every_worst_case_with_no_rows_is_refused() {
    for entry in std::fs::read_dir(worst_case_dir()).unwrap() {
        let path = entry.unwrap().path();
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        v["data"]["rows"] = serde_json::json!([]);
        let e = tiles_intent::parse(&v.to_string()).expect_err("no rows should be refused");
        assert_eq!(e.to_string(), NO_ROWS, "{}", path.display());
    }
}

/// The refusal an intent with no rows gets.
const NO_ROWS: &str = "data.rows: is empty; send at least one row";

/// The worst cases a form refuses today, with the refusal each gets.
const WORST_REFUSED: &[(&str, &str)] = &[
    (
        "slope-one-row",
        "data.rows: slope needs at least two periods in x; add rows for a second period",
    ),
    (
        "stacked-100-bar-negative",
        "data.rows: stacked-100-bar needs values of zero or more; use grouped-bar for signed \
         values",
    ),
    (
        "stacked-bar-negative",
        "data.rows: stacked-bar needs values of zero or more; use grouped-bar for signed \
         values",
    ),
];

/// The CLI's default: scale 2, crisp enough for a `LinkedIn` post, in the
/// default theme.
#[test]
fn column_at_scale_two_matches_its_baseline() {
    let engine = Engine::new().unwrap();
    let png = engine
        .render_json(&intent_json("column"), &Options::default(), 2.0)
        .unwrap();
    assert_golden(&format!("column-{}@2x", Theme::default().name()), &png);
}

/// The same intent draws the same bytes every time (ADR-002).
#[test]
fn rendering_is_deterministic() {
    let engine = Engine::new().unwrap();
    let json = intent_json("multi-line");
    let a = engine.render_json(&json, &Options::default(), 2.0).unwrap();
    let b = Engine::new()
        .unwrap()
        .render_json(&json, &Options::default(), 2.0)
        .unwrap();
    assert!(a == b, "two renders of one intent differ");
}

fn intent_json(form: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/intents")
        .join(format!("{form}.json"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no intent at {}: {e}", path.display()))
}
