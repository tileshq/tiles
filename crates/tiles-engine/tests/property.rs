//! Property tests ADR-009 promises for the engine: every form's fixture with
//! its rows mutated at random (odd numbers, duplicate and missing rows, wide,
//! long and look-alike labels, a moved highlight) either draws or is refused.
//! Nothing panics, every drawn mark sits at finite coordinates, and no drawn
//! text holds a character that can drive a terminal or hide text, other than
//! a zero width joiner between two emoji (ADR-050). The seeds
//! take the build's themes in turn, so each theme draws a share. A small
//! deterministic generator keeps the crate free of new dependencies and names
//! the seed of any failure.

use std::path::PathBuf;

use serde_json::Value;
use tiles_engine::{Engine, Options, Theme};
use tiles_intent::text::first_refused;

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }
    fn pick<'a, T>(&mut self, from: &'a [T]) -> &'a T {
        &from[self.below(from.len())]
    }
}

/// Numbers chosen to break layout: signs, zero, the smallest subnormal,
/// the largest magnitude an intent accepts and just past it, and values
/// that sum or step to awkward totals.
const NUMBERS: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    5e-324,
    1e-9,
    999.6,
    999_999.6,
    1e9,
    -1e9,
    1e15,
    -1e15,
    1.000_001e15,
    123_456_789.123,
    42.0,
];

/// Labels chosen to break text layout and the look-alike rule: empty, long,
/// wide, combining, right-to-left, emoji, and near-duplicates.
const LABELS: &[&str] = &[
    "",
    "A",
    "Q1",
    "Q1 ",
    "Acme Corporation International Holdings Limited Partnership",
    "Acme Corporation International Holdings Limited Partners",
    "日本語のラベル",
    "e\u{301}",
    "שלום",
    "✅ done",
    "👩‍💻",
    "a·b·c",
    "-",
    "0",
];

fn fixtures() -> Vec<(String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/intents");
    let mut out: Vec<(String, Value)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| {
            let v = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            (p.file_stem().unwrap().to_string_lossy().into_owned(), v)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn mutate(rng: &mut XorShift, intent: &Value) -> Value {
    let mut v = intent.clone();
    let Some(rows) = v["data"]["rows"].as_array_mut() else {
        return v;
    };
    for _ in 0..rng.below(4) {
        match rng.below(4) {
            0 if !rows.is_empty() => {
                let at = rng.below(rows.len());
                rows.remove(at);
            }
            1 if !rows.is_empty() => {
                let at = rng.below(rows.len());
                let copy = rows[at].clone();
                rows.push(copy);
            }
            _ => {}
        }
    }
    for row in rows.iter_mut() {
        let Some(cells) = row.as_object_mut() else {
            continue;
        };
        for cell in cells.values_mut() {
            if rng.below(3) != 0 {
                continue;
            }
            *cell = match &*cell {
                Value::Number(_) => match rng.below(8) {
                    0 => Value::Null,
                    _ => serde_json::json!(*rng.pick(NUMBERS)),
                },
                Value::String(_) => Value::String((*rng.pick(LABELS)).to_string()),
                other => other.clone(),
            };
        }
    }
    if rng.below(4) == 0 {
        let label = *rng.pick(LABELS);
        v["highlight"] = serde_json::json!([label]);
    }
    v
}

#[test]
fn mutated_intents_draw_or_are_refused_and_never_panic() {
    let engine = Engine::new().expect("the embedded assets should load");
    let themes = Theme::all();
    let mut drawn = 0;
    for (form, fixture) in fixtures() {
        for seed in 1..=80u64 {
            let opts = Options {
                theme: themes[usize::try_from(seed).unwrap() % themes.len()],
                ..Options::default()
            };
            let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let v = mutate(&mut rng, &fixture);
            let Ok(intent) = tiles_intent::parse(&v.to_string()) else {
                continue;
            };
            let Ok(scene) = engine.scene(&intent, &opts) else {
                continue;
            };
            drawn += 1;
            for item in &scene.items {
                let b = item.bounds();
                assert!(
                    [b.left, b.top, b.right, b.bottom]
                        .iter()
                        .all(|n| n.is_finite()),
                    "{form} seed {seed}: a {:?} mark at {b:?}",
                    item.part
                );
                if let Some(text) = item.text() {
                    assert!(
                        first_refused(text).is_none(),
                        "{form} seed {seed}: {text:?}"
                    );
                }
            }
            // Rasterising is the slow step, so one mutation in eight is drawn.
            if seed % 8 == 0 {
                let _ = engine.png(&scene, 1.0);
            }
        }
    }
    assert!(
        drawn > 100,
        "only {drawn} mutations drew; the generator is too harsh"
    );
}
