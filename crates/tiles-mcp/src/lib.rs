//! The Tiles MCP server (ADR-003, ADR-004): the chart tool AI coding harnesses
//! call, over stdio.
//!
//! [`server`] speaks the protocol, [`card`] turns an intent into a saved card
//! through the engine's narrow [`card::Painter`] interface, [`board`] sizes
//! and places several cards on one picture, [`cache`] keeps cards owner-only,
//! [`instructions`] holds the session instruction every harness gets,
//! [`settings`] keeps the theme the user picked and [`theme`] lists and sets
//! it. [`source`] is the seam other data sources join through.
//! [`look_from_env`] reads the terminal geometry `tiles run` hands the
//! server, and [`companion_from_env`] whether its companion window shows
//! the cards.

pub mod board;
pub mod cache;
pub mod card;
pub mod instructions;
pub mod server;
pub mod settings;
pub mod source;
pub mod theme;

use card::Look;

/// The MCP tool name harnesses call to draw a chart.
pub const TOOL_NAME: &str = "tiles_chart";
/// The tool that lists Omni topics.
pub const OMNI_TOPICS_TOOL: &str = "tiles_omni_topics";
/// The tool that lists an Omni topic's fields.
pub const OMNI_FIELDS_TOOL: &str = "tiles_omni_fields";
/// The tool that lists and sets the card theme.
pub const THEME_TOOL: &str = "tiles_theme";

/// The intent schema version the tool accepts.
#[must_use]
pub fn accepted_schema() -> u32 {
    tiles_intent::SCHEMA_VERSION
}

/// The theme from [`settings::theme`] (`TILES_THEME`, else the settings
/// file, else the design tokens' default theme), and the geometry from `TILES_COLS`, `TILES_ROWS`,
/// `TILES_CELL_PX` (`WxH` device pixels), `TILES_SCALE`, `TILES_FRAMES`
/// (draw the entrance: one card's frames, 2 to 60), `TILES_MOTION`
/// (`reduce` draws the reduced entrance) and `TILES_SESSION` (the `tiles
/// run` session's hex token), each ignored when absent or out of range.
#[must_use]
pub fn look_from_env(env: &dyn Fn(&str) -> Option<String>) -> Look {
    let mut look = Look {
        theme: settings::theme(env).0,
        ..Look::default()
    };
    if let Some(cols) = env("TILES_COLS")
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|c| (10..=1000).contains(c))
    {
        look.term_cols = cols;
    }
    look.term_rows = env("TILES_ROWS")
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|r| (2..=1000).contains(r));
    if let Some((w, h)) = env("TILES_CELL_PX").and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
    }) && (2.0..=200.0).contains(&w)
        && (2.0..=400.0).contains(&h)
    {
        look.cell_px = (w, h);
    }
    if let Some(n) = env("TILES_FRAMES")
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|n| (2..=60).contains(n))
    {
        look.frames = n;
    }
    if env("TILES_MOTION").as_deref() == Some("reduce") {
        look.entrance = tiles_engine::Entrance::Reduced;
    }
    if let Some(s) = env("TILES_SESSION")
        .filter(|v| (1..=16).contains(&v.len()))
        .and_then(|v| u64::from_str_radix(&v, 16).ok())
    {
        look.session = Some(s);
    }
    if let Some(s) = env("TILES_SCALE")
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|s| (0.5..=4.0).contains(s))
    {
        look.scale = s;
    }
    look
}

/// The variable `tiles run` sets to `1`, beside `TILES_SESSION`, when its
/// companion window shows the session's cards beside the terminal on the
/// text route (ADR-046).
pub const COMPANION_ENV: &str = "TILES_COMPANION";

/// Whether [`COMPANION_ENV`] says the companion window shows the cards. The
/// server never guesses it from the terminal: only `tiles run` knows it
/// keeps the window.
#[must_use]
pub fn companion_from_env(env: &dyn Fn(&str) -> Option<String>) -> bool {
    env(COMPANION_ENV).as_deref() == Some("1")
}

#[cfg(test)]
mod tests {
    use super::{COMPANION_ENV, TOOL_NAME, accepted_schema, companion_from_env, look_from_env};
    use crate::card::Theme;

    #[test]
    fn tool_accepts_the_current_schema() {
        assert_eq!(TOOL_NAME, "tiles_chart");
        assert_eq!(accepted_schema(), 1);
    }

    #[test]
    fn look_reads_valid_settings_and_ignores_the_rest() {
        let env = |k: &str| match k {
            "TILES_THEME" => Some("light".to_string()),
            "TILES_COLS" => Some("120".to_string()),
            "TILES_ROWS" => Some("48".to_string()),
            "TILES_CELL_PX" => Some("18x36".to_string()),
            "TILES_SCALE" => Some("9".to_string()),
            "TILES_FRAMES" => Some("15".to_string()),
            "TILES_SESSION" => Some("0000000000000abc".to_string()),
            _ => None,
        };
        let look = look_from_env(&env);
        assert_eq!(look.theme, Theme::from_name("light").unwrap());
        assert_eq!(look.term_cols, 120);
        assert_eq!(look.term_rows, Some(48));
        assert_eq!(look_from_env(&|_| None).term_rows, None);
        assert_eq!(look.cell_px, (18.0, 36.0));
        assert!((look.scale - 2.0).abs() < f32::EPSILON);
        assert_eq!(look.frames, 15);
        assert_eq!(look.session, Some(0xabc));
        assert_eq!(super::Look::default().session, None);
        assert_eq!(look_from_env(&|_| None).theme, Theme::default(), "ADR-021");
        for &t in Theme::all() {
            let named = |k: &str| (k == "TILES_THEME").then(|| t.name().to_string());
            assert_eq!(look_from_env(&named).theme, t);
        }
        let one = |k: &str| (k == "TILES_FRAMES").then(|| "1".to_string());
        assert_eq!(look_from_env(&one).frames, 0, "one frame is no entrance");
        let bad = |k: &str| (k == "TILES_CELL_PX").then(|| "NaNxinf".to_string());
        assert_eq!(look_from_env(&bad).cell_px, (16.0, 32.0));
    }

    #[test]
    fn only_a_session_that_plays_entrances_swaps_cards_in_place() {
        let warp = |k: &str| (k == "TILES_SESSION").then(|| "abc".to_string());
        assert_eq!(look_from_env(&warp).session, Some(0xabc));
        assert_eq!(look_from_env(&warp).swapping_session(), None);
        let kitty = |k: &str| match k {
            "TILES_SESSION" => Some("abc".to_string()),
            "TILES_FRAMES" => Some("15".to_string()),
            _ => None,
        };
        assert_eq!(look_from_env(&kitty).swapping_session(), Some(0xabc));
        let precut = |k: &str| match k {
            "TILES_SESSION" => Some("abc".to_string()),
            "TILES_FRAMES" => Some("10".to_string()),
            "TILES_ROWS" => Some("40".to_string()),
            _ => None,
        };
        assert_eq!(
            look_from_env(&precut).swapping_session(),
            None,
            "the pre-cut route plays the entrance but keeps old pictures"
        );
    }

    #[test]
    fn reduce_motion_draws_the_reduced_entrance() {
        let reduce = |k: &str| (k == "TILES_MOTION").then(|| "reduce".to_string());
        assert_eq!(
            look_from_env(&reduce).entrance,
            tiles_engine::Entrance::Reduced
        );
        assert_eq!(
            look_from_env(&|_| None).entrance,
            tiles_engine::Entrance::Full
        );
    }

    #[test]
    fn only_tiles_run_turns_the_companion_wording_on() {
        assert!(companion_from_env(
            &|k| (k == COMPANION_ENV).then(|| "1".into())
        ));
        assert!(!companion_from_env(&|_| None));
        assert!(!companion_from_env(
            &|k| (k == COMPANION_ENV).then(|| "0".into())
        ));
        let text_route = |k: &str| (k == "TILES_DRAW").then(|| "text".to_string());
        assert!(
            !companion_from_env(&text_route),
            "never guessed from the route"
        );
    }
}
