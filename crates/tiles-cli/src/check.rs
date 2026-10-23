//! `tiles check`: one line per check, `pass` or `fail` first, then the fact,
//! for a terminal that cannot be driven from the development Mac (Warp,
//! ADR-025). It reads the terminal's answers, the variables `tiles run` would
//! set, the harnesses on `PATH`, the card cache and the `TILES_TRACE` file.
//! Of a saved card it prints only its id, form, theme, claim tone and rows,
//! never a value or title from its rows (ADR-013).

use std::path::{Path, PathBuf};

use tiles_engine::Theme;
use tiles_intent::{TILES_VERSION, Tone};
use tiles_mcp::instructions::Draw;
use tiles_mcp::settings::Source;
use tiles_term::detect::{self, CellSource, Detected, Replies, Route, Terminal};

use crate::harness::{self, Harness};
use crate::wrap::TRACE_SESSION;

/// What the checks read.
pub struct Inputs<'a> {
    /// One environment variable.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// The terminal's answers, when it was asked.
    pub replies: Option<&'a Replies>,
    /// Why it was not asked: neither standard input nor output is a
    /// terminal, or one is not and `/dev/tty` is no terminal either.
    pub not_asked: Option<&'a str>,
    /// The terminal's size and window cell.
    pub screen: crate::run::Screen,
    /// Where a program is on `PATH`.
    pub which: &'a dyn Fn(&str) -> Option<PathBuf>,
    /// The cache directory, or why there is none.
    pub cache: Result<PathBuf, String>,
    /// `--tag`: the release tag that was installed.
    pub tag: Option<&'a str>,
    /// What `opencode --help` and `--version` said, when it is on `PATH`.
    pub opencode: Option<&'a crate::opencode::Found>,
    /// `OpenCode`'s `config.json` when `tiles install opencode` put a tiles
    /// server in it.
    pub opencode_install: Option<PathBuf>,
    /// macOS's Reduce Motion, `None` when it cannot be read.
    pub reduce_motion: Option<bool>,
    /// Each harness on `PATH` that was asked its `--version`, with the
    /// version it named (ADR-032).
    pub versions: &'a [(Harness, Option<String>)],
}

/// Saved cards described, newest first.
const NEWEST: usize = 5;
/// Largest geometry, call or trace file read.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

fn line(ok: bool, fact: &str) -> String {
    format!(
        "{} {}",
        if ok { "pass" } else { "fail" },
        tiles_term::sanitize::line(fact, 600)
    )
}

/// The checks, one line each.
#[must_use]
pub fn report(i: &Inputs<'_>) -> Vec<String> {
    let mut out = vec![version(i.tag)];
    let detected = match detect::detect(i.env, i.replies) {
        Ok(d) => {
            out.push(line(d.route != Route::Text, &format!("terminal: {d}")));
            Some(d)
        }
        Err(e) => {
            out.push(line(false, &format!("terminal: {e}")));
            None
        }
    };
    if let Some(d) = &detected {
        out.push(graphics(i.replies, i.not_asked, d.route));
        out.push(cell(i, d.terminal));
        out.push(rows(i, d));
        out.push(motion(i, d));
        out.extend(harnesses(i.which, d, i.versions));
    }
    out.extend(opencode(i));
    let (theme, source) = tiles_mcp::settings::theme(i.env);
    out.push(line(
        true,
        &format!(
            "theme: {} for every harness, from {}",
            theme.name(),
            match source {
                Source::Env => "TILES_THEME",
                Source::Saved => "the saved setting (tiles theme)",
                Source::Default => "the default",
            }
        ),
    ));
    match &i.cache {
        Ok(dir) => {
            let cards = dir.join("cards");
            out.push(cache_line(&cards));
            out.extend(newest(&cards, theme));
            out.extend(trace(i.env, &cards));
        }
        Err(e) => out.push(line(false, &format!("cards: {e}"))),
    }
    out
}

/// How cards enter in `tiles run` here: the entrance's frames on the
/// route, the reduced fade, or none (ADR-030).
fn motion(i: &Inputs<'_>, d: &Detected) -> String {
    use crate::motion::Motion;
    let (m, _) = crate::motion::setting(i.env, || i.reduce_motion);
    let route = d.route.name();
    let fact = match (harness::animates(d), m, m.frames()) {
        (false, _, _) => format!("motion: no entrance on the {route} route"),
        (true, Motion::Off, _) | (true, _, None) => "motion: off".to_string(),
        (true, Motion::Reduce, Some((n, ms))) => format!(
            "motion: reduce, {n} frames over {} ms",
            u32::from(n.saturating_sub(1)) * u32::from(ms)
        ),
        (true, Motion::On, Some((n, ms))) => {
            format!("motion: entrance on the {route} route, {n} frames at {ms} ms")
        }
    };
    line(true, &fact)
}

fn version(tag: Option<&str>) -> String {
    match tag {
        Some(tag) => {
            let ok = tag.strip_prefix('v').unwrap_or(tag) == TILES_VERSION;
            let is = if ok { "is" } else { "is not" };
            line(
                ok,
                &format!("version: tiles {TILES_VERSION} {is} release {tag}"),
            )
        }
        None if TILES_VERSION == "0.0.0" => line(
            false,
            "version: tiles 0.0.0, a build that carries no release tag",
        ),
        None => line(
            true,
            &format!("version: tiles {TILES_VERSION}, from release v{TILES_VERSION}"),
        ),
    }
}

fn graphics(replies: Option<&Replies>, not_asked: Option<&str>, route: Route) -> String {
    let Some(r) = replies else {
        return line(
            false,
            &format!(
                "graphics: not asked; {}",
                not_asked.unwrap_or("stdin or stdout is not a terminal")
            ),
        );
    };
    let yes = |b: bool| if b { "yes" } else { "no" };
    let ok = match route {
        Route::KittyPlaceholders | Route::KittyPrecut => r.kitty_graphics,
        Route::Sixel => r.sixel,
        Route::Text => false,
    };
    line(
        ok,
        &format!(
            "graphics: kitty {}, sixel {}, for the {} route",
            yes(r.kitty_graphics),
            yes(r.sixel),
            route.name()
        ),
    )
}

/// The cell size `tiles run` would pass, and the two candidates it chose
/// from.
fn cell(i: &Inputs<'_>, terminal: Terminal) -> String {
    let scale = tiles_mcp::look_from_env(i.env).scale;
    let size = |(w, h): (u16, u16)| format!("{w}x{h}");
    let reply = i
        .replies
        .and_then(|r| r.cell_px)
        .map_or_else(|| "none".into(), size);
    let window = match (
        i.screen.window,
        detect::cell_px(terminal, None, i.screen.window, scale),
    ) {
        (None, _) => "none".into(),
        (Some(w), None) => format!("{} per cell, too small to use", size(w)),
        (Some(w), Some((c, CellSource::WindowPoints))) => {
            format!("{} per cell, read as points {}", size(w), size(c))
        }
        (Some(w), Some((c, _))) => format!("{} per cell, as device px {}", size(w), size(c)),
    };
    let candidates = format!("CSI 16 t reply {reply}; window size {window}");
    match crate::run::cell(i.replies, i.env, terminal, i.screen.window) {
        Some((c, from)) => line(
            true,
            &format!("cell size: TILES_CELL_PX={c}, from {from}; {candidates}"),
        ),
        None => line(
            false,
            &format!("cell size: none found, so cards use 16x32 device px; {candidates}"),
        ),
    }
}

/// Whether a route that shows only a card's visible rows gets the rows, and
/// the tallest card they allow.
fn rows(i: &Inputs<'_>, detected: &Detected) -> String {
    let cache = i.cache.as_deref().unwrap_or(Path::new(""));
    let vars = crate::run::look_env(i.replies, i.env, cache, Harness::Claude, detected, i.screen);
    let get = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let route = detected.route.name();
    match detected.route {
        Route::KittyPrecut => {}
        Route::Sixel => {
            return line(
                true,
                "rows: the sixel route draws a tall card cut at the screen's edge, so it gets no TILES_ROWS",
            );
        }
        Route::KittyPlaceholders | Route::Text => {
            return line(
                true,
                &format!("rows: the {route} route scrolls a whole card, so it needs no TILES_ROWS"),
            );
        }
    }
    match (
        get("TILES_ROWS"),
        tiles_mcp::look_from_env(&get).card_rows(),
    ) {
        (Some(r), Some(most)) => line(
            true,
            &format!(
                "rows: TILES_ROWS={r} on the {route} route, so a card stands at most {most} rows"
            ),
        ),
        _ => line(
            false,
            &format!("rows: no TILES_ROWS on the {route} route, so a tall card loses its headline"),
        ),
    }
}

/// Each harness on `PATH`, who draws its card, and for Pi, Claude Code and
/// Codex whether Tiles was proved on its version (ADR-032); `OpenCode`'s is
/// the `opencode server:` line.
fn harnesses(
    which: &dyn Fn(&str) -> Option<PathBuf>,
    detected: &Detected,
    versions: &[(Harness, Option<String>)],
) -> Vec<String> {
    let found: Vec<String> = Harness::ALL
        .into_iter()
        .filter_map(|h| {
            let path = which(h.name())?;
            let (draw, wrap) = harness::draw_for(h, detected, false);
            let who = match (draw, wrap, h) {
                (_, true, _) => "drawn by the wrapper",
                (Draw::Text, ..) => "text only, no card",
                (_, _, Harness::Pi) => "drawn by Pi itself",
                (_, _, Harness::Claude) => "drawn by Claude Code's Image mod",
                _ => "drawn by the harness",
            };
            let version = versions
                .iter()
                .find(|(v, _)| *v == h)
                .and_then(|(_, v)| crate::proved::check_line(h, v.as_deref()))
                .map(|(ok, said)| line(ok, &said));
            let mut lines = vec![line(
                draw != Draw::Text,
                &format!("{}: {}, {who}", h.name(), path.display()),
            )];
            lines.extend(version);
            Some(lines)
        })
        .flatten()
        .collect();
    if found.is_empty() {
        return vec![line(
            false,
            "harnesses: none of pi, opencode, claude or codex on PATH",
        )];
    }
    found
}

/// Which server a `tiles run opencode` session's model calls, on the
/// `OpenCode` on `PATH`, and whether an installed tiles server could take
/// its place (ADR-029).
fn opencode(i: &Inputs<'_>) -> Vec<String> {
    use crate::opencode::Cli;
    let Some(found) = i.opencode else {
        return Vec::new();
    };
    let (ok, said) = found.describe();
    let mut out = vec![line(ok, &format!("opencode server: {said}"))];
    if let Some(file) = &i.opencode_install {
        let file = file.display();
        out.push(match found.cli {
            Some(Cli::Public) => line(
                true,
                &format!(
                    "opencode install: {file} holds a tiles server too; OpenCode merges OPENCODE_CONFIG_CONTENT after it, so the session's replaces it"
                ),
            ),
            _ => line(
                false,
                &format!(
                    "opencode install: {file} holds a tiles server too, and Tiles has not proved which of the two this OpenCode keeps; move the file aside, or run a tiles run opencode session with TILES_TRACE set and check its trace line"
                ),
            ),
        });
    }
    out
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&read(path)?).ok()
}

fn read(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

/// The saved cards' ids, newest first.
fn saved(cards: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(cards) else {
        return Vec::new();
    };
    let mut ids: Vec<(std::time::SystemTime, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let id = name.strip_suffix(".json")?;
            if !tiles_term::block::is_card_id(id) {
                return None;
            }
            Some((e.metadata().ok()?.modified().ok()?, id.to_string()))
        })
        .collect();
    ids.sort();
    ids.into_iter().rev().map(|(_, id)| id).collect()
}

fn cache_line(cards: &Path) -> String {
    let count = saved(cards).len();
    line(true, &format!("cards: {}, {count} saved", cards.display()))
}

/// The claim tones anywhere in a call: one for a chart, one per card for a
/// row or a dashboard. Only a tone the schema lists is kept.
fn tones(v: &serde_json::Value, out: &mut Vec<Tone>) {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(t) = m
                .get("claim")
                .and_then(|c| c.get("tone"))
                .and_then(serde_json::Value::as_str)
                .and_then(|s| Tone::ALL.into_iter().find(|t| t.name() == s))
            {
                out.push(t);
            }
            m.values().for_each(|v| tones(v, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|v| tones(v, out)),
        _ => {}
    }
}

/// The newest saved cards: theme, claim tones and rows, each passing when
/// it is in the theme in use and no taller than its terminal allowed.
fn newest(cards: &Path, theme: Theme) -> Vec<String> {
    saved(cards)
        .into_iter()
        .take(NEWEST)
        .map(|id| {
            let meta = read_json(&cards.join(format!("{id}.json"))).unwrap_or_default();
            let call = read_json(&cards.join(format!("{id}.call.json"))).unwrap_or_default();
            let drawn = meta
                .get("theme")
                .and_then(serde_json::Value::as_str)
                .and_then(Theme::from_name);
            let form = call
                .pointer("/call/form")
                .and_then(serde_json::Value::as_str)
                .filter(|f| f.len() <= 24 && f.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'))
                .unwrap_or("several cards");
            let mut found = Vec::new();
            tones(
                call.get("call").unwrap_or(&serde_json::Value::Null),
                &mut found,
            );
            let tone = if found.is_empty() {
                "no claim tone".to_string()
            } else {
                let names: Vec<&str> = found.iter().map(|t| t.name()).collect();
                let red = if found.contains(&Tone::Bad) {
                    "a bad tone draws its highlight red (playbook K2)"
                } else {
                    "highlight in the accent"
                };
                format!("tone {}, {red}", names.join(", "))
            };
            let num = |v: Option<&serde_json::Value>| {
                v.and_then(serde_json::Value::as_u64)
                    .and_then(|n| u16::try_from(n).ok())
            };
            let card_rows = num(meta.get("rows"));
            let term_rows = num(call.pointer("/look/term_rows"));
            let most = term_rows.and_then(|r| {
                tiles_mcp::card::Look {
                    term_rows: Some(r),
                    ..tiles_mcp::card::Look::default()
                }
                .card_rows()
            });
            let (fits, rows) = match (card_rows, term_rows, most) {
                (Some(c), Some(t), Some(m)) => (c <= m, format!("{c} rows of {t}, at most {m}")),
                (Some(c), ..) => (true, format!("{c} rows")),
                _ => (true, "rows unknown".to_string()),
            };
            let same = drawn == Some(theme);
            line(
                same && fits,
                &format!(
                    "card {id}: {form}, {} theme, {tone}, {rows}",
                    drawn.map_or("unknown", Theme::name)
                ),
            )
        })
        .collect()
}

/// When `TILES_TRACE` names a file: whether the last session in it saw every
/// card it made whole on screen, which the wrapper needs before it draws one,
/// and whether any card's last placement, loss or scroll was its copy lost
/// on screen and its pictures deleted.
fn trace(env: &dyn Fn(&str) -> Option<String>, cards: &Path) -> Option<String> {
    let path = PathBuf::from(env("TILES_TRACE")?);
    if !path.is_file() {
        return None;
    }
    let shown = path.display();
    let text = String::from_utf8_lossy(&read(&path).unwrap_or_default()).into_owned();
    let Some(start) = text
        .match_indices(TRACE_SESSION)
        .map(|(at, _)| at)
        .filter(|at| *at == 0 || text.as_bytes()[at - 1] == b'\n')
        .last()
    else {
        return Some(line(
            false,
            &format!("trace: {shown} has no session; run tiles run with TILES_TRACE set first"),
        ));
    };
    let session = &text[start + TRACE_SESSION.len()..];
    let session: String = session
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    let after = &text[start..];
    let made: Vec<String> = saved(cards)
        .into_iter()
        .rev()
        .filter(|id| {
            read_json(&cards.join(format!("{id}.json")))
                .and_then(|m| m.get("session")?.as_str().map(|s| s == session))
                .unwrap_or(false)
        })
        .collect();
    if made.is_empty() {
        // Cards the session saw whole that another session's server made:
        // the harness called a tiles server that was not this session's.
        let others: Vec<String> = saved(cards)
            .into_iter()
            .rev()
            .filter(|id| after.lines().any(|l| l.contains(&format!("{id} whole:"))))
            .collect();
        let why = if others.is_empty() {
            String::new()
        } else {
            format!(
                "; it saw {} whole, made by another session's tiles server, so the harness did not call this session's",
                others.join(", ")
            )
        };
        return Some(line(
            false,
            &format!("trace: the last session in {shown} made no card{why}"),
        ));
    }
    let blank: Vec<&str> = made
        .iter()
        .filter(|id| !after.lines().any(|l| l.contains(&format!("{id} whole:"))))
        .map(String::as_str)
        .collect();
    let lost = lost_on_screen(&made, after);
    let cards = |n: usize| {
        if n == 1 {
            "1 card".to_string()
        } else {
            format!("{n} cards")
        }
    };
    let saw = if made.len() == 1 { "its" } else { "all" };
    Some(if blank.is_empty() && lost.is_empty() {
        line(
            true,
            &format!(
                "trace: the last session in {shown} saw {saw} {} whole: {}",
                cards(made.len()),
                made.join(", ")
            ),
        )
    } else if blank.is_empty() {
        let their = if lost.len() == 1 { "its" } else { "their" };
        line(
            false,
            &format!(
                "trace: the last session in {shown} saw {saw} {} whole, then lost {} on screen and deleted {their} pictures",
                cards(made.len()),
                lost.join(", ")
            ),
        )
    } else {
        let they = if blank.len() == 1 { "it" } else { "they" };
        line(
            false,
            &format!(
                "trace: the last session in {shown} made {} and never saw {} whole, so {they} stayed blank",
                cards(made.len()),
                blank.join(", ")
            ),
        )
    })
}

/// The cards of `made` whose last placement, loss or scroll in the trace
/// text `after` deleted their pictures with their block still on screen:
/// its tokens went, as when `OpenCode` drew them again as links.
fn lost_on_screen<'a>(made: &'a [String], after: &str) -> Vec<&'a str> {
    made.iter()
        .filter(|id| {
            let lost = format!(" {id} copy lost");
            after
                .lines()
                .rev()
                .find(|l| {
                    l.contains(&lost)
                        || l.contains(&format!(" {id} scrolled away"))
                        || (l.contains(&format!(" {id} ")) && l.contains(" placed "))
                })
                .is_some_and(|l| l.contains(&lost))
        })
        .map(String::as_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use tiles_term::detect::Replies;

    use super::{Inputs, report};
    use crate::harness::Harness;
    use tiles_engine::Theme;

    /// A row value an Omni topic could return; no check line may show it.
    const ROW_VALUE: &str = "acme_planted_row_4417";

    fn run(vars: &[(&str, &str)], cache: &Path, window: Option<(u16, u16)>) -> Vec<String> {
        run_with(vars, cache, window, &[])
    }

    fn run_with(
        vars: &[(&str, &str)],
        cache: &Path,
        window: Option<(u16, u16)>,
        versions: &[(Harness, Option<String>)],
    ) -> Vec<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        let env = move |k: &str| map.get(k).cloned();
        let which = |p: &str| (p != "codex").then(|| PathBuf::from(format!("/opt/bin/{p}")));
        let replies = Replies::parse(b"\x1b_Gi=31;OK\x1b\\\x1bP>|Warp\x1b\\\x1b[?62c");
        report(&Inputs {
            env: &env,
            replies: Some(&replies),
            not_asked: None,
            screen: crate::run::Screen {
                size: (50, 154),
                window,
            },
            which: &which,
            cache: Ok(cache.to_path_buf()),
            tag: Some("v0.0.0"),
            opencode: None,
            opencode_install: None,
            reduce_motion: Some(false),
            versions,
        })
    }

    fn has(lines: &[String], want: &str) -> bool {
        lines.iter().any(|l| l == want)
    }

    fn cache(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tiles-check-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cards")).unwrap();
        dir
    }

    fn card(dir: &Path, id: &str, theme: &str, tone: &str, rows: u16, session: &str) {
        let meta = serde_json::json!({
            "cols": 58, "rows": rows, "title": ROW_VALUE, "frames": 0,
            "session": session, "theme": theme,
        });
        let call = serde_json::json!({
            "call": { "v": 1, "form": "column", "title": ROW_VALUE,
                      "claim": { "kind": "change", "tone": tone } },
            "omni": [true],
            "look": { "term_cols": 154, "term_rows": 50, "cell_px": [16, 32], "scale": 2.0 },
        });
        let cards = dir.join("cards");
        std::fs::write(cards.join(format!("{id}.json")), meta.to_string()).unwrap();
        std::fs::write(cards.join(format!("{id}.call.json")), call.to_string()).unwrap();
        std::fs::write(cards.join(format!("{id}.png")), b"png").unwrap();
    }

    #[test]
    fn warp_reads_its_window_as_points_and_the_wrapper_draws_for_every_harness() {
        let dir = cache("warp");
        let lines = run(&[("TERM_PROGRAM", "WarpTerminal")], &dir, Some((8, 16)));
        assert!(
            has(&lines, "pass version: tiles 0.0.0 is release v0.0.0"),
            "{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("pass terminal: warp, route kitty-precut")),
            "{lines:#?}"
        );
        assert!(has(
            &lines,
            "pass graphics: kitty yes, sixel no, for the kitty-precut route"
        ));
        assert!(
            has(
                &lines,
                "pass cell size: TILES_CELL_PX=16x32, from the window size, read as points; CSI 16 t reply none; window size 8x16 per cell, read as points 16x32"
            ),
            "{lines:#?}"
        );
        assert!(has(
            &lines,
            "pass rows: TILES_ROWS=50 on the kitty-precut route, so a card stands at most 40 rows"
        ));
        assert!(
            has(&lines, "pass pi: /opt/bin/pi, drawn by the wrapper"),
            "{lines:#?}"
        );
        assert!(has(
            &lines,
            "pass claude: /opt/bin/claude, drawn by the wrapper"
        ));
        assert!(has(
            &lines,
            "pass opencode: /opt/bin/opencode, drawn by the wrapper"
        ));
        assert!(!lines.iter().any(|l| l.contains("codex")));
        assert!(has(
            &lines,
            &format!(
                "pass theme: {} for every harness, from the default",
                Theme::default().name()
            )
        ));
        let cards = dir.join("cards").display().to_string();
        assert!(
            has(&lines, &format!("pass cards: {cards}, 0 saved")),
            "{lines:#?}"
        );
        let silent = run(&[("TERM_PROGRAM", "WarpTerminal")], &dir, None);
        assert!(
            has(
                &silent,
                "fail cell size: none found, so cards use 16x32 device px; CSI 16 t reply none; window size none"
            ),
            "{silent:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_sixel_terminal_gets_no_rows_and_keeps_its_window_as_pixels() {
        let dir = cache("sixel");
        let lines = run(
            &[("TILES_TERMINAL", "foot"), ("TILES_ROUTE", "sixel")],
            &dir,
            Some((16, 32)),
        );
        assert!(
            has(
                &lines,
                "pass rows: the sixel route draws a tall card cut at the screen's edge, so it gets no TILES_ROWS"
            ),
            "{lines:#?}"
        );
        assert!(
            has(
                &lines,
                "pass cell size: TILES_CELL_PX=16x32, from the window size in pixels; CSI 16 t reply none; window size 16x32 per cell, as device px 16x32"
            ),
            "{lines:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kitty_keeps_its_reply_and_needs_no_rows() {
        let dir = cache("kitty");
        let lines = run(
            &[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")],
            &dir,
            Some((18, 36)),
        );
        assert!(
            has(
                &lines,
                "pass rows: the kitty-placeholders route scrolls a whole card, so it needs no TILES_ROWS"
            ),
            "{lines:#?}"
        );
        assert!(
            has(&lines, "pass pi: /opt/bin/pi, drawn by Pi itself"),
            "{lines:#?}"
        );
        assert!(has(
            &lines,
            "pass claude: /opt/bin/claude, drawn by Claude Code's Image mod"
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_saved_card_shows_its_theme_and_tone_and_never_a_row_value() {
        let dir = cache("cards");
        let default = Theme::default().name();
        card(&dir, "f1099b", default, "bad", 25, "00000000000000aa");
        card(&dir, "a1b2c3", "dark", "good", 45, "00000000000000aa");
        let lines = run(&[("TERM_PROGRAM", "WarpTerminal")], &dir, Some((8, 16)));
        assert!(
            has(
                &lines,
                &format!(
                    "pass card f1099b: column, {default} theme, tone bad, a bad tone draws its highlight red (playbook K2), 25 rows of 50, at most 40"
                )
            ),
            "{lines:#?}"
        );
        assert!(
            has(
                &lines,
                "fail card a1b2c3: column, dark theme, tone good, highlight in the accent, 45 rows of 50, at most 40"
            ),
            "{lines:#?}"
        );
        assert!(lines.iter().all(|l| !l.contains(ROW_VALUE)), "{lines:#?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One trace file, rewritten case by case.
    fn the_trace_check_reads_the_last_session_only() {
        let dir = cache("trace");
        card(
            &dir,
            "aaaaaa",
            Theme::default().name(),
            "neutral",
            20,
            "00000000000000aa",
        );
        card(
            &dir,
            "bbbbbb",
            Theme::default().name(),
            "neutral",
            20,
            "00000000000000bb",
        );
        card(
            &dir,
            "cccccc",
            Theme::default().name(),
            "neutral",
            20,
            "00000000000000bb",
        );
        let trace = dir.join("trace.txt");
        let header =
            |s| crate::wrap::trace_header(s, tiles_term::detect::Route::KittyPrecut, (50, 154));
        let first = format!(
            "{}\n   10.0 aaaaaa whole: rows 1 to 20 from screen row 3\n",
            header(0xaa)
        );
        let path = trace.display().to_string();
        let vars = [
            ("TERM_PROGRAM", "WarpTerminal"),
            ("TILES_TRACE", path.as_str()),
        ];
        std::fs::write(&trace, &first).unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!("pass trace: the last session in {path} saw its 1 card whole: aaaaaa")
            ),
            "{lines:#?}"
        );
        std::fs::write(
            &trace,
            format!(
                "{first}{}\n   12.0 bbbbbb whole: rows 1 to 20 from screen row 3\n",
                header(0xbb)
            ),
        )
        .unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!(
                    "fail trace: the last session in {path} made 2 cards and never saw cccccc whole, so it stayed blank"
                )
            ),
            "{lines:#?}"
        );
        std::fs::write(
            &trace,
            format!(
                "{first}{}\n   12.0 bbbbbb whole: rows 1 to 20 from screen row 3\n   13.0 cccccc whole: rows 1 to 20 from screen row 30\n",
                header(0xbb)
            ),
        )
        .unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!(
                    "pass trace: the last session in {path} saw all 2 cards whole: bbbbbb, cccccc"
                )
            ),
            "{lines:#?}"
        );
        // OpenCode drew cccccc's rows again as links: its last event deleted
        // it while its block was on screen. A card that scrolled away, or one
        // placed again after its copy was lost, passes.
        let lost = |last: &str| {
            format!(
                "{first}{}\n   12.0 bbbbbb whole: rows 1 to 20 from screen row 3\n   13.0 cccccc whole: rows 1 to 20 from screen row 30\n   14.0 bbbbbb scrolled away; pictures deleted\n   15.0 cccccc copy lost at screen row 30; pictures deleted\n{last}",
                header(0xbb)
            )
        };
        std::fs::write(
            &trace,
            lost("   16.0 row 30 written back from column 0 over 20 columns\n"),
        )
        .unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!(
                    "fail trace: the last session in {path} saw all 2 cards whole, then lost cccccc on screen and deleted its pictures"
                )
            ),
            "{lines:#?}"
        );
        // The entrance's own later lines about cccccc do not hide the loss.
        std::fs::write(
            &trace,
            lost("   16.0 cccccc entrance in the terminal\n   17.0 cccccc entrance over\n"),
        )
        .unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!(
                    "fail trace: the last session in {path} saw all 2 cards whole, then lost cccccc on screen and deleted its pictures"
                )
            ),
            "{lines:#?}"
        );
        std::fs::write(
            &trace,
            lost("   16.0 cccccc rows 1 to 20 placed settled at screen row 30\n"),
        )
        .unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!(
                    "pass trace: the last session in {path} saw all 2 cards whole: bbbbbb, cccccc"
                )
            ),
            "{lines:#?}"
        );
        std::fs::write(&trace, format!("{}\n", header(0xaa))).unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!(
                    "fail trace: the last session in {path} made 1 card and never saw aaaaaa whole, so it stayed blank"
                )
            ),
            "{lines:#?}"
        );
        // A launcher build of OpenCode, 2026-10-19: the session saw cards a
        // background service's tiles server made for another session.
        std::fs::write(
            &trace,
            format!(
                "{first}{}\n   14.0 bbbbbb whole: rows 1 to 20 from screen row 3\n",
                header(0xcc)
            ),
        )
        .unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!(
                    "fail trace: the last session in {path} made no card; it saw bbbbbb whole, made by another session's tiles server, so the harness did not call this session's"
                )
            ),
            "{lines:#?}"
        );
        std::fs::write(&trace, format!("{}\n", header(0xcc))).unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            has(
                &lines,
                &format!("fail trace: the last session in {path} made no card")
            ),
            "{lines:#?}"
        );
        std::fs::write(&trace, "  1.0 aaaaaa whole:\n").unwrap();
        let lines = run(&vars, &dir, None);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("fail trace:") && l.contains("has no session"))
        );
        let lines = run(&[("TILES_TRACE", "/nonexistent/trace")], &dir, None);
        assert!(!lines.iter().any(|l| l.contains("trace:")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_opencode_lines_name_its_server_and_an_install_beside_it() {
        use crate::opencode::{Cli, Found};
        let dir = cache("opencode");
        let env = |k: &str| (k == "TERM_PROGRAM").then(|| "WarpTerminal".to_string());
        let which = |p: &str| (p == "opencode").then(|| PathBuf::from("/u/.local/bin/opencode"));
        let lines = |found: &Found, install: Option<&str>| {
            report(&Inputs {
                env: &env,
                replies: None,
                not_asked: Some("not a terminal"),
                screen: crate::run::Screen {
                    size: (24, 100),
                    window: None,
                },
                which: &which,
                cache: Ok(dir.clone()),
                tag: None,
                opencode: Some(found),
                opencode_install: install.map(PathBuf::from),
                reduce_motion: None,
                versions: &[],
            })
        };
        let launcher = Found {
            cli: Some(Cli::Launcher),
            version: Some("2.0.32".into()),
        };
        let out = lines(&launcher, None);
        assert!(
            has(
                &out,
                "pass opencode server: OpenCode 2.0.32, a launcher with a background service: tiles run passes --standalone for a private server, which loads the session's tiles server; proved on 2.0.32"
            ),
            "{out:#?}"
        );
        assert!(!out.iter().any(|l| l.contains("opencode install")));
        let out = lines(&launcher, Some("/u/.config/opencode/config.json"));
        assert!(
            out.iter().any(|l| l.starts_with(
                "fail opencode install: /u/.config/opencode/config.json holds a tiles server too, and Tiles has not proved"
            )),
            "{out:#?}"
        );
        let public = Found {
            cli: Some(Cli::Public),
            version: Some("1.19.3".into()),
        };
        let out = lines(&public, Some("/u/.config/opencode/config.json"));
        assert!(
            has(
                &out,
                "pass opencode install: /u/.config/opencode/config.json holds a tiles server too; OpenCode merges OPENCODE_CONFIG_CONTENT after it, so the session's replaces it"
            ),
            "{out:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_no_terminal_to_ask_the_graphics_line_names_dev_tty() {
        let dir = cache("no-tty");
        let env = |k: &str| (k == "TERM_PROGRAM").then(|| "WarpTerminal".to_string());
        let which = |_: &str| None;
        let why = "stdin or stdout is not a terminal and /dev/tty cannot be opened: Device not configured (os error 6)";
        let lines = report(&Inputs {
            env: &env,
            replies: None,
            not_asked: Some(why),
            screen: crate::run::Screen {
                size: (24, 100),
                window: None,
            },
            which: &which,
            cache: Ok(dir.clone()),
            tag: None,
            opencode: None,
            opencode_install: None,
            reduce_motion: None,
            versions: &[],
        });
        assert!(
            has(&lines, &format!("fail graphics: not asked; {why}")),
            "{lines:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ADR-032: a harness version Tiles was not proved on fails the line
    /// after its harness, one that cannot be read passes and says so, and
    /// `OpenCode`'s stays the `opencode server:` line.
    #[test]
    fn each_harness_version_gets_a_line_after_its_harness() {
        let dir = cache("versions");
        let warp = [("TERM_PROGRAM", "WarpTerminal")];
        let versions = [
            (Harness::Pi, Some("1.3.4".to_string())),
            (Harness::OpenCode, Some("1.19.3".to_string())),
            (Harness::Claude, Some("2.2.0".to_string())),
        ];
        let lines = run_with(&warp, &dir, None, &versions);
        let at = |want: &str| {
            lines
                .iter()
                .position(|l| l == want)
                .unwrap_or_else(|| panic!("{want}\n{}", lines.join("\n")))
        };
        assert_eq!(
            at("pass pi version: Pi 1.3.4, proved on 1.4.0 (ADR-032)"),
            at("pass pi: /opt/bin/pi, drawn by the wrapper") + 1
        );
        assert_eq!(
            at(
                "fail claude version: Claude Code 2.2.0, not the major and minor version Tiles was proved on, 2.1.309 (ADR-032)"
            ),
            at("pass claude: /opt/bin/claude, drawn by the wrapper") + 1
        );
        assert!(!lines.iter().any(|l| l.contains("opencode version")));
        assert!(
            !lines.iter().any(|l| l.contains("codex version")),
            "codex is not on PATH"
        );
        let lines = run_with(&warp, &dir, None, &[(Harness::Claude, None)]);
        assert!(has(
            &lines,
            "pass claude version: claude --version could not be read; Tiles was proved on 2.1.309"
        ));
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/hostile/text.json"
        );
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for e in json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
        {
            let lines = run_with(&warp, &dir, None, &[(Harness::Pi, Some(e.to_string()))]);
            assert!(
                lines
                    .iter()
                    .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char)),
                "{e:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_development_build_or_another_tag_fails_the_version_check() {
        assert_eq!(
            super::version(Some("v0.1.1")),
            "fail version: tiles 0.0.0 is not release v0.1.1"
        );
        assert_eq!(
            super::version(None),
            "fail version: tiles 0.0.0, a build that carries no release tag"
        );
    }

    #[test]
    fn hostile_environment_values_are_printed_plain() {
        let dir = cache("hostile");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/hostile/text.json"
        );
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for e in json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
        {
            for key in [
                "TILES_CELL_PX",
                "TILES_TERMINAL",
                "TILES_ROUTE",
                "TILES_THEME",
                "TILES_CACHE_DIR",
            ] {
                let lines = run(&[("TERM_PROGRAM", "WarpTerminal"), (key, e)], &dir, None);
                assert!(
                    lines
                        .iter()
                        .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char)),
                    "{key} {e:?}"
                );
            }
            // What OpenCode answers and where its config sits are printed too.
            let env = |_: &str| None;
            let which = |_: &str| None;
            let found = crate::opencode::Found {
                cli: None,
                version: Some(e.to_string()),
            };
            let lines = report(&Inputs {
                env: &env,
                replies: None,
                not_asked: Some(e),
                screen: crate::run::Screen {
                    size: (24, 100),
                    window: None,
                },
                which: &which,
                cache: Ok(dir.clone()),
                tag: Some(e),
                opencode: Some(&found),
                opencode_install: Some(PathBuf::from(e)),
                reduce_motion: None,
                versions: &[],
            });
            assert!(
                lines
                    .iter()
                    .any(|l| l.starts_with("fail opencode install:"))
            );
            assert!(
                lines
                    .iter()
                    .all(|l| !l.chars().any(tiles_intent::text::is_unsafe_char)),
                "opencode {e:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod motion_tests {
    use tiles_term::detect::Replies;

    use super::{Inputs, report};

    /// The report's motion line in `terminal` (`TERM_PROGRAM`) with
    /// `TILES_MOTION` at `setting` and macOS's Reduce Motion at `system`.
    fn motion(terminal: &str, setting: Option<&str>, system: Option<bool>) -> String {
        let env = |k: &str| match k {
            "TERM_PROGRAM" => Some(terminal.to_string()),
            "TILES_MOTION" => setting.map(str::to_string),
            _ => None,
        };
        let which = |_: &str| None;
        let replies = Replies::parse(b"\x1b_Gi=31;OK\x1b\\\x1b[?62c");
        report(&Inputs {
            env: &env,
            replies: Some(&replies),
            not_asked: None,
            screen: crate::run::Screen {
                size: (50, 154),
                window: Some((8, 16)),
            },
            which: &which,
            cache: Err("no cache".into()),
            tag: None,
            opencode: None,
            opencode_install: None,
            reduce_motion: system,
            versions: &[],
        })
        .into_iter()
        .find(|l| l.contains(" motion: "))
        .unwrap_or_default()
    }

    #[test]
    fn check_names_the_entrance_the_route_plays() {
        assert_eq!(
            motion("WarpTerminal", None, Some(false)),
            "pass motion: entrance on the kitty-precut route, 10 frames at 50 ms"
        );
        assert_eq!(
            motion("WarpTerminal", None, Some(true)),
            "pass motion: reduce, 4 frames over 150 ms"
        );
        assert_eq!(
            motion("WarpTerminal", Some("off"), Some(true)),
            "pass motion: off"
        );
        assert_eq!(
            motion("WarpTerminal", Some("on"), Some(true)),
            "pass motion: entrance on the kitty-precut route, 10 frames at 50 ms",
            "TILES_MOTION wins over the system"
        );
    }
}
