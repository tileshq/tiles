//! The colour checks of the design Obsidian was ported from, applied to
//! every theme the build carries (ADR-021, ADR-036): every categorical
//! colour stands at least 3:1 on the card, so none needs a label to be seen
//! ("Needs labels: none" for Obsidian in that design), with no exemption
//! (ADR-042); text stands at least 4.5:1; and every two
//! categorical colours stay apart for readers with deuteranopia or
//! protanopia. That design simulates both with Machado, Oliveira and
//! Fernandes (2009) at severity 1 in linear sRGB, and the pairs it flags as
//! too close measure 8.0 to 9.5 in CIEDE2000, so a pair here must measure 10
//! or more. The context greys of the lines beside a named line stand 3:1 too,
//! stay 8 apart and carry no colour (ADR-047); no house or plain category
//! is red, orange or magenta, each replaced one stays 10 apart from its
//! neighbours with tritanopia too, no category sits near a context grey,
//! and no series takes a text or axis colour.
// The formulas keep the published papers' one-letter names and averages.
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::manual_midpoint
)]

use tiles_engine::color::Rgba;
use tiles_engine::tokens::Palette;
use tiles_engine::{Engine, Theme, Tokens};

const MACHADO_DEUTAN: [[f64; 3]; 3] = [
    [0.367_322, 0.860_646, -0.227_968],
    [0.280_085, 0.672_501, 0.047_413],
    [-0.011_820, 0.042_940, 0.968_881],
];
const MACHADO_PROTAN: [[f64; 3]; 3] = [
    [0.152_286, 1.052_583, -0.204_868],
    [0.114_503, 0.786_281, 0.099_216],
    [-0.003_882, -0.048_116, 1.051_998],
];
const MACHADO_TRITAN: [[f64; 3]; 3] = [
    [1.255_528, -0.076_749, -0.178_779],
    [-0.078_411, 0.930_809, 0.147_602],
    [0.004_733, 0.691_367, 0.303_900],
];
/// Least CIEDE2000 distance between two categorical colours as a reader
/// with colour blindness sees them.
const MIN_SEPARATION: f64 = 10.0;
/// Least CIEDE2000 distance between two context greys, with normal vision
/// and with each colour blindness (ADR-047).
const CONTEXT_SEPARATION: f64 = 8.0;
/// Most CIELAB chroma a context grey carries, so the accent alone has
/// colour.
const CONTEXT_CHROMA: f64 = 10.0;

fn linear(c: Rgba) -> [f64; 3] {
    [c.r, c.g, c.b].map(|v| {
        let v = f64::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
}

fn luminance(c: Rgba) -> f64 {
    let [r, g, b] = linear(c);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio.
fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (hi, lo) = {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y), x.min(y))
    };
    (hi + 0.05) / (lo + 0.05)
}

/// The CIELAB lightness, chroma and hue angle of `c` as drawn.
fn lch(c: Rgba) -> (f64, f64, f64) {
    let [l, a, b] = lab(linear(c));
    (l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0))
}

fn simulate(c: Rgba, m: &[[f64; 3]; 3]) -> [f64; 3] {
    let l = linear(c);
    m.map(|row| (row[0] * l[0] + row[1] * l[1] + row[2] * l[2]).clamp(0.0, 1.0))
}

/// CIELAB (D65) from linear sRGB.
fn lab(l: [f64; 3]) -> [f64; 3] {
    let x = (0.4124 * l[0] + 0.3576 * l[1] + 0.1805 * l[2]) / 0.950_47;
    let y = 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
    let z = (0.0193 * l[0] + 0.1192 * l[1] + 0.9505 * l[2]) / 1.088_83;
    let f = |t: f64| {
        if t > 0.008_856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 colour difference (Sharma, Wu and Dalal 2005).
fn ciede2000(p: [f64; 3], q: [f64; 3]) -> f64 {
    let [l1, a1, b1] = p;
    let [l2, a2, b2] = q;
    let c_bar = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
    let g = 0.5 * (1.0 - (c_bar.powi(7) / (c_bar.powi(7) + 25f64.powi(7))).sqrt());
    let (a1p, a2p) = (a1 * (1.0 + g), a2 * (1.0 + g));
    let (c1p, c2p) = (a1p.hypot(b1), a2p.hypot(b2));
    let hue = |b: f64, a: f64| b.atan2(a).to_degrees().rem_euclid(360.0);
    let (h1, h2) = (hue(b1, a1p), hue(b2, a2p));
    let dl = l2 - l1;
    let dc = c2p - c1p;
    let dh = if c1p * c2p == 0.0 {
        0.0
    } else if (h2 - h1).abs() <= 180.0 {
        h2 - h1
    } else if h2 > h1 {
        h2 - h1 - 360.0
    } else {
        h2 - h1 + 360.0
    };
    let dh_big = 2.0 * (c1p * c2p).sqrt() * (dh / 2.0).to_radians().sin();
    let l_bar = (l1 + l2) / 2.0;
    let cp_bar = (c1p + c2p) / 2.0;
    let h_bar = if c1p * c2p == 0.0 {
        h1 + h2
    } else if (h1 - h2).abs() <= 180.0 {
        (h1 + h2) / 2.0
    } else if h1 + h2 < 360.0 {
        (h1 + h2 + 360.0) / 2.0
    } else {
        (h1 + h2 - 360.0) / 2.0
    };
    let cos = |d: f64| d.to_radians().cos();
    let t =
        1.0 - 0.17 * cos(h_bar - 30.0) + 0.24 * cos(2.0 * h_bar) + 0.32 * cos(3.0 * h_bar + 6.0)
            - 0.20 * cos(4.0 * h_bar - 63.0);
    let d_theta = 30.0 * (-((h_bar - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cp_bar.powi(7) / (cp_bar.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (l_bar - 50.0).powi(2) / (20.0 + (l_bar - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cp_bar;
    let sh = 1.0 + 0.015 * cp_bar * t;
    let rt = -(2.0 * d_theta).to_radians().sin() * rc;
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dh_big / sh).powi(2) + rt * (dc / sc) * (dh_big / sh))
        .sqrt()
}

#[test]
fn the_colour_maths_matches_published_values() {
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let black = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    assert!((contrast(white, black) - 21.0).abs() < 1e-9);
    // Sharma, Wu and Dalal's test pair 1.
    let d = ciede2000([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485]);
    assert!((d - 2.0425).abs() < 1e-4, "{d}");
}

/// Every way `p` misses the contrast checks.
fn contrast_failures(p: &Palette) -> Vec<String> {
    let theme = &p.name;
    let mut failures = Vec::new();
    for (i, c) in p.categorical.iter().enumerate() {
        let r = contrast(*c, p.card);
        if r < 3.0 {
            failures.push(format!("{theme}: cat_{} is {r:.2}:1 on the card", i + 1));
        }
    }
    for (i, c) in p.context.iter().enumerate() {
        let r = contrast(*c, p.card);
        if r < 3.0 {
            failures.push(format!(
                "{theme}: context_{} is {r:.2}:1 on the card",
                i + 1
            ));
        }
    }
    for (name, c) in [("accent", p.accent), ("bad", p.bad)] {
        let r = contrast(c, p.card);
        if r < 3.0 {
            failures.push(format!("{theme}: {name} is {r:.2}:1 on the card"));
        }
    }
    for (name, c) in [("ink", p.ink), ("muted", p.muted)] {
        let r = contrast(c, p.card);
        if r < 4.5 {
            failures.push(format!("{theme}: {name} text is {r:.2}:1 on the card"));
        }
    }
    failures
}

#[test]
fn every_theme_passes_the_contrast_checks() {
    let e = Engine::new().unwrap();
    for p in e.tokens().themes.all() {
        let failures = contrast_failures(p);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

#[test]
fn a_light_with_a_grey_under_three_to_one_fails() {
    let e = Engine::new().unwrap();
    let mut light = e.tokens().themes.get("light").unwrap().clone();
    light.categorical[2] = Rgba::parse("#a0a8b2").unwrap();
    let r = contrast(light.categorical[2], light.card);
    assert!(r < 3.0);
    assert_eq!(
        contrast_failures(&light),
        [format!("light: cat_3 is {r:.2}:1 on the card")]
    );
}

#[test]
fn a_context_grey_under_three_to_one_fails() {
    let e = Engine::new().unwrap();
    let mut light = e.tokens().themes.get("light").unwrap().clone();
    let last = light.context.len() - 1;
    light.context[last] = Rgba::parse("#a0a8b2").unwrap();
    let r = contrast(light.context[last], light.card);
    assert!(r < 3.0);
    assert_eq!(
        contrast_failures(&light),
        [format!(
            "light: context_{} is {r:.2}:1 on the card",
            last + 1
        )]
    );
}

/// ADR-042: no theme lists colours the contrast check lets pass.
#[test]
fn tokens_refuse_a_list_of_colours_under_three_to_one() {
    let mut json: serde_json::Value =
        serde_json::from_str(&tiles_engine::embedded_tokens().unwrap()).unwrap();
    let first = json["themes"]["list"][0]["categorical"][0].clone();
    json["themes"]["list"][0]["under_three_to_one"] =
        serde_json::json!([{"slot": 1, "colour": first}]);
    let err = Tokens::from_json(&json.to_string()).unwrap_err();
    assert!(err.contains("under_three_to_one"), "{err}");
}

#[test]
fn every_themes_categories_stay_apart_for_colour_blind_readers() {
    let e = Engine::new().unwrap();
    for p in e.tokens().themes.all() {
        let cat = &p.categorical;
        for (kind, m) in [
            ("deuteranopia", &MACHADO_DEUTAN),
            ("protanopia", &MACHADO_PROTAN),
        ] {
            for i in 0..cat.len() {
                for j in i + 1..cat.len() {
                    let d = ciede2000(lab(simulate(cat[i], m)), lab(simulate(cat[j], m)));
                    assert!(
                        d >= MIN_SEPARATION,
                        "{}: cat_{} and cat_{} are {d:.1} apart with {kind}",
                        p.name,
                        i + 1,
                        j + 1
                    );
                }
            }
        }
    }
}

/// The themes of a tokens file under `assets/`, or none when the tree lacks
/// it: the house file leaves the public tree at the split (ADR-037).
fn themes_in(file: &str) -> Vec<Palette> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(file);
    std::fs::read_to_string(path).map_or_else(
        |_| Vec::new(),
        |json| Tokens::from_json(&json).unwrap().themes.list,
    )
}

/// The house and the plain themes, both checked whichever one the build
/// seals.
fn house_and_plain_themes() -> Vec<(&'static str, Palette)> {
    ["tokens.json", "plain/tokens.json"]
        .into_iter()
        .flat_map(|file| themes_in(file).into_iter().map(move |p| (file, p)))
        .collect()
}

/// ADR-047: the categorical slots, counted from 1, that took a new colour,
/// by tokens file and theme.
const REPLACED_SLOTS: [(&str, &str, &[usize]); 5] = [
    ("tokens.json", "obsidian", &[3, 4]),
    ("tokens.json", "light", &[4]),
    ("tokens.json", "dark", &[2, 4]),
    ("plain/tokens.json", "dark", &[3, 4]),
    ("plain/tokens.json", "light", &[3, 4]),
];

/// ADR-047: a replaced slot stays apart from the two slots beside it for
/// readers with tritanopia too.
#[test]
fn each_replaced_category_stays_apart_from_its_neighbours_with_tritanopia() {
    for (file, p) in house_and_plain_themes() {
        let (_, _, slots) = REPLACED_SLOTS
            .iter()
            .find(|(f, name, _)| *f == file && *name == p.name)
            .unwrap_or_else(|| panic!("{file}: {} lists no replaced slots", p.name));
        let cat = &p.categorical;
        for &s in *slots {
            for n in [s - 1, s + 1] {
                if n == 0 || n > cat.len() {
                    continue;
                }
                let d = ciede2000(
                    lab(simulate(cat[s - 1], &MACHADO_TRITAN)),
                    lab(simulate(cat[n - 1], &MACHADO_TRITAN)),
                );
                assert!(
                    d >= MIN_SEPARATION,
                    "{file} {}: cat_{s} and cat_{n} are {d:.1} apart with tritanopia",
                    p.name
                );
            }
        }
    }
}

/// ADR-047: red means a bad finding (K2), so no house or plain category is
/// red or reads as one: no colour of hue angle under 78 degrees (red,
/// orange) or over 312 (magenta, pink), unless it is a grey.
#[test]
fn no_category_is_red_orange_or_magenta() {
    for (file, p) in house_and_plain_themes() {
        for (i, c) in p.categorical.iter().enumerate() {
            let (_, chroma, hue) = lch(*c);
            assert!(
                chroma < CONTEXT_CHROMA || (78.0..=312.0).contains(&hue),
                "{file} {}: cat_{} {} has hue {hue:.0}, chroma {chroma:.0}",
                p.name,
                i + 1,
                c.hex()
            );
        }
    }
}

/// ADR-042's categorical greys, counted from 1, that sit within
/// `CONTEXT_SEPARATION` of a context grey of their theme (ADR-047).
const CATEGORIES_NEAR_CONTEXT: [(&str, &str, &[usize]); 2] = [
    ("tokens.json", "light", &[2, 3, 5]),
    ("tokens.json", "dark", &[2, 3, 5]),
];

/// ADR-047: a category stands apart from every context grey of its theme,
/// so it never reads as a line beside a named one; only the listed house
/// greys may not, and each of them still does not.
#[test]
fn no_category_reads_as_a_context_grey() {
    for (file, p) in house_and_plain_themes() {
        let near: &[usize] = CATEGORIES_NEAR_CONTEXT
            .iter()
            .find(|(f, name, _)| *f == file && *name == p.name)
            .map_or(&[], |(_, _, slots)| slots);
        for (i, c) in p.categorical.iter().enumerate() {
            let d = p
                .context
                .iter()
                .map(|g| ciede2000(lab(linear(*c)), lab(linear(*g))))
                .fold(f64::INFINITY, f64::min);
            if near.contains(&(i + 1)) {
                assert!(
                    d < CONTEXT_SEPARATION,
                    "{file} {}: cat_{} stands {d:.1} from the context greys; drop it from the list",
                    p.name,
                    i + 1
                );
            } else {
                assert!(
                    d >= CONTEXT_SEPARATION,
                    "{file} {}: cat_{} is {d:.1} from a context grey",
                    p.name,
                    i + 1
                );
            }
        }
    }
}

/// ADR-047: a series never draws in its theme's text or axis colour, so a
/// line is not read as a tick or a label.
#[test]
fn no_series_colour_is_a_text_or_axis_colour() {
    for (file, p) in house_and_plain_themes() {
        let text_and_axis = [
            ("ink", p.ink),
            ("muted", p.muted),
            ("grid", p.grid),
            ("line", p.line),
            ("track", p.track),
        ];
        let series = [p.accent, p.grey]
            .into_iter()
            .chain(p.categorical.iter().copied())
            .chain(p.neutrals.iter().copied())
            .chain(p.context.iter().copied());
        for c in series {
            for (name, t) in text_and_axis {
                assert_ne!(c, t, "{file} {}: a series draws in {name}", p.name);
            }
        }
    }
}

#[test]
fn every_themes_context_greys_stay_apart_and_carry_no_colour() {
    let e = Engine::new().unwrap();
    for p in e.tokens().themes.all() {
        let ctx = &p.context;
        assert!(
            ctx.len() >= 4,
            "{}: a five-line chart's four context lines",
            p.name
        );
        for (i, c) in ctx.iter().enumerate() {
            let (_, chroma, _) = lch(*c);
            assert!(
                chroma < CONTEXT_CHROMA,
                "{}: context_{} has chroma {chroma:.1}",
                p.name,
                i + 1
            );
        }
        assert!(
            lch(p.accent).1 >= 2.0 * CONTEXT_CHROMA,
            "{}: the accent has colour to lead the greys",
            p.name
        );
        for (kind, m) in [
            ("normal vision", None),
            ("deuteranopia", Some(&MACHADO_DEUTAN)),
            ("protanopia", Some(&MACHADO_PROTAN)),
            ("tritanopia", Some(&MACHADO_TRITAN)),
        ] {
            let seen = |c: Rgba| lab(m.map_or_else(|| linear(c), |m| simulate(c, m)));
            for i in 0..ctx.len() {
                for j in i + 1..ctx.len() {
                    let d = ciede2000(seen(ctx[i]), seen(ctx[j]));
                    assert!(
                        d >= CONTEXT_SEPARATION,
                        "{}: context_{} and context_{} are {d:.1} apart with {kind}",
                        p.name,
                        i + 1,
                        j + 1
                    );
                }
            }
        }
    }
}

#[test]
fn every_theme_is_listed_once_and_the_default_is_one_of_them() {
    let e = Engine::new().unwrap();
    let names: Vec<&str> = Theme::all().iter().map(|t| t.name()).collect();
    let listed: Vec<&str> = e.tokens().themes.all().map(|p| p.name.as_str()).collect();
    assert_eq!(names, listed);
    assert!(names.contains(&Theme::default().name()));
    assert_eq!(Theme::default().name(), e.tokens().themes.default);
}

/// ADR-037: the plain theme's colours were written fresh, so none of them is
/// a house colour. The house tokens leave the public tree at the split, and
/// the test with them.
#[test]
fn the_plain_theme_shares_no_colour_with_the_house_tokens() {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let Ok(house) = std::fs::read_to_string(assets.join("tokens.json")) else {
        return;
    };
    let plain = std::fs::read_to_string(assets.join("plain/tokens.json")).unwrap();
    let colours = |json: &str| -> Vec<Rgba> {
        let tokens = Tokens::from_json(json).unwrap();
        tokens
            .themes
            .all()
            .flat_map(|p| {
                [
                    p.card, p.line, p.ink, p.muted, p.grey, p.accent, p.bad, p.bad_soft, p.grid,
                    p.track,
                ]
                .into_iter()
                .chain(p.categorical.iter().copied())
                .chain(p.neutrals.iter().copied())
                .chain(p.context.iter().copied())
                .collect::<Vec<_>>()
            })
            .collect()
    };
    let house = colours(&house);
    let shared: Vec<String> = colours(&plain)
        .into_iter()
        .filter(|c| house.contains(c))
        .map(Rgba::hex)
        .collect();
    assert!(
        shared.is_empty(),
        "plain colours from the house file: {shared:?}"
    );
}

/// ADR-037: a build from source carries the plain theme's dark and light,
/// and draws in dark unless the user picks light.
#[test]
fn a_build_from_source_carries_plain_dark_and_light_with_dark_the_default() {
    if tiles_engine::SEAL_SOURCE != "assets" {
        return; // Built from a design pack: its themes are the pack's.
    }
    let names: Vec<&str> = Theme::all().iter().map(|t| t.name()).collect();
    assert_eq!(names, ["dark", "light"]);
    assert_eq!(Theme::default().name(), "dark");
}
