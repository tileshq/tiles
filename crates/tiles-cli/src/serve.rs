//! `tiles mcp` (the MCP server on stdio), `tiles card` (one intent to one
//! saved card, for the Pi extension), `tiles theme` and `tiles open`. Both
//! wire the engine, the card cache, the Omni source and the registered
//! sources ([`tiles_mcp::source::Source`]) into `tiles-mcp`; the Omni client
//! is built only when an intent or a browse tool needs it, and only from the
//! user's own configuration (ADR-005, ADR-008), and a source only serves a
//! session while the user has configured it.

use std::cell::OnceCell;
use std::io::{self, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use serde_json::json;
use tiles_intent::source::{OmniSource, Table};
use tiles_mcp::cache::{self, Cache};
use tiles_mcp::card::{self, EnginePainter, Fetched, Look, Query};
use tiles_mcp::instructions::Draw;
use tiles_mcp::server::{Browse, Server};
use tiles_mcp::source::{Offer, Reach, Source};
use tiles_omni::{OmniClient, OmniConfig};

pub(crate) fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// The cache directory this process uses.
pub(crate) fn cache_dir() -> Result<PathBuf, String> {
    cache::default_dir(&env)
        .ok_or_else(|| "no HOME to keep cards under; set TILES_CACHE_DIR".to_string())
}

pub(crate) fn open_cache() -> Result<Cache, String> {
    Cache::open(&cache_dir()?)
}

/// The geometry from the environment, with the conversation's width and
/// the terminal's rows the session's wrapper last wrote
/// ([`crate::wrap::conversation_file`]), so a resize or a harness panel
/// applies to the next card. The rows count only where `tiles run` passed
/// `TILES_ROWS`, on the pre-cut route.
pub(crate) fn look() -> Look {
    let mut look = tiles_mcp::look_from_env(&env);
    let Some(size) = look
        .session
        .zip(cache_dir().ok())
        .and_then(|(session, dir)| {
            std::fs::read_to_string(crate::wrap::conversation_file(&dir, session)).ok()
        })
    else {
        return look;
    };
    let mut parts = size.split_whitespace().map(|p| p.parse::<u16>().ok());
    if let Some(cols) = parts.next().flatten().filter(|c| (10..=1000).contains(c)) {
        look.term_cols = cols;
    }
    if let Some(rows) = parts.next().flatten().filter(|r| (2..=1000).contains(r))
        && look.term_rows.is_some()
    {
        look.term_rows = Some(rows);
    }
    look
}

pub(crate) fn draw() -> Draw {
    Draw::from_name(env("TILES_DRAW").as_deref())
}

/// Whether `tiles run`'s companion window shows the cards (ADR-046).
fn companion() -> bool {
    tiles_mcp::companion_from_env(&env)
}

/// The opt-in that lets the values of a card drawn from Omni rows reach the
/// model: `TILES_OMNI_VALUES_TO_MODEL=1`, and nothing else.
pub(crate) fn omni_values() -> bool {
    env("TILES_OMNI_VALUES_TO_MODEL").as_deref() == Some("1")
}

/// Whether Omni is configured, without reaching it. Reads the environment
/// first; the Keychain only when the URL variable is absent.
pub(crate) fn omni_configured() -> bool {
    OmniConfig::from_system().is_ok()
}

/// Each registered source the user has configured, with what it offers.
pub(crate) fn offered<'a>(sources: &[&'a dyn Source]) -> Vec<(&'a dyn Source, Offer)> {
    sources
        .iter()
        .filter_map(|s| s.offer().map(|o| (*s, o)))
        .collect()
}

/// What a session can fetch from: Omni and each configured source.
pub(crate) fn reach(sources: &[&dyn Source]) -> Reach {
    Reach {
        omni: omni_configured(),
        offers: offered(sources).into_iter().map(|(_, o)| o).collect(),
    }
}

/// The origins whose values a chart result may hold: Omni's with
/// `TILES_OMNI_VALUES_TO_MODEL=1`, a source's with its own opt-in.
pub(crate) fn shared(offered: &[(&dyn Source, Offer)]) -> Vec<String> {
    let mut out = Vec::new();
    if omni_values() {
        out.push("Omni".to_string());
    }
    out.extend(
        offered
            .iter()
            .filter(|(s, _)| s.values_to_model())
            .map(|(_, o)| o.origin.to_string()),
    );
    out
}

/// A warehouse query's rows from the first configured source that runs
/// them.
pub(crate) fn fetch_warehouse(
    offered: &[(&dyn Source, Offer)],
    query: &tiles_intent::source::WarehouseSource,
    started: Instant,
) -> Result<Fetched, String> {
    offered
        .iter()
        .find_map(|(s, _)| s.fetch(query, started))
        .unwrap_or_else(|| {
            Err("no warehouse is configured for Tiles; tiles doctor says how to set one up".into())
        })
}

/// The Omni client, made on first use.
struct Omni(OnceCell<Result<OmniClient, String>>);

impl Omni {
    fn client(&self) -> Result<&OmniClient, String> {
        self.0
            .get_or_init(|| {
                OmniConfig::from_system()
                    .map(OmniClient::new)
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    fn fetch(&self, source: &OmniSource) -> Result<Table, String> {
        self.client()?.query(source).map_err(|e| e.to_string())
    }
}

impl Browse for Omni {
    fn topics(&self) -> Result<String, String> {
        let mut out = Vec::new();
        crate::omni::topics(self.client()?, &mut out).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn fields(&self, topic: &str) -> Result<String, String> {
        let mut out = Vec::new();
        crate::omni::fields(self.client()?, topic, &mut out).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}

/// `tiles mcp`: serves until standard input closes.
pub fn mcp(sources: &[&dyn Source]) -> ExitCode {
    let mut log = io::stderr();
    let painter = match EnginePainter::new() {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(log, "tiles: {}", tiles_term::sanitize::line(&e, 300));
            return ExitCode::FAILURE;
        }
    };
    let omni = Omni(OnceCell::new());
    let configured = omni_configured();
    // A source configured after the session started stays off until the
    // next one, since the harness's tool approvals were set without it.
    let offered = offered(sources);
    let fetch = |query: Query<'_>, started: Instant| match query {
        Query::Omni(source) if configured => omni.fetch(source).map(Fetched::omni),
        Query::Omni(_) => Err("Omni is not configured".to_string()),
        Query::Warehouse(source) => fetch_warehouse(&offered, source, started),
    };
    let server = Server {
        painter: &painter,
        fetch: &fetch,
        browse: configured.then_some(&omni as &dyn Browse),
        sources: offered.clone(),
        cache: open_cache(),
        look: &look,
        draw: draw(),
        companion: companion(),
        shared: shared(&offered),
        env: &env,
    };
    let stdin = io::stdin();
    match server.serve(&mut stdin.lock(), &mut io::stdout().lock(), &mut log) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(log, "tiles: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `tiles card <intent.json | ->`: draws and saves one card and prints it as
/// one JSON object, `{"ok": true, ...}` or `{"ok": false, "error"}`.
pub fn card(input: &str, sources: &[&dyn Source]) -> ExitCode {
    let offered = offered(sources);
    let outcome = (|| {
        let json = crate::read_input(input)?;
        let painter = EnginePainter::new()?;
        let omni = Omni(OnceCell::new());
        let fetch = |query: Query<'_>, started: Instant| match query {
            Query::Omni(source) => omni.fetch(source).map(Fetched::omni),
            Query::Warehouse(source) => fetch_warehouse(&offered, source, started),
        };
        let cache = open_cache()?;
        let draw = draw();
        card::make(&json, &painter, &fetch, &cache, &look())
            .map(|c| (c, draw))
            .map_err(|r| r.to_string())
    })();
    let (value, code) = match outcome {
        Ok((c, draw)) => (
            json!({
                "ok": true,
                "card": c.id,
                "png": c.path.display().to_string(),
                "title": c.title,
                "form": c.forms(),
                "cols": c.cells.0,
                "rows": c.cells.1,
                "width": c.size_px.0,
                "height": c.size_px.1,
                "text": card::text_result(&c, draw, companion(), &shared(&offered)),
            }),
            ExitCode::SUCCESS,
        ),
        Err(e) => (
            json!({ "ok": false, "error": tiles_term::sanitize::line(&e, 600) }),
            ExitCode::FAILURE,
        ),
    };
    println!("{value}");
    code
}

/// `tiles theme [name]`: lists the themes, or saves one; inside a `tiles
/// run` kitty session the session's earlier cards change with it (ADR-021).
pub fn theme(name: Option<&str>) -> Result<String, String> {
    let Some(name) = name else {
        return Ok(tiles_mcp::theme::list(&env));
    };
    let chosen = tiles_mcp::theme::parse(name)?;
    let session = look().swapping_session();
    let painter = EnginePainter::new()?;
    let cache = session.and_then(|_| open_cache().ok());
    tiles_mcp::theme::set(chosen, &env, &painter, cache.as_ref(), session)
}

/// Usage of `tiles cache`.
pub const CACHE_USAGE: &str = "tiles cache clear";

/// `tiles cache clear`: removes every saved card, its call and its entrance
/// frames, and says how many cards went. Rows a running session fetched from
/// Omni or a warehouse were never on disk and go when that session ends
/// (ADR-027).
pub fn cache_clear() -> Result<String, String> {
    open_cache()?.clear().map(cleared)
}

/// What `tiles cache clear` prints for `n` cards removed.
#[must_use]
pub fn cleared(n: usize) -> String {
    format!("Removed {n} {}.", if n == 1 { "card" } else { "cards" })
}

/// `tiles open <id>`. Inside `tiles run` it draws an earlier card again as
/// a new card in the theme in force and prints what the chart tool would
/// (the placeholder block where the wrapper draws), for terminals that
/// cannot change a card in place (ADR-021). In a plain shell, `!tiles open`
/// in a harness included, it prints the card's id, theme and PNG path, the
/// card drawn again in the theme in force only when it was drawn in
/// another, and on macOS shows it in a window of its own (ADR-023). A card
/// drawn from fetched rows is shown as it was drawn, with a line saying only
/// the session that fetched them can draw it in another theme, and inside
/// `tiles run` with no window (ADR-027).
pub fn open(id: &str, sources: &[&dyn Source]) -> Result<String, String> {
    let painter = EnginePainter::new()?;
    let look = look();
    let cache = open_cache()?;
    let id = id.trim();
    // A card drawn from fetched rows is drawn again only by the session
    // that fetched them, never by this process (ADR-027): it is shown as
    // drawn.
    if env("TILES_DRAW").is_none() || card::needs_its_session(&cache, id) {
        let shown = card::shown(id, &painter, &cache, look.theme)?;
        let mut said = format!(
            "Card {} in {}: {}",
            shown.id,
            shown.theme.name(),
            tiles_term::sanitize::line(&shown.path.display().to_string(), 600)
        );
        if let Some(note) = &shown.note {
            said.push('\n');
            said.push_str(&tiles_term::sanitize::line(note, 400));
        }
        if !crate::window::wanted(&env) {
            return Ok(said);
        }
        let light = shown.theme.is_light();
        return match crate::window::show(&shown.path, &shown.title, light) {
            Ok(()) => Ok(format!(
                "{said}\nOpened in a window of its own; Cmd-W closes it."
            )),
            Err(e) => Err(format!("{said}; {e}")),
        };
    }
    let card = card::reopen(id, &painter, &cache, &look)?;
    Ok(card::text_result(
        &card,
        draw(),
        companion(),
        &shared(&offered(sources)),
    ))
}
