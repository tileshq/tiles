//! Playbook M6: level strokes land on whole device pixels at every scale, so
//! a connector or a gridline draws as solid rows instead of a soft smear of
//! two solid rows and two faint ones. Each test draws in every theme the
//! build carries (ADR-037).

use std::path::PathBuf;

use tiles_engine::color::Rgba;
use tiles_engine::scene::{Part, Shape};
use tiles_engine::{Engine, Entrance, Moment, Options, Scene, Theme};

fn scene(engine: &Engine, form: &str, theme: Theme) -> Scene {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/intents")
        .join(format!("{form}.json"));
    let intent = tiles_intent::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let opts = Options {
        theme,
        ..Options::default()
    };
    engine.scene(&intent, &opts).unwrap()
}

/// How far each device row in column `x` around `y` sits between the card
/// colour (0) and the stroke colour (1).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)] // Pixel coordinates and channel counts on a small card.
fn coverage(engine: &Engine, scene: &Scene, scale: f32, x: f32, y: f32, ink: Rgba) -> Vec<f32> {
    let frame = engine.frame_at(scene, scale, Moment::FINAL).unwrap();
    let card = engine.tokens().palette(scene.theme).card;
    let px = (x * scale) as i32;
    let cy = (y * scale) as i32;
    let span = (4.0 * scale).ceil() as i32 + 2;
    (cy - span..=cy + span)
        .map(|row| {
            let i = ((row * frame.width + px) * 4) as usize;
            let p = &frame.rgba[i..i + 3];
            let along = |c: usize, a: u8, b: u8| {
                let (a, b) = (f32::from(a), f32::from(b));
                if (b - a).abs() < 1.0 {
                    None
                } else {
                    Some((f32::from(p[c]) - a) / (b - a))
                }
            };
            let parts: Vec<f32> = [(0, card.r, ink.r), (1, card.g, ink.g), (2, card.b, ink.b)]
                .into_iter()
                .filter_map(|(c, a, b)| along(c, a, b))
                .collect();
            parts.iter().sum::<f32>() / parts.len() as f32
        })
        .collect()
}

/// Asserts each row is untouched or solid, and that some row is solid.
fn assert_crisp(what: &str, rows: &[f32]) {
    let soft: Vec<String> = rows
        .iter()
        .filter(|c| **c > 0.08 && **c < 0.92)
        .map(|c| format!("{c:.2}"))
        .collect();
    assert!(
        soft.is_empty(),
        "M6 {what}: half-covered rows {soft:?} in {rows:.2?}"
    );
    assert!(
        rows.iter().any(|c| *c >= 0.92),
        "M6 {what}: the stroke is missing from {rows:.2?}"
    );
}

const SCALES: [f32; 5] = [1.0, 1.5, 2.0, 2.5, 3.0];

#[test]
fn m6_dumbbell_connectors_draw_as_solid_rows_at_every_scale() {
    let engine = Engine::new().unwrap();
    for &theme in Theme::all() {
        let s = scene(&engine, "dumbbell", theme);
        let mut checked = 0;
        for item in s.part(Part::Guide) {
            let Shape::Line { points, .. } = &item.shape else {
                continue;
            };
            let ((x0, y), (x1, _)) = (points[0], points[points.len() - 1]);
            // Between the two dots, clear of both.
            if (x1 - x0).abs() < 30.0 {
                continue;
            }
            let x = f32::midpoint(x0, x1);
            for scale in SCALES {
                let rows = coverage(&engine, &s, scale, x, y, item.color);
                assert_crisp(&format!("dumbbell connector at {scale}x"), &rows);
            }
            checked += 1;
        }
        assert!(checked > 0, "M6: the dumbbell fixture has a long connector");
    }
}

#[test]
fn m6_gridlines_draw_as_one_solid_row_at_every_scale() {
    let engine = Engine::new().unwrap();
    for &theme in Theme::all() {
        gridlines_are_solid(&engine, theme);
    }
}

fn gridlines_are_solid(engine: &Engine, theme: Theme) {
    let s = scene(engine, "line", theme);
    let grid = s.part(Part::Grid).nth(1).expect("M6: a gridline");
    let Shape::Line { points, .. } = &grid.shape else {
        panic!("a gridline is a line")
    };
    // Left of the first point, so no data line crosses the sample.
    let (x, y) = (points[0].0 + 2.0, points[0].1);
    for scale in SCALES {
        let rows = coverage(engine, &s, scale, x, y, grid.color);
        assert_crisp(&format!("{theme:?} gridline at {scale}x"), &rows);
        let solid = rows.iter().filter(|c| **c >= 0.92).count();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let want = scale.round().max(1.0) as usize;
        assert_eq!(
            solid, want,
            "M6: a 1 px gridline is {want} device rows at {scale}x"
        );
    }
}

/// A board draws each card as the card draws alone, offset by whole device
/// pixels, so a row of cards is as crisp as one card.
#[test]
fn a_board_draws_each_card_as_it_draws_alone() {
    let engine = Engine::new().unwrap();
    for &theme in Theme::all() {
        board_draws_each_card_alone(&engine, theme);
    }
}

fn board_draws_each_card_alone(engine: &Engine, theme: Theme) {
    let card = scene(engine, "big-number", theme);
    let alone = engine.png(&card, 2.0).unwrap();
    let one = tiles_engine::Board {
        width: card.width,
        height: card.height,
        tiles: vec![tiles_engine::Placed {
            scene: card.clone(),
            x: 0.0,
            y: 0.0,
        }],
    };
    assert_eq!(engine.board_png(&one, 2.0).unwrap(), alone);

    let gap = 16.0;
    let two = tiles_engine::Board {
        width: 2.0 * card.width + gap,
        tiles: vec![
            one.tiles[0].clone(),
            tiles_engine::Placed {
                scene: card.clone(),
                x: card.width + gap,
                y: 0.0,
            },
        ],
        ..one
    };
    let decode = |png: &[u8]| {
        let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(png)).unwrap();
        let info = skia_safe::ImageInfo::new(
            (image.width(), image.height()),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Unpremul,
            None,
        );
        let row = usize::try_from(image.width()).unwrap() * 4;
        let mut px = vec![0u8; row * usize::try_from(image.height()).unwrap()];
        assert!(image.read_pixels(
            &info,
            &mut px,
            row,
            (0, 0),
            skia_safe::image::CachingHint::Allow
        ));
        (row, px)
    };
    let (row, card_px) = decode(&alone);
    let (board_row, board_px) = decode(&engine.board_png(&two, 2.0).unwrap());
    // The right card starts one card and the 16 px gap, at scale 2, along.
    let shift = row + 16 * 2 * 4;
    // Anti-aliasing further right may shift a corner pixel's coverage by a
    // few levels, as where a small corner's shadow meets its border; a card
    // off by a fraction of a pixel moves an edge pixel by a hundred levels
    // or more. The channels compare premultiplied, as they show: a pixel of
    // faint shadow a few levels apart there can be thirty apart
    // unpremultiplied.
    let shown = |px: &[u8]| {
        let a = u16::from(px[3]);
        [px[0], px[1], px[2]].map(|c| (u16::from(c) * a + 127) / 255)
    };
    let near = |a: &[u8], b: &[u8]| {
        a.chunks(4).zip(b.chunks(4)).all(|(p, q)| {
            p[3].abs_diff(q[3]) <= 8
                && shown(p)
                    .iter()
                    .zip(shown(q))
                    .all(|(x, y)| x.abs_diff(y) <= 8)
        })
    };
    for (y, line) in card_px.chunks(row).enumerate() {
        let at = y * board_row;
        assert_eq!(&board_px[at..at + row], line, "left card, row {y}");
        assert!(
            near(&board_px[at + shift..at + shift + row], line),
            "{theme:?}: right card, row {y}"
        );
    }

    // The entrance of the row: the cards one after another, left to right,
    // each frame the board's size, from nothing to almost settled; the
    // settled picture is not among them.
    let frames = engine.board_entrance(&two, 2.0, Entrance::Full).unwrap();
    // 450 ms and one 50 ms stagger, 50 ms apart: 10 frames and the settled
    // picture.
    assert_eq!(engine.board_frame_count(&two, Entrance::Full), 11);
    assert_eq!(frames.len(), 10);
    let (_, first) = decode(&frames[0]);
    assert_eq!(first.len(), board_px.len());
    assert!(first.chunks(4).all(|p| p[3] == 0), "frame 0 is transparent");
    let (_, late) = decode(&frames[9]);
    assert_eq!(late.len(), board_px.len());
    assert_ne!(late, board_px, "the last entrance frame is not yet settled");
    let ink = |px: &[u8]| px.chunks(4).filter(|p| p[3] > 0).count();
    assert!(ink(&late) > ink(&board_px) * 9 / 10, "and is nearly there");
    // The right card starts 50 ms, one frame, after the left: on the second
    // frame the left card shows and the right has not begun.
    assert!((engine.board_entrance_ms(&two, Entrance::Full) - 500.0).abs() < 0.01);
    let alpha = |px: &[u8], left: bool| {
        px.chunks(board_row)
            .map(|line| {
                let half = if left {
                    &line[..row]
                } else {
                    &line[shift..shift + row]
                };
                half.chunks(4).map(|p| u64::from(p[3])).sum::<u64>()
            })
            .sum::<u64>()
    };
    let (_, early) = decode(&frames[1]);
    assert!(
        alpha(&early, true) > alpha(&early, false) * 3 / 2,
        "{} {}",
        alpha(&early, true),
        alpha(&early, false)
    );
}

/// A dashboard's heading is its title and note with no card box: the
/// picture is transparent but for the letters.
#[test]
fn a_heading_draws_text_with_no_card_box() {
    let engine = Engine::new().unwrap();
    for &theme in Theme::all() {
        heading_has_no_card_box(&engine, theme);
    }
}

fn heading_has_no_card_box(engine: &Engine, theme: Theme) {
    let opts = tiles_engine::Options {
        width: 600.0,
        theme,
        cell_height: Some(16.0),
    };
    let heading = engine.heading("Business, last 12 months", Some("Example data"), &opts);
    assert!(!heading.boxed);
    assert_eq!(heading.height % 16.0, 0.0, "whole cells");
    let titles = heading.part(tiles_engine::scene::Part::Title).count();
    let notes = heading.part(tiles_engine::scene::Part::Subtitle).count();
    assert_eq!((titles, notes), (1, 1));
    let board = tiles_engine::Board {
        width: heading.width,
        height: heading.height,
        tiles: vec![tiles_engine::Placed {
            scene: heading,
            x: 0.0,
            y: 0.0,
        }],
    };
    let png = engine.board_png(&board, 2.0).unwrap();
    let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&png)).unwrap();
    let info = skia_safe::ImageInfo::new(
        (image.width(), image.height()),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    let row = usize::try_from(image.width()).unwrap() * 4;
    let mut px = vec![0u8; row * usize::try_from(image.height()).unwrap()];
    assert!(image.read_pixels(
        &info,
        &mut px,
        row,
        (0, 0),
        skia_safe::image::CachingHint::Allow
    ));
    let inked = px.chunks(4).filter(|p| p[3] > 0).count();
    assert!(inked > 0, "the letters draw");
    assert!(inked < px.len() / 4 / 10, "and nothing else: {inked}");
    // A card box would fill its right end, which no letter reaches.
    let at = (20 * row) + 1160 * 4;
    assert_eq!(px[at + 3], 0);
}

/// The reduced entrance (playbook N2): every card of a board fades in at
/// once over 150 ms, already settled in its place.
#[test]
fn reduce_motion_fades_a_board_in_place() {
    let engine = Engine::new().unwrap();
    for &theme in Theme::all() {
        reduced_board_fades_in_place(&engine, theme);
    }
}

fn reduced_board_fades_in_place(engine: &Engine, theme: Theme) {
    let card = scene(engine, "column", theme);
    let two = tiles_engine::Board {
        tiles: vec![
            tiles_engine::Placed {
                scene: card.clone(),
                x: 0.0,
                y: 0.0,
            },
            tiles_engine::Placed {
                scene: card.clone(),
                x: card.width + 16.0,
                y: 0.0,
            },
        ],
        width: card.width * 2.0 + 16.0,
        height: card.height,
    };
    assert_eq!(engine.board_frame_count(&two, Entrance::Reduced), 4);
    let frames = engine.board_entrance(&two, 1.0, Entrance::Reduced).unwrap();
    assert_eq!(frames.len(), 3);
}

/// Dashboard starts fall on whole 50 ms frames, at most 200 ms after the
/// first, so each one shows on a wrapper tick.
#[test]
fn board_starts_are_whole_ticks_within_200_ms() {
    let engine = Engine::new().unwrap();
    assert_eq!(engine.start_ms(0, 1), 0.0);
    let five: Vec<f32> = (0..5).map(|k| engine.start_ms(k, 5)).collect();
    assert_eq!(five, [0.0, 50.0, 100.0, 150.0, 200.0]);
    let nine: Vec<f32> = (0..9).map(|k| engine.start_ms(k, 9)).collect();
    assert_eq!(
        nine,
        [0.0, 0.0, 50.0, 50.0, 100.0, 100.0, 150.0, 150.0, 200.0]
    );
}
