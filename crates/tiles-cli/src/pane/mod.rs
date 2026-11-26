//! `tiles pane <fixture>`: the interactive dashboard pane prototype
//! (ADR-063), built only with debug assertions on (a debug build, or
//! `cargo build --profile pane`), left out of the usage text, and reached
//! from no MCP tool, add-on or `tiles run`.
//!
//! It takes the whole terminal in the alternate screen and raw mode, lays
//! the fixture's cards out with the dashboard planner at the pane's width
//! and sends each card as its own kitty picture. Hovering a mark sends one
//! small picture over its card, the mark's outline and its tooltip; a click
//! filters the other cards' rows in memory and sends only the cards that
//! changed; a resize lays everything out again. A fixture holding
//! `"stress": "table"` draws a grid of text that the wheel scrolls, to price
//! a table before a table form exists.
//!
//! Every byte to the terminal comes from `tiles_term::pane` and
//! `tiles_term::kitty`; input goes through `tiles_term::input`. The screen
//! is given back on every way out: `q`, Control-C or Control-D, SIGINT,
//! SIGTERM, SIGHUP or SIGQUIT, the end of input, an error, and a panic.

mod board;
mod screen;
mod trace;

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tiles_engine::{Moment, Scene};
use tiles_intent::{Intent, one_line};
use tiles_mcp::card::{Call, EnginePainter, Look, Tile};
use tiles_term::detect::Terminal;
use tiles_term::input::{Event, Key, MouseKind, Parser};
use tiles_term::{kitty, pane};

use board::{Selection, Slot};
use trace::Trace;

/// The command's usage, printed on a wrong argument only.
pub const USAGE: &str = "usage: tiles pane <fixture.json> [--trace <file>]";

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
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown flag {}; {USAGE}", one_line(flag)));
            }
            path if fixture.is_none() => fixture = Some(path.to_string()),
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(Args {
        fixture: fixture.ok_or(USAGE)?,
        trace,
    })
}

/// What the pane shows.
enum Content {
    /// A chart call's cards, as the call asks, with the rows a click left.
    Dashboard {
        call: Call,
        intents: Vec<Intent>,
        selection: Option<Selection>,
    },
    /// The text grid, scrolled `offset` rows down its rows.
    Stress {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        offset: usize,
    },
}

/// Reads a fixture: the stress grid when it holds `"stress": "table"`,
/// else a chart call.
fn content(json: &str, painter: &EnginePainter) -> Result<Content, String> {
    let value: Option<serde_json::Value> = serde_json::from_str(json).ok();
    if let Some(v) = value.as_ref().filter(|v| v.get("stress").is_some()) {
        return stress(v);
    }
    let call = Call::read(json, painter).map_err(|e| e.to_string())?;
    Ok(Content::Dashboard {
        intents: call.intents.clone(),
        call,
        selection: None,
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
struct App {
    painter: EnginePainter,
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

impl App {
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
            Content::Dashboard { call, intents, .. } => {
                let (laid, order) = call
                    .lay_out(intents, &self.painter, &look)
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
            Content::Dashboard {
                selection: Some(s), ..
            } => {
                let _ = write!(
                    text,
                    " · {} = {} (click it again or empty space to clear)",
                    s.column, s.value
                );
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
            Content::Dashboard { .. } => text.push_str(" · hover a mark, click to filter"),
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

    fn drop_overlay(&mut self, bytes: &mut Vec<u8>) {
        if let Some((_, id)) = self.overlay.take() {
            bytes.extend(kitty::delete(id));
        }
    }

    /// The pointer at device pixel `x`, `y`: the overlay for the mark under
    /// it, or none; `None` when that is what the screen already shows.
    fn hover(&mut self, x: u32, y: u32) -> Result<Option<Update>, String> {
        let target = board::slot_at(&self.slots, self.cell, x, y).and_then(|(s, (dx, dy))| {
            board::hit_index(&self.slots[s].hits, dx, dy).map(|h| (s, h))
        });
        if target == self.overlay.map(|(t, _)| t) {
            return Ok(None);
        }
        self.begin("hover");
        let mut bytes = pane::BEGIN_UPDATE.as_bytes().to_vec();
        let old = self.overlay.map(|(_, id)| id);
        self.overlay = None;
        if let Some((s, h)) = target {
            let id = if old == Some(OVERLAY_IDS[0]) {
                OVERLAY_IDS[1]
            } else {
                OVERLAY_IDS[0]
            };
            if let Some(placed) = self.overlay_bytes(s, h, id)? {
                bytes.extend(placed);
                self.overlay = Some(((s, h), id));
            }
        }
        if let Some(old) = old {
            bytes.extend(kitty::delete(old));
        }
        bytes.extend(pane::END_UPDATE.as_bytes());
        Ok(Some(Update { bytes, cards: 0 }))
    }

    fn overlay_bytes(&self, s: usize, h: usize, id: u32) -> Result<Option<Vec<u8>>, String> {
        let slot = &self.slots[s];
        let scale = self.look.scale;
        let overlay = self.engine().hover(&slot.scene, &slot.hits[h], scale);
        let frame = self
            .engine()
            .frame_at(&overlay.scene, scale, Moment::FINAL)
            .map_err(|e| e.to_string())?;
        self.trace.line(
            "draw",
            format_args!("update={} kind=hover card=overlay", self.update),
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

    /// A left click at `x`, `y`: select the mark's value and filter the
    /// other cards, or clear the selection, sending only the cards whose
    /// scene changed.
    fn click(&mut self, x: u32, y: u32) -> Result<Option<Update>, String> {
        let Content::Dashboard {
            call,
            intents: shown,
            selection,
        } = &self.content
        else {
            return Ok(None);
        };
        let target = board::slot_at(&self.slots, self.cell, x, y).and_then(|(s, (dx, dy))| {
            let slot = &self.slots[s];
            let hit = board::hit_index(&slot.hits, dx, dy)?;
            Some((slot.intent?, slot.hits[hit].row?))
        });
        let picked = target.and_then(|(card, row)| board::pick(shown, card, row));
        let next = if picked == *selection { None } else { picked };
        if next.is_none() && selection.is_none() {
            return Ok(None);
        }
        let wanted = board::apply(&call.intents, next.as_ref());
        self.begin("click");
        let mut bytes = pane::BEGIN_UPDATE.as_bytes().to_vec();
        self.drop_overlay(&mut bytes);
        let (intents, cards) = self.relay(wanted, &mut bytes)?;
        if let Content::Dashboard {
            intents: shown,
            selection,
            ..
        } = &mut self.content
        {
            *shown = intents;
            *selection = next;
        }
        bytes.extend(self.status_bytes());
        bytes.extend(pane::END_UPDATE.as_bytes());
        Ok(Some(Update { bytes, cards }))
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
        let Content::Dashboard { intents: shown, .. } = &self.content else {
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
                "{} kept its rows: they cannot be drawn filtered",
                refused.join(", ")
            )
        });
        let cards = changed.len();
        for (i, scene) in changed {
            self.slots[i].hits = tiles_engine::hit::hit_map(&scene, look.scale);
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
            match *event {
                Event::Mouse(m) if m.kind == MouseKind::Press(0) => {
                    let (x, y) = self.pixel(m.x, m.y);
                    if let Some(u) = self.click(x, y)? {
                        self.send("click", &u)?;
                    }
                }
                Event::Key(Key::Char('q') | Key::Interrupt | Key::EndOfInput) => return Ok(false),
                Event::CellSize { width, height } => {
                    let cell = cell_of(self.terminal, Some((width, height)), self.look.scale)
                        .unwrap_or(self.cell);
                    if cell != self.cell {
                        self.cell = cell;
                        self.resize()?;
                    }
                }
                _ => {}
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

/// Runs `tiles pane` and returns its exit code.
pub fn main(args: &[String]) -> ExitCode {
    let started = Instant::now();
    match run(args, started) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tiles pane: {}", one_line(&e));
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String], started: Instant) -> Result<(), String> {
    let args = parse_args(args)?;
    let json = crate::read_input(&args.fixture)?;
    let painter = EnginePainter::new()?;
    let content = content(&json, &painter)?;
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
    let name = std::path::Path::new(&args.fixture).file_name().map_or_else(
        || args.fixture.clone(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut app = App {
        painter,
        content,
        look,
        terminal,
        cell,
        size,
        pixels: probed.pixels,
        slots: Vec::new(),
        ids: Vec::new(),
        overlay: None,
        name,
        note: None,
        trace,
        update: 0,
    };
    app.begin("first");
    app.lay_out()?;
    let first = app.draw_all("first")?;
    app.send("first", &first)?;
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
        assert_eq!(parse_args(&args(&[])), Err(USAGE.into()));
        assert_eq!(parse_args(&args(&["a", "b"])), Err(USAGE.into()));
        assert!(parse_args(&args(&["a", "--trace"])).is_err());
        assert!(parse_args(&args(&["a", "--frames"])).is_err());
    }

    #[test]
    fn a_height_covers_whole_cells() {
        assert_eq!(cell_rows(32.0, 16.0), 2);
        assert_eq!(cell_rows(33.0, 16.0), 3);
        assert_eq!(cell_rows(f32::NAN, 16.0), 1);
    }

    #[test]
    fn the_usage_text_leaves_the_pane_out() {
        assert!(!crate::USAGE.contains("pane"));
    }
}
