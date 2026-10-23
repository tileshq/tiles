//! Lists and sets the card theme (ADR-021), for `tiles theme` and the
//! `tiles_theme` tool behind each harness's `/theme`. Setting it writes the
//! settings file ([`crate::settings`]); the next card is drawn in it. Inside
//! a `tiles run` session that plays cards in kitty, the session's earlier
//! cards are drawn again in it under their own ids, so the wrapper swaps
//! every copy in place ([`crate::card::retheme`]); a card drawn from Omni
//! rows changes only in the process that fetched them, the MCP server that
//! drew it (ADR-027). Elsewhere a terminal keeps
//! old copies (Warp keeps every picture it was sent), so only new cards
//! change, and `tiles open <id>` shows an earlier card in the new theme.

use std::fmt::Write as _;

use tiles_engine::Theme;

use crate::cache::Cache;
use crate::card::{self, Painter};
use crate::settings::{self, Source};

/// What a theme looks like, for the list, from the design tokens; the
/// default theme says so.
#[must_use]
pub fn describe(theme: Theme) -> String {
    if theme == Theme::default() {
        format!("{} (the default)", theme.about())
    } else {
        theme.about().to_string()
    }
}

/// The themes, one a line, the one in force marked with `*` and where it
/// came from.
#[must_use]
pub fn list(env: &dyn Fn(&str) -> Option<String>) -> String {
    let (current, source) = settings::theme(env);
    let mut out = String::new();
    for &t in Theme::all() {
        let mark = if t == current { '*' } else { ' ' };
        let _ = writeln!(out, "{mark} {:<9} {}", t.name(), describe(t));
    }
    let why = match source {
        Source::Env => "TILES_THEME sets it for this session",
        Source::Saved => "saved with tiles theme",
        Source::Default => "nothing chosen yet, so the default",
    };
    let _ = write!(
        out,
        "Cards are drawn in {} ({why}). Set one with tiles theme <name>, or your harness's Tiles theme command.",
        current.name()
    );
    out
}

/// The theme `name` names, or a one-line refusal listing the names.
///
/// # Errors
///
/// When `name` is not a theme.
pub fn parse(name: &str) -> Result<Theme, String> {
    Theme::from_name(name.trim()).ok_or_else(|| {
        let names: Vec<&str> = Theme::all().iter().map(|t| t.name()).collect();
        format!(
            "\"{}\" is not a theme; use {}",
            tiles_term::sanitize::line(name, 40),
            names.join(", ")
        )
    })
}

/// Saves `theme` and, inside a `tiles run` kitty session (`session`), draws
/// that session's cards again in it. Returns the result text.
///
/// # Errors
///
/// When the settings file cannot be written.
pub fn set(
    theme: Theme,
    env: &dyn Fn(&str) -> Option<String>,
    painter: &dyn Painter,
    cache: Option<&Cache>,
    session: Option<u64>,
) -> Result<String, String> {
    let path = settings::save_theme(env, theme)?;
    let mut out = format!(
        "Theme set to {}, saved in {}.",
        theme.name(),
        tiles_term::sanitize::line(&path.display().to_string(), 200)
    );
    let (now, source) = settings::theme(env);
    if source == Source::Env && now != theme {
        let _ = write!(
            out,
            " TILES_THEME={} still sets this session's theme; cards follow the saved theme in sessions without it.",
            now.name()
        );
        return Ok(out);
    }
    out.push_str(" The next card is drawn in it.");
    match (session, cache) {
        (Some(s), Some(cache)) => {
            let card::Rethemed { done, failed, kept } = card::retheme(painter, cache, s, theme);
            match done {
                0 => {}
                1 => out.push_str(" The 1 card drawn earlier in this session switches in place."),
                n => {
                    let _ = write!(
                        out,
                        " The {n} cards drawn earlier in this session switch in place."
                    );
                }
            }
            match kept {
                0 => {}
                1 => out.push_str(
                    " The 1 card drawn from Omni rows keeps its theme, since Tiles keeps those rows \
                     only in the memory of the session that fetched them.",
                ),
                n => {
                    let _ = write!(
                        out,
                        " The {n} cards drawn from Omni rows keep their theme, since Tiles keeps \
                         those rows only in the memory of the session that fetched them."
                    );
                }
            }
            if failed > 0 {
                let _ = write!(
                    out,
                    " {failed} could not be drawn again and keep their theme; tiles open <id> shows one in {}.",
                    theme.name()
                );
            }
        }
        _ => {
            let _ = write!(
                out,
                " Cards already on screen keep their theme in this terminal; tiles open <id> shows an earlier card in {}.",
                theme.name()
            );
        }
    }
    Ok(out)
}
