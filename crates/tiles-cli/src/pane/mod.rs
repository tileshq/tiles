//! `tiles pane <intent.json | card id>`: the interactive dashboard pane
//! (ADR-063, ADR-065). It draws the cards of a chart call as the chart tool
//! takes it, or of a card the chart tool drew, with their rows: the rows
//! the call holds, or rows fetched from Omni or a warehouse through the
//! same source seam as an inline card, held in this process's memory only
//! (ADR-027).
//!
//! It takes the whole terminal in the alternate screen and raw mode, lays
//! the cards out with the dashboard planner at the pane's width and sends
//! each card as its own kitty picture. Hovering a mark sends one small
//! picture over its card, the mark's outline and its tooltip. What the user
//! picks is one [`state::State`] every card is laid out from: a click on a
//! mark filters the other cards' rows in memory, a click on a legend entry
//! hides its series, `s` sorts the card under the pointer, `d` drills into
//! the period under it by asking Omni for the next finer timeframe, `u`
//! goes back up and `r` clears it all; each sends only the cards whose
//! scene changed. A resize lays everything out again. In a build with debug
//! assertions a fixture holding `"stress": "table"` draws a grid of text
//! that the wheel scrolls, to price a table before a table form exists.
//!
//! Every byte to the terminal comes from `tiles_term::pane` and
//! `tiles_term::kitty`; input goes through `tiles_term::input`. The screen
//! is given back on every way out: `q`, Control-C or Control-D, SIGINT,
//! SIGTERM, SIGHUP or SIGQUIT, the end of input, an error, and a panic.

mod board;
mod screen;
mod state;
mod trace;

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tiles_engine::{Moment, Scene};
use tiles_intent::{Intent, one_line};
use tiles_mcp::card::{Call, EnginePainter, Fetch, Look, Painter as _, Query, Tile};
use tiles_mcp::source::Source;
use tiles_term::detect::Terminal;
use tiles_term::input::{Event, Key, MouseKind, Parser};
use tiles_term::{kitty, pane};

use board::Slot;
use state::{Drilled, State};
use trace::Trace;

/// The command's usage line.
pub const USAGE: &str = "tiles pane <intent.json | card id> [--trace <file>]";

/// Rows of the stress grid on screen at once.
const STRESS_ROWS: usize = 40;
/// Most stress grid rows a fixture may hold.
const STRESS_MAX_ROWS: usize = 10_000;
/// Most stress grid columns.
const STRESS_MAX_COLS: usize = 8;
/// First kitty image id the pane uses; each card has two, swapped on
/// every redraw so the old picture goes only once the new one is placed.
const FIRST_ID: u32 = 7_000_000;
/// The hover overlay's two image ids.
const OVERLAY_IDS: [u32; 2] = [6_999_990, 6_999_991];
/// Stacking order of cards and of the overlay above them.
const Z_CARD: i32 = 1;
const Z_OVERLAY: i32 = 2;
/// How long the start-up probe waits for the terminal's replies.
const PROBE_WAIT: Duration = Duration::from_millis(500);
/// How often `--trace` samples memory and CPU time.
const SAMPLE_EVERY: Duration = Duration::from_secs(1);

/// The command line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    fixture: String,
    trace: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut fixture = None;
    let mut trace = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--trace" => {
                trace = Some(PathBuf::from(
                    it.next().ok_or("--trace needs a file")?.as_str(),
                ));
            }
            "-" => {
                return Err(
                    "the pane reads the keyboard on standard input; pass the call as a file".into(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag {}; usage: {USAGE}", one_line(flag)));
            }
            path if fixture.is_none() => fixture = Some(path.to_string()),
            _ => return Err(format!("usage: {USAGE}")),
        }
    }
    Ok(Args {
        fixture: fixture.ok_or(format!("usage: {USAGE}"))?,
        trace,
    })
}

/// The call `input` names, and its name on the status line: a file's
/// contents, or the call a card was drawn from when `input` is no file but
/// a card id.
fn read_call(input: &str) -> Result<(String, String), String> {
    if !std::path::Path::new(input).exists() && tiles_term::block::is_card_id(input) {
        let cache = crate::serve::open_cache()?;
        let json = tiles_mcp::card::saved_call(&cache, input)?;
        return Ok((json, format!("card {input}")));
    }
    let json = crate::read_input(input)?;
    let name = std::path::Path::new(input)
        .file_name()
        .map_or_else(|| input.to_string(), |n| n.to_string_lossy().into_owned());
    Ok((json, name))
}

/// What the pane shows.
#[allow(clippy::large_enum_variant)] // One value per pane, made once.
enum Content {
    /// A chart call's cards, drawn from `state` as `shown`.
    Dashboard {
        call: Call,
        shown: Vec<Intent>,
        state: State,
    },
    /// The text grid, scrolled `offset` rows down its rows.
    Stress {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        offset: usize,
    },
}

/// Reads a call, fetching the rows it names through `fetch`; in a build
/// with debug assertions, the stress grid when it holds
/// `"stress": "table"`.
fn content(json: &str, painter: &EnginePainter, fetch: &Fetch<'_>) -> Result<Content, String> {
    if cfg!(debug_assertions) {
        let value: Option<serde_json::Value> = serde_json::from_str(json).ok();
        if let Some(v) = value.as_ref().filter(|v| v.get("stress").is_some()) {
            return stress(v);
        }
    }
    let call = Call::read(json, painter, fetch).map_err(|e| e.to_string())?;
    Ok(Content::Dashboard {
        shown: call.intents.clone(),
        call,
        state: State::default(),
    })
}

fn stress(v: &serde_json::Value) -> Result<Content, String> {
    if v["stress"] != "table" {
        return Err("stress: the only stress scene is \"table\"".into());
    }
    let strings = |v: &serde_json::Value, key: &str| -> Result<Vec<String>, String> {
        v.as_array()
            .ok_or_else(|| format!("{key} must be a list of text"))?
            .iter()
            .map(|c| {
                c.as_str()
                    .map(tiles_intent::text::sanitize)
                    .ok_or_else(|| format!("{key} must be a list of text"))
            })
            .collect()
    };
    let header = strings(&v["columns"], "columns")?;
    if !(1..=STRESS_MAX_COLS).contains(&header.len()) {
        return Err(format!("columns: 1 to {STRESS_MAX_COLS} columns"));
    }
    let rows = v["rows"]
        .as_array()
        .ok_or("rows must be a list of rows")?
        .iter()
        .map(|r| strings(r, "each row"))
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() || rows.len() > STRESS_MAX_ROWS {
        return Err(format!("rows: 1 to {STRESS_MAX_ROWS} rows"));
    }
    Ok(Content::Stress {
        header,
        rows,
        offset: 0,
    })
}

/// The pane's whole state.
struct App<'a> {
    painter: EnginePainter,
    /// Fetches a drill's rows through the source seam.
    fetch: &'a Fetch<'a>,
    content: Content,
    look: Look,
    /// The terminal detected at start, for reading a new cell size.
    terminal: Terminal,
    /// One cell in device pixels.
    cell: (u16, u16),
    /// The terminal's rows and columns.
    size: (u16, u16),
    /// Whether mouse reports are in pixels (mode 1016).
    pixels: bool,
    slots: Vec<Slot>,
    /// Each slot's image id on screen.
    ids: Vec<u32>,
    /// The overlay on screen: its slot and hover target, and its image id.
    overlay: Option<((usize, usize), u32)>,
    /// The pointer's last place in device pixels, which the keys act at.
    pointer: Option<(u32, u32)>,
    /// The fixture's name, for the status line.
    name: String,
    /// A note for the status line, such as a card that refused its rows.
    note: Option<String>,
    trace: Arc<Trace>,
    update: u64,
}

/// One update's bytes, and what the trace says about it.
struct Update {
    bytes: Vec<u8>,
    cards: usize,
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(
        Vec::with_capacity(data.len() / 8),
        flate2::Compression::fast(),
    );
    let _ = z.write_all(data);
    z.finish().unwrap_or_default()
}

impl App<'_> {
    fn engine(&self) -> &tiles_engine::Engine {
        self.painter.engine()
    }

    /// Rows the cards may cover: all but the status line.
    fn card_rows(&self) -> u16 {
        self.size.0.saturating_sub(1)
    }

    fn base_look(&self) -> Look {
        Look {
            term_cols: self.size.1,
            term_rows: None,
            cell_px: (f32::from(self.cell.0), f32::from(self.cell.1)),
            frames: 0,
            session: None,
            ..self.look
        }
    }

    /// Lays every card out again for the pane's size.
    fn lay_out(&mut self) -> Result<(), String> {
        let look = self.base_look();
        self.look = look;
        let scale = look.scale;
        self.slots = match &self.content {
            Content::Dashboard { call, shown, .. } => {
                let (laid, order) = call
                    .lay_out(shown, &self.painter, &look)
                    .map_err(|e| e.to_string())?;
                let mut order = order.into_iter();
                laid.cards
                    .into_iter()
                    .map(|c| Slot {
                        intent: c.align.and_then(|_| order.next()),
                        align: c.align,
                        at: c.at,
                        cells: c.cells,
                        hits: tiles_engine::hit::hit_map(&c.scene, scale),
                        legend: self.painter.engine().legend(&c.scene, scale),
                        scene: c.scene,
                    })
                    .collect()
            }
            Content::Stress { .. } => {
                let scene = self.stress_scene()?;
                let rows = cell_rows(scene.height, look.cell_height_css());
                vec![Slot {
                    intent: None,
                    align: None,
                    at: (0, 0),
                    cells: (look.term_cols.saturating_sub(1).max(1), rows),
                    hits: Vec::new(),
                    legend: Vec::new(),
                    scene,
                }]
            }
        };
        self.ids = (0..self.slots.len())
            .map(|i| FIRST_ID + 2 * u32::try_from(i).unwrap_or(0))
            .collect();
        self.overlay = None;
        Ok(())
    }

    fn stress_scene(&self) -> Result<Scene, String> {
        let Content::Stress {
            header,
            rows,
            offset,
        } = &self.content
        else {
            return Err("not the stress scene".into());
        };
        let shown: Vec<Vec<String>> = (0..STRESS_ROWS.min(rows.len()))
            .map(|i| rows[(offset + i) % rows.len()].clone())
            .collect();
        let look = self.base_look();
        let width = look
            .width_css(look.term_cols.saturating_sub(1).max(1))
            .clamp(tiles_engine::MIN_WIDTH, tiles_engine::MAX_WIDTH);
        self.engine()
            .text_grid(
                header,
                &shown,
                &tiles_engine::Options {
                    width,
                    theme: look.theme,
                    cell_height: Some(look.cell_height_css().clamp(1.0, 200.0)),
                },
            )
            .map_err(|e| e.to_string())
    }

    /// The bytes that place slot `i` as image `id`, cut to its own rows and
    /// to the rows above the status line; empty when none of it shows.
    fn card_bytes(&self, i: usize, id: u32) -> Result<Vec<u8>, String> {
        let slot = &self.slots[i];
        let room = self.card_rows().saturating_sub(slot.at.1).min(slot.cells.1);
        if room == 0 || slot.at.0 >= self.size.1 {
            return Ok(Vec::new());
        }
        let frame = self
            .engine()
            .frame_at(&slot.scene, self.look.scale, Moment::FINAL)
            .map_err(|e| e.to_string())?;
        let width = u32::try_from(frame.width).unwrap_or(0);
        let height = u32::try_from(frame.height).unwrap_or(0);
        let rgba = tiles_term::image::Rgba::new(width, height, frame.rgba)
            .ok_or("the engine drew a frame of the wrong size")?
            .rows(0, u32::from(room) * u32::from(self.cell.1));
        let mut out = pane::move_to(slot.at.1, slot.at.0).into_bytes();
        out.extend(kitty::show_rgba_zlib(
            id,
            rgba.width,
            rgba.height,
            &zlib(&rgba.data),
            kitty::Placement {
                placement: 1,
                x_px: 0,
                y_px: 0,
                z: Z_CARD,
            },
        ));
        Ok(out)
    }

    fn status_bytes(&self) -> Vec<u8> {
        let mut text = format!("tiles pane · {} · q quits", self.name);
        match &self.content {
            Content::Dashboard { state, .. } if !state.is_empty() => {
                if let Some(s) = &state.filter {
                    let _ = write!(text, " · {} = {}", s.column, s.value);
                }
                let levels: usize = state.drills.values().map(Vec::len).sum();
                if levels > 0 {
                    let _ = write!(text, " · drilled {levels}, u goes up");
                }
                let hidden: usize = state
                    .hidden
                    .values()
                    .map(std::collections::BTreeSet::len)
                    .sum();
                if hidden > 0 {
                    let _ = write!(text, " · {hidden} hidden");
                }
                if !state.sorts.is_empty() {
                    let _ = write!(text, " · {} sorted", state.sorts.len());
                }
                text.push_str(" · r clears");
            }
            Content::Stress { offset, rows, .. } => {
                let _ = write!(
                    text,
                    " · rows {}–{} of {}, wheel scrolls",
                    offset + 1,
                    (offset + STRESS_ROWS.min(rows.len()) - 1) % rows.len() + 1,
                    rows.len()
                );
            }
            Content::Dashboard { .. } => text
                .push_str(" · click a mark to filter, a legend entry to hide · s sorts · d drills"),
        }
        if let Some(note) = &self.note {
            text.push_str(" · ");
            text.push_str(note);
        }
        pane::status(self.size.0.saturating_sub(1), self.size.1, &text).into_bytes()
    }

    fn begin(&mut self, kind: &str) -> u64 {
        self.update += 1;
        self.trace
            .line("begin", format_args!("update={} kind={kind}", self.update));
        self.update
    }

    /// Every card from nothing: the first frame and after a resize.
    fn draw_all(&mut self, kind: &str) -> Result<Update, String> {
        let update = self.update;
        let mut bytes = pane::BEGIN_UPDATE.as_bytes().to_vec();
        bytes.extend(pane::CLEAR.as_bytes());
        for i in 0..self.slots.len() {
            bytes.extend(self.card_bytes(i, self.ids[i])?);
            self.trace
                .line("draw", format_args!("update={update} kind={kind} card={i}"));
        }
        bytes.extend(self.status_bytes());
        bytes.extend(pane::END_UPDATE.as_bytes());
        Ok(Update {
            bytes,
            cards: self.slots.len(),
        })
    }

    /// Slot `i` drawn again as its other image id, the old one deleted once
    /// the new one is placed.
    fn replace(&mut self, i: usize, kind: &str, bytes: &mut Vec<u8>) -> Result<(), String> {
        let old = self.ids[i];
        let new = if old % 2 == FIRST_ID % 2 {
            old + 1
        } else {
            old - 1
        };
        bytes.extend(self.card_bytes(i, new)?);
        bytes.extend(kitty::delete(old));
        self.ids[i] = new;
        self.trace.line(
            "draw",
            format_args!("update={} kind={kind} card={i}", self.update),
        );
        Ok(())
    }

    /// The slot and the hover target under device pixel `x`, `y`, read from
    /// the hit maps of the scenes on screen.
    fn target(&self, x: u32, y: u32) -> Option<(usize, usize)> {
        board::slot_at(&self.slots, self.cell, x, y)
            .and_then(|(s, (dx, dy))| board::hit_index(&self.slots[s].hits, dx, dy).map(|h| (s, h)))
    }

    /// Places the overlay for `target` as the other overlay id, or none,
    /// and deletes the old one once the new one is placed.
    fn set_overlay(
        &mut self,
        target: Option<(usize, usize)>,
        kind: &str,
        bytes: &mut Vec<u8>,
    ) -> Result<(), String> {
        let old = self.overlay.take().map(|(_, id)| id);
        if let Some((s, h)) = target {
            let id = if old == Some(OVERLAY_IDS[0]) {
                OVERLAY_IDS[1]
            } else {
                OVERLAY_IDS[0]
            };
            if let Some(placed) = self.overlay_bytes(s, h, id, kind)? {
                bytes.extend(placed);
                self.overlay = Some(((s, h), id));
            }
        }
        if let Some(old) = old {
            bytes.extend(kitty::delete(old));
        }
        Ok(())
    }

    /// The pointer at device pixel `x`, `y`: the overlay for the mark under
    /// it, or none; `None` when that is what the screen already shows.
    fn hover(&mut self, x: u32, y: u32) -> Result<Option<Update>, String> {
        let target = self.target(x, y);
        if target == self.overlay.map(|(t, _)| t) {
            return Ok(None);
        }
        self.begin("hover");
        let mut bytes = pane::BEGIN_UPDATE.as_bytes().to_vec();
        self.set_overlay(target, "hover", &mut bytes)?;
        bytes.extend(pane::END_UPDATE.as_bytes());
        Ok(Some(Update { bytes, cards: 0 }))
    }

    fn overlay_bytes(
        &self,
        s: usize,
        h: usize,
        id: u32,
        kind: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        let slot = &self.slots[s];
        let scale = self.look.scale;
        let overlay = self.engine().hover(&slot.scene, &slot.hits[h], scale);
        let frame = self
            .engine()
            .frame_at(&overlay.scene, scale, Moment::FINAL)
            .map_err(|e| e.to_string())?;
        self.trace.line(
            "draw",
            format_args!("update={} kind={kind} card=overlay", self.update),
        );
        let (cw, ch) = (i64::from(self.cell.0), i64::from(self.cell.1));
        let x = i64::from(slot.at.0) * cw + i64::from(overlay.left);
        let y = i64::from(slot.at.1) * ch + i64::from(overlay.top);
        let (col, row) = (x.div_euclid(cw), y.div_euclid(ch));
        let (Ok(col), Ok(row)) = (u16::try_from(col), u16::try_from(row)) else {
            return Ok(None);
        };
        if row >= self.card_rows() {
            return Ok(None);
        }
        let mut out = pane::move_to(row, col).into_bytes();
        out.extend(kitty::show_rgba_zlib(
            id,
            u32::try_from(frame.width).unwrap_or(0),
            u32::try_from(frame.height).unwrap_or(0),
            &zlib(&frame.rgba),
            kitty::Placement {
                placement: 1,
                x_px: u16::try_from(x.rem_euclid(cw)).unwrap_or(0),
                y_px: u16::try_from(y.rem_euclid(ch)).unwrap_or(0),
                z: Z_OVERLAY,
            },
        ));
        Ok(Some(out))
    }

    /// The card under device pixel `x`, `y`, as its intent's index.
    fn card_at(&self, x: u32, y: u32) -> Option<usize> {
        board::slot_at(&self.slots, self.cell, x, y).and_then(|(s, _)| self.slots[s].intent)
    }

    /// The card and the row of the mark under device pixel `x`, `y`.
    fn mark_at(&self, x: u32, y: u32) -> Option<(usize, usize)> {
        board::slot_at(&self.slots, self.cell, x, y).and_then(|(s, (dx, dy))| {
            let slot = &self.slots[s];
            let hit = board::hit_index(&slot.hits, dx, dy)?;
            Some((slot.intent?, slot.hits[hit].row?))
        })
    }

    /// The card and the series of the legend entry under device pixel `x`,
    /// `y`.
    fn legend_at(&self, x: u32, y: u32) -> Option<(usize, String)> {
        let Content::Dashboard { call, state, .. } = &self.content else {
            return None;
        };
        let (s, (dx, dy)) = board::slot_at(&self.slots, self.cell, x, y)?;
        let slot = &self.slots[s];
        let entry = board::legend_index(&slot.legend, dx, dy)?;
        let card = slot.intent?;
        let name =
            state::legend_series(state.base(&call.intents, card)?, &slot.legend[entry].label)?;
        Some((card, name))
    }

    /// A left click at `x`, `y`: on a legend entry, hide or show its series;
    /// on a mark, select its value and filter the other cards, or clear the
    /// selection when it is selected already or the click hit no mark.
    fn click(&mut self, x: u32, y: u32) -> Result<Option<Update>, String> {
        let Content::Dashboard {
            call, shown, state, ..
        } = &self.content
        else {
            return Ok(None);
        };
        let mut next = state.clone();
        if let Some((card, name)) = self.legend_at(x, y) {
            return match next.toggle(&call.intents, card, &name) {
                Ok(()) => self.restate(next, "click", None).map(Some),
                Err(e) => Ok(Some(self.say("click", e))),
            };
        }
        let picked = self
            .mark_at(x, y)
            .and_then(|(card, row)| board::pick(shown, card, row));
        if picked == state.filter {
            if picked.is_none() {
                return Ok(None);
            }
            next.filter = None;
        } else {
            next.filter = picked;
        }
        self.restate(next, "click", None).map(Some)
    }

    /// `s`: steps the sort of the card under the pointer.
    fn sort(&mut self) -> Result<Option<Update>, String> {
        let Content::Dashboard { shown, state, .. } = &self.content else {
            return Ok(None);
        };
        let Some(card) = self.pointer.and_then(|(x, y)| self.card_at(x, y)) else {
            return Ok(Some(
                self.say("sort", "point at a card, then press s".into()),
            ));
        };
        let mut next = state.clone();
        match next.sort(&shown[card], card) {
            Ok(order) => {
                let note = format!(
                    "{}: {}",
                    shown[card].title,
                    order.map_or("its own order", state::Order::name)
                );
                self.restate(next, "sort", Some(note)).map(Some)
            }
            Err(e) => Ok(Some(self.say("sort", e))),
        }
    }

    /// `d`: drills the card under the pointer into the period of the mark
    /// under it, asking the card's source for the next finer timeframe.
    /// Only the drilled card changes; the rows outside the period are
    /// dropped as they arrive.
    fn drill(&mut self) -> Result<Option<Update>, String> {
        let Content::Dashboard {
            call, shown, state, ..
        } = &self.content
        else {
            return Ok(None);
        };
        let Some((card, row)) = self.pointer.and_then(|(x, y)| self.mark_at(x, y)) else {
            return Ok(Some(
                self.say("drill", "point at a period, then press d".into()),
            ));
        };
        let planned = plan_drill(call, shown, state, card, row);
        let mut next = state.clone();
        let (request, mut within, title) = match planned {
            Ok(p) => p,
            Err(e) => return Ok(Some(self.say("drill", e))),
        };
        let label = within.last().map(|w| w.1.clone()).unwrap_or_default();
        let wait = self.say("drill", format!("{title}: asking Omni for {label}…"));
        self.send("drill", &wait)?;
        let intent = match drilled(self.fetch, &self.painter, &request, &within) {
            Ok(i) => i,
            Err(e) => return Ok(Some(self.say("drill", e))),
        };
        let frame = request.fields.get("x").map_or("", |x| {
            x.rsplit('[').next().unwrap_or("").trim_end_matches(']')
        });
        let note = format!("{title}: {label} by {frame}");
        next.drills.entry(card).or_default().push(Drilled {
            request,
            within: within.pop().unwrap_or_default(),
            intent,
        });
        self.restate(next, "drill", Some(note)).map(Some)
    }

    /// `u`: takes the card under the pointer one drill level back up.
    fn undrill(&mut self) -> Result<Option<Update>, String> {
        let Content::Dashboard { state, .. } = &self.content else {
            return Ok(None);
        };
        let card = self.pointer.and_then(|(x, y)| self.card_at(x, y));
        let Some(card) = card.filter(|c| state.drills.contains_key(c)) else {
            return Ok(Some(
                self.say("drill", "point at a drilled card, then press u".into()),
            ));
        };
        let mut next = state.clone();
        if let Some(levels) = next.drills.get_mut(&card) {
            levels.pop();
            if levels.is_empty() {
                next.drills.remove(&card);
            }
        }
        self.restate(next, "drill", None).map(Some)
    }

    /// `r`: clears everything picked.
    fn reset(&mut self) -> Result<Option<Update>, String> {
        match &self.content {
            Content::Dashboard { state, .. } if !state.is_empty() => {
                self.restate(State::default(), "reset", None).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// The status line with `note` on it, and nothing else changed.
    fn say(&mut self, kind: &str, note: String) -> Update {
        self.begin(kind);
        self.note = Some(note);
        let mut bytes = pane::BEGIN_UPDATE.as_bytes().to_vec();
        bytes.extend(self.status_bytes());
        bytes.extend(pane::END_UPDATE.as_bytes());
        Update { bytes, cards: 0 }
    }

    /// Draws `next`: lays out again each card whose intent it changes,
    /// sends those whose scene changed, and moves the overlay to the mark
    /// under the pointer in the new scenes, or deletes it when no mark is
    /// there. `note` goes on the status line after any card that refused.
    fn restate(&mut self, next: State, kind: &str, note: Option<String>) -> Result<Update, String> {
        let Content::Dashboard { call, .. } = &self.content else {
            return Err("not a dashboard".into());
        };
        let wanted = next.apply(&call.intents);
        self.begin(kind);
        let mut bytes = pane::BEGIN_UPDATE.as_bytes().to_vec();
        let (intents, cards) = self.relay(wanted, &mut bytes)?;
        if self.note.is_none() {
            self.note = note;
        }
        let target = self.pointer.and_then(|(x, y)| self.target(x, y));
        self.set_overlay(target, kind, &mut bytes)?;
        if let Content::Dashboard { shown, state, .. } = &mut self.content {
            *shown = intents;
            *state = next;
        }
        bytes.extend(self.status_bytes());
        bytes.extend(pane::END_UPDATE.as_bytes());
        Ok(Update { bytes, cards })
    }

    /// Lays out again each card whose intent `intents` changes, in its own
    /// slot, and sends those whose scene changed. A card that refuses its
    /// filtered rows keeps its picture and its intent, and names itself on
    /// the status line. Returns the intents the cards now show, and how
    /// many cards it sent.
    fn relay(
        &mut self,
        mut intents: Vec<Intent>,
        bytes: &mut Vec<u8>,
    ) -> Result<(Vec<Intent>, usize), String> {
        let Content::Dashboard { shown, .. } = &self.content else {
            return Ok((intents, 0));
        };
        let look = self.look;
        let mut changed = Vec::new();
        let mut refused = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            let (Some(k), Some(align)) = (slot.intent, slot.align) else {
                continue;
            };
            if intents[k] == shown[k] {
                continue;
            }
            let tile = Tile {
                intent: &intents[k],
                cols: slot.cells.0,
                key: "",
            };
            let align = tiles_engine::Align {
                min_height: f32::from(slot.cells.1) * look.cell_height_css(),
                ..align
            };
            match self.painter.lay_card(&tile, &look, align) {
                Ok(scene) if scene != slot.scene => changed.push((i, scene)),
                Ok(_) => {}
                Err(_) => refused.push(k),
            }
        }
        let refused: Vec<String> = refused
            .into_iter()
            .map(|k| {
                intents[k] = shown[k].clone();
                intents[k].title.clone()
            })
            .collect();
        self.note = (!refused.is_empty()).then(|| {
            format!(
                "{} kept its picture: it cannot be drawn that way",
                refused.join(", ")
            )
        });
        let cards = changed.len();
        for (i, scene) in changed {
            self.slots[i].hits = tiles_engine::hit::hit_map(&scene, look.scale);
            self.slots[i].legend = self.painter.engine().legend(&scene, look.scale);
            self.slots[i].scene = scene;
            self.replace(i, "click", bytes)?;
        }
        Ok((intents, cards))
    }

    /// The stress grid moved `by` rows.
    fn scroll(&mut self, by: isize) -> Result<Option<Update>, String> {
        let Content::Stress { rows, offset, .. } = &mut self.content else {
            return Ok(None);
        };
        let n = rows.len();
        *offset = (*offset + n).saturating_add_signed(by % isize::try_from(n).unwrap_or(1)) % n;
        self.begin("scroll");
        let scene = self.stress_scene()?;
        self.slots[0].scene = scene;
        let mut bytes = pane::BEGIN_UPDATE.as_bytes().to_vec();
        self.replace(0, "scroll", &mut bytes)?;
        bytes.extend(self.status_bytes());
        bytes.extend(pane::END_UPDATE.as_bytes());
        Ok(Some(Update { bytes, cards: 1 }))
    }

    /// Writes an update and traces its end.
    fn send(&self, kind: &str, update: &Update) -> Result<(), String> {
        let mut out = std::io::stdout().lock();
        out.write_all(&update.bytes)
            .and_then(|()| out.flush())
            .map_err(|e| format!("cannot write to the terminal: {e}"))?;
        self.trace.line(
            "write",
            format_args!(
                "update={} kind={kind} bytes={} cards={}",
                self.update,
                update.bytes.len(),
                update.cards
            ),
        );
        Ok(())
    }

    /// The pointer position in device pixels: as reported under mode 1016,
    /// else the middle of the reported cell.
    fn pixel(&self, x: u32, y: u32) -> (u32, u32) {
        if self.pixels {
            (x, y)
        } else {
            let (cw, ch) = (u32::from(self.cell.0), u32::from(self.cell.1));
            (
                x.saturating_mul(cw).saturating_add(cw / 2),
                y.saturating_mul(ch).saturating_add(ch / 2),
            )
        }
    }

    /// Acts on one batch of events read together. Motion reports coalesce
    /// to the last, and scroll steps to their sum, until a click, a key or
    /// a new cell size needs the screen as it is. `false` to quit.
    fn handle(&mut self, events: &[Event]) -> Result<bool, String> {
        let mut moved = None;
        let mut scrolled = 0isize;
        for event in events {
            let step = match *event {
                Event::Mouse(m) => match m.kind {
                    MouseKind::Move => {
                        moved = Some((m.x, m.y));
                        self.pointer = Some(self.pixel(m.x, m.y));
                        continue;
                    }
                    MouseKind::ScrollDown => 1,
                    MouseKind::ScrollUp => -1,
                    _ => 0,
                },
                Event::Key(Key::Down) => 1,
                Event::Key(Key::Up) => -1,
                Event::Key(Key::PageDown) => 40,
                Event::Key(Key::PageUp) => -40,
                _ => 0,
            };
            if step != 0 {
                scrolled = scrolled.saturating_add(step);
                continue;
            }
            self.flush(moved.take(), std::mem::take(&mut scrolled))?;
            let done = match *event {
                Event::Mouse(m) if m.kind == MouseKind::Press(0) => {
                    let (x, y) = self.pixel(m.x, m.y);
                    self.pointer = Some((x, y));
                    self.click(x, y)?.map(|u| ("click", u))
                }
                Event::Key(Key::Char('s')) => self.sort()?.map(|u| ("sort", u)),
                Event::Key(Key::Char('d')) => self.drill()?.map(|u| ("drill", u)),
                Event::Key(Key::Char('u')) => self.undrill()?.map(|u| ("drill", u)),
                Event::Key(Key::Char('r')) => self.reset()?.map(|u| ("reset", u)),
                Event::Key(Key::Char('q') | Key::Interrupt | Key::EndOfInput) => return Ok(false),
                Event::CellSize { width, height } => {
                    let cell = cell_of(self.terminal, Some((width, height)), self.look.scale)
                        .unwrap_or(self.cell);
                    if cell != self.cell {
                        self.cell = cell;
                        self.resize()?;
                    }
                    None
                }
                _ => None,
            };
            if let Some((kind, u)) = done {
                self.send(kind, &u)?;
            }
        }
        self.flush(moved, scrolled)?;
        Ok(true)
    }

    /// Draws what coalesced: the scroll steps, then the last pointer place.
    fn flush(&mut self, moved: Option<(u32, u32)>, scrolled: isize) -> Result<(), String> {
        if scrolled != 0
            && let Some(u) = self.scroll(scrolled)?
        {
            self.send("scroll", &u)?;
        }
        if let Some((x, y)) = moved {
            let (x, y) = self.pixel(x, y);
            if let Some(u) = self.hover(x, y)? {
                self.send("hover", &u)?;
            }
        }
        Ok(())
    }

    fn resize(&mut self) -> Result<(), String> {
        self.begin("resize");
        self.lay_out()?;
        let u = self.draw_all("resize")?;
        self.send("resize", &u)
    }
}

/// The card a drill draws: `request`'s rows fetched through `fetch`, kept
/// only inside every period of `within`, and mended as the chart tool
/// mends a card. The rows outside are dropped as they arrive.
fn drilled(
    fetch: &Fetch<'_>,
    painter: &EnginePainter,
    request: &tiles_intent::Request,
    within: &[(String, String)],
) -> Result<Intent, String> {
    let source = request
        .omni()
        .ok_or("d drills only a card drawn from an Omni date field")?;
    let fetched = fetch(Query::Omni(source), Instant::now()).map_err(|e| format!("omni: {e}"))?;
    let mut intent = request
        .clone()
        .with_table(&fetched.table)
        .map(|i| state::inside(&i, within))
        .map_err(|e| e.to_string())?;
    if intent.table.rows.is_empty() {
        let label = within.last().map_or("", |w| w.1.as_str());
        return Err(format!("Omni has no rows inside {label}"));
    }
    intent.notes.extend(fetched.notes);
    painter.mend(&mut intent);
    Ok(intent)
}

/// A planned drill: the request, the periods, the card's title.
type Planned = (tiles_intent::Request, Vec<(String, String)>, String);

/// What a drill into row `row` of card `card` asks: the request with the
/// next finer timeframe, every period the card is inside (the new one
/// last, as a column and its label), and the card's title.
fn plan_drill(
    call: &Call,
    shown: &[Intent],
    state: &State,
    card: usize,
    row: usize,
) -> Result<Planned, String> {
    let levels = state.drills.get(&card).map_or(&[][..], Vec::as_slice);
    let from = levels
        .last()
        .map_or_else(|| call.requests.get(card), |d| Some(&d.request))
        .ok_or("no such card")?;
    let intent = shown.get(card).ok_or("no such card")?;
    let label = intent
        .column("x")
        .and_then(|c| intent.table.rows.get(row)?.get(c))
        .map(tiles_intent::Cell::label)
        .ok_or("no period under the pointer")?;
    let request = state::drill(from, &label)?;
    let mut within: Vec<(String, String)> = levels.iter().map(|d| d.within.clone()).collect();
    within.push((from.fields.get("x").cloned().unwrap_or_default(), label));
    Ok((request, within, intent.title.clone()))
}

/// One cell in device pixels, from the terminal's `CSI 16 t` reply or the
/// window size, read as start-up reads it.
fn cell_of(terminal: Terminal, reply: Option<(u16, u16)>, scale: f32) -> Option<(u16, u16)> {
    tiles_term::detect::cell_px(terminal, reply, screen::window_cell(), scale).map(|(c, _)| c)
}

/// The cells a height in CSS pixels covers, for cells `cell` CSS pixels
/// tall.
fn cell_rows(height: f32, cell: f32) -> u16 {
    let rows = (height / cell - 0.01).ceil();
    if rows.is_finite() && rows >= 1.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped.
        let r = rows.min(f32::from(u16::MAX)) as u16;
        r
    } else {
        1
    }
}

/// Runs `tiles pane` with the registered data `sources` and returns its
/// exit code.
pub fn main(args: &[String], sources: &[&dyn Source]) -> ExitCode {
    let started = Instant::now();
    match run(args, sources, started) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tiles pane: {}", one_line(&e));
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String], sources: &[&dyn Source], started: Instant) -> Result<(), String> {
    let args = parse_args(args)?;
    let (json, name) = read_call(&args.fixture)?;
    let painter = EnginePainter::new()?;
    let fetcher = crate::serve::Fetcher::new(sources);
    let fetch = |query: Query<'_>, at: Instant| fetcher.fetch(query, at);
    let content = content(&json, &painter, &fetch)?;
    let trace = Arc::new(Trace::open(args.trace.as_deref(), started)?);
    let signals = screen::Signals::register()?;
    let screen = screen::Screen::take()?;
    let probed = screen.probe(PROBE_WAIT);
    let look = tiles_mcp::look_from_env(&crate::serve::env);
    let size = screen::size();
    let env = |k: &str| std::env::var(k).ok();
    let terminal = tiles_term::detect::detect(&env, None).map_or(Terminal::Unknown, |d| d.terminal);
    let cell = cell_of(terminal, probed.cell, look.scale).unwrap_or((16, 32));
    let mut setup = pane::MOUSE.to_string();
    if probed.pixels {
        setup.push_str(pane::PIXEL_MOUSE);
    }
    screen::write(setup.as_bytes())?;
    trace.line(
        "start",
        format_args!(
            "terminal={} pixels={} cell={}x{} scale={}",
            terminal.name(),
            if probed.pixels { "yes" } else { "no" },
            cell.0,
            cell.1,
            look.scale
        ),
    );
    trace::sample(Arc::clone(&trace), SAMPLE_EVERY);
    let mut app = App {
        painter,
        fetch: &fetch,
        content,
        look,
        terminal,
        cell,
        size,
        pixels: probed.pixels,
        slots: Vec::new(),
        ids: Vec::new(),
        overlay: None,
        pointer: None,
        name,
        note: None,
        trace,
        update: 0,
    };
    app.begin("first");
    app.lay_out()?;
    let first = app.draw_all("first")?;
    app.send("first", &first)?;
    #[cfg(debug_assertions)]
    assert!(
        std::env::var_os("TILES_PANE_TEST_PANIC").is_none(),
        "TILES_PANE_TEST_PANIC: a panic after the first frame, to prove the screen comes back"
    );
    serve(&mut app, &signals, probed)?;
    drop(screen);
    Ok(())
}

/// Reads input and signals until the pane ends.
fn serve(app: &mut App, signals: &screen::Signals, probed: screen::Probed) -> Result<(), String> {
    let mut parser = Parser::new();
    let mut events = Vec::new();
    loop {
        let ready = signals.wait();
        if ready.quit {
            break;
        }
        if ready.resize {
            // A terminal that answered `CSI 16 t` answers again, and a new
            // cell size arrives as input; one that did not may report the
            // window in pixels.
            let now = screen::size();
            let cell = if probed.cell.is_some() {
                screen::write(pane::CELL_SIZE.as_bytes())?;
                app.cell
            } else {
                cell_of(app.terminal, None, app.look.scale).unwrap_or(app.cell)
            };
            if (now, cell) != (app.size, app.cell) {
                app.size = now;
                app.cell = cell;
                app.resize()?;
            }
        }
        if ready.input {
            let Some(bytes) = screen::read()? else { break };
            events.clear();
            parser.feed(&bytes, &mut events);
            let reports = events
                .iter()
                .filter(|e| matches!(e, Event::Mouse(_)))
                .count();
            app.trace.line(
                "read",
                format_args!("bytes={} reports={reports}", bytes.len()),
            );
            if !app.handle(&events)? {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Args, USAGE, cell_rows, parse_args};

    fn usage() -> String {
        format!("usage: {USAGE}")
    }

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_string()).collect()
    }

    #[test]
    fn the_fixture_and_a_trace_file_are_read() {
        assert_eq!(
            parse_args(&args(&["d.json", "--trace", "t.txt"])),
            Ok(Args {
                fixture: "d.json".into(),
                trace: Some("t.txt".into())
            })
        );
        assert_eq!(parse_args(&args(&[])), Err(usage()));
        assert_eq!(parse_args(&args(&["a", "b"])), Err(usage()));
        assert!(parse_args(&args(&["a", "--trace"])).is_err());
        assert!(parse_args(&args(&["a", "--frames"])).is_err());
        assert!(
            parse_args(&args(&["-"]))
                .unwrap_err()
                .contains("standard input"),
            "standard input is the keyboard"
        );
    }

    #[test]
    fn a_height_covers_whole_cells() {
        assert_eq!(cell_rows(32.0, 16.0), 2);
        assert_eq!(cell_rows(33.0, 16.0), 3);
        assert_eq!(cell_rows(f32::NAN, 16.0), 1);
    }

    #[test]
    fn the_usage_text_names_the_pane() {
        assert!(crate::USAGE.contains(USAGE));
    }

    /// A column card of Lumen Coffee's orders by month, from Omni.
    const BY_MONTH: &str = r#"{"v":1,"form":"column","title":"July was the busiest month",
        "claim":{"kind":"rank","tone":"good"},
        "fields":{"x":"orders.created_at[month]","y":"orders.count"},"highlight":["2026-07-01"],
        "data":{"omni":{"topic":"orders","fields":["orders.created_at[month]","orders.count"],
        "sorts":[{"field":"orders.created_at[month]","desc":false}],"limit":6}}}"#;

    // ADR-065: a drill goes through the same source seam as the card, to
    // the replay server, which answers only the recorded requests.
    #[test]
    fn a_drill_asks_omni_for_the_next_timeframe_and_keeps_only_its_month() {
        use super::state::{Drilled, State};
        use super::{drilled, plan_drill};
        use tiles_mcp::card::{Call, EnginePainter, Fetched, Query};
        use tiles_omni::replay::{ReplayServer, recorded};

        let server = ReplayServer::start("replay-key", recorded());
        let client = tiles_omni::OmniClient::new(server.config());
        let fetch = |q: Query<'_>, _: std::time::Instant| match q {
            Query::Omni(s) => client
                .query(s)
                .map(Fetched::omni)
                .map_err(|e| e.to_string()),
            Query::Warehouse(_) => Err("no warehouse".to_string()),
        };
        let painter = EnginePainter::new().unwrap();
        let call = Call::read(BY_MONTH, &painter, &fetch).unwrap();
        assert_eq!(call.origins, [Some("Omni".to_string())]);
        let months = &call.intents[0];
        let x = months.column("x").unwrap();
        let august = months
            .table
            .rows
            .iter()
            .position(|r| r[x].label() == "2026-08-01")
            .unwrap();

        let state = State::default();
        let (request, within, title) = plan_drill(&call, &call.intents, &state, 0, august).unwrap();
        assert_eq!(title, "July was the busiest month");
        assert_eq!(
            within,
            [(
                "orders.created_at[month]".to_string(),
                "2026-08-01".to_string()
            )]
        );
        let weeks = drilled(&fetch, &painter, &request, &within).unwrap();
        assert!(server.unrecorded().is_empty(), "{:?}", server.unrecorded());
        let w = weeks.column("x").unwrap();
        let labels: Vec<String> = weeks.table.rows.iter().map(|r| r[w].label()).collect();
        assert_eq!(
            labels,
            [
                "2026-07-27",
                "2026-08-03",
                "2026-08-10",
                "2026-08-17",
                "2026-08-24"
            ],
            "August's weeks only, the one July shares included"
        );
        assert_eq!(weeks.subtitle.as_deref(), Some("2026-08-01 by week"));
        assert!(
            weeks.highlight.is_empty(),
            "July is not among August's weeks"
        );

        // The drilled card is what the state draws, and a second drill
        // starts from its request.
        let mut state = state;
        state.drills.entry(0).or_default().push(Drilled {
            request,
            within: within[0].clone(),
            intent: weeks.clone(),
        });
        assert_eq!(state.apply(&call.intents), std::slice::from_ref(&weeks));
        let (deeper, within, _) = plan_drill(&call, &[weeks], &state, 0, 1).unwrap();
        assert_eq!(deeper.fields["x"], "orders.created_at[date]");
        assert_eq!(within.len(), 2);
        assert_eq!(within[1].1, "2026-08-03");
    }

    #[test]
    fn a_card_with_rows_of_its_own_does_not_drill() {
        use super::state::State;
        use super::{drilled, plan_drill};
        use tiles_mcp::card::{Call, EnginePainter};

        let painter = EnginePainter::new().unwrap();
        let refuse = |_: tiles_mcp::card::Query<'_>, _: std::time::Instant| -> Result<_, String> {
            panic!("an inline card reaches no source")
        };
        let json = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/pane/dashboard.json"
        ))
        .unwrap();
        let call = Call::read(&json, &painter, &refuse).unwrap();
        assert!(call.origins.iter().all(Option::is_none));
        let err = plan_drill(&call, &call.intents, &State::default(), 0, 0).unwrap_err();
        assert!(err.contains("Omni date field"), "{err}");
        assert!(drilled(&refuse, &painter, &call.requests[0], &[]).is_err());
    }
}
