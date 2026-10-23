//! The wrapper's paint step: the harness's bytes in, the bytes for the real
//! terminal out. Pure, so every route is tested byte for byte.
//!
//! A [`vt100`] screen follows everything the harness draws. After each chunk
//! that ends at ground state, the painter finds the placeholder blocks on
//! screen ([`tiles_term::block`]) and draws each card over its rows by the
//! route [`tiles_term::detect`] chose, then puts the harness's cursor and
//! drawing attributes back. The prototype's rules hold:
//!
//! - nothing is painted inside an escape sequence or a UTF-8 character; a
//!   read that ends inside one is painted at its last ground point;
//! - a card counts as whole only once every row of its block has been on
//!   screen, across paints, so a block taller than a harness's own
//!   scrolling viewport (`OpenCode` scrolls under its tab bar, never
//!   reaching screen row 0) still draws its visible part; one copy per
//!   card is drawn (folded tool results and previews show partial copies);
//! - on the sixel route a card shows only once it is whole;
//! - pre-cut: the whole card goes up on the block's first row, over the
//!   rows the block will take, so it never shows a row at a time; the rows
//!   typed under it place nothing, and the rows not typed yet are blanked
//!   and given back once the card leaves them (ADR-044). A block with no
//!   room on screen for all its rows places nothing until it has, or is
//!   whole; a block that stops being typed is cut to its typed rows and
//!   gives the rows under them back. A card still being typed that moved is
//!   placed again as one strip at most once a tick, its band encoded once
//!   per set of rows and sent again as it is when only its place changed;
//! - in kitty ([`Painter::with_motion`]) the whole card shows as soon as the
//!   block's first row does: harnesses type a block a row at a time
//!   (`OpenCode` took about 1.3 s for 23 rows), so the card is placed over
//!   the rows the block will take, on an opaque backing in the
//!   conversation's colour, until the block is whole and its entrance has
//!   played;
//! - in kitty a card drawn again under its id (`tiles theme`, ADR-021) is
//!   sent again under its image id with its virtual placement, so every
//!   placeholder copy on screen and in scrollback shows the new picture with
//!   no new placement;
//! - in kitty the entrance is one kitty animation, sent ahead as soon as the
//!   card is in the cache: the settled card is its first frame (what shows
//!   if the animation never runs), the entrance frames from the first
//!   visible one follow (frame 0, the card at no opacity, is never sent),
//!   then the settled card again; from the card's first showing the wrapper
//!   shows the frame for the time passed, `floor(elapsed / frame_ms)` with
//!   the clock started a frame early so the first is visible, skipping any
//!   a late tick missed, and stops on the last frame once the entrance's
//!   time is up (ADR-030);
//! - pre-cut: the entrance's frames from the first visible one go to the
//!   terminal ahead as images of their own; from the card's first showing
//!   each paint places the frame for the time passed over the one before,
//!   then the crisp card takes over. A card placed again, moved or cut,
//!   shows its current frame or the settled card, never the entrance from
//!   the start; a moved card whose current frame is too large to send
//!   again in a tick shows the next frame sent ahead, ahead of its turn.
//!   A block that waited for room starts its entrance only once its card
//!   stops moving: when it is whole, when its typing stalls, or a tick
//!   after a row is typed with the block in place (ADR-053);
//! - token text that is not drawn over, or is half typed, is blanked, and
//!   so is a code fence line (```` ```tiles ````, ```` ``` ````) on the row
//!   right above a block's first row or right below its last; any other
//!   text stays;
//! - blanks and card cells sit on the background of the cell beside the
//!   block, else on the conversation's, remembered from the last screen
//!   mostly painted in a colour of its own, not the terminal's default:
//!   `OpenCode` paints its conversation near-black but leaves a fenced
//!   block on the default, so default cells showed each card in a lighter
//!   box, and a block typed from column 0 flashed;
//! - the harness's bytes written outside an update of its own go in the
//!   paint's synchronized update, so a typed token never shows before its
//!   blank;
//! - pre-cut: a partly visible card is sent as a new picture of its visible
//!   rows, and a moved card is placed under a new image id, above the old
//!   one, since Warp ignores crop rectangles and keeps old copies; a picture
//!   replaced is deleted a tick later, never in the update that places its
//!   replacement, since Warp then showed no card until it had decoded the
//!   new one; a whole card in several strips goes up again as one; each
//!   session numbers its pictures from an id of its own, since Warp keeps
//!   an earlier session's pictures by id in the same tab;
//! - while the harness holds a synchronized update, the paint waits for the
//!   update's end and goes in just before it, so it never reads a frame the
//!   harness has half drawn; otherwise it is wrapped in synchronized output;
//! - a panic inside the screen model (`vt100` has some on narrowing resizes
//!   and one-row screens) never reaches the harness: the painter stops
//!   drawing and passes every later byte through unchanged, adding only the
//!   deletes for pictures already placed once the stream is at ground.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::Once;
use std::time::Duration;

use tiles_term::block::{self, Token};
use tiles_term::detect::Route;
use tiles_term::image::Rgba;
use tiles_term::{kitty, sixel};

use super::escape::Tracker;

/// A card the painter can draw.
#[derive(Debug, Clone)]
pub struct CardInfo {
    /// Columns the card covers.
    pub cols: u16,
    /// Rows the card covers.
    pub rows: u16,
    /// The whole card as PNG.
    pub png: Rc<Vec<u8>>,
    /// The PNG's size in pixels: width, height.
    pub size_px: (u32, u32),
    /// Entrance frames in the cache beside it ([`Pictures::frames`]).
    pub frames: u16,
    /// Milliseconds from one entrance frame to the next.
    pub frame_ms: u16,
}

/// Where the painter gets cards.
pub trait Pictures {
    /// Card `id`, if it was made on this machine.
    fn card(&mut self, id: &str) -> Option<CardInfo>;
    /// Rows `first` to `first + count` (card rows, from 0) of card `id` as
    /// pixels.
    fn band(&mut self, id: &str, first: u16, count: u16) -> Option<Rgba>;
    /// [`Pictures::band`] as PNG.
    fn band_png(&mut self, id: &str, first: u16, count: u16) -> Option<Vec<u8>>;
    /// Card `id`'s entrance frames as PNG, in order; empty when any is
    /// missing.
    fn frames(&mut self, id: &str) -> Vec<Vec<u8>>;
    /// Rows `first` to `first + count` (card rows, from 0) of card `id`'s
    /// entrance frame `n` as PNG: the frame's own file when that is all of
    /// it.
    fn frame_band_png(&mut self, id: &str, n: u16, first: u16, count: u16) -> Option<Vec<u8>>;
    /// A flat picture `w` by `h` pixels in one colour, as PNG.
    fn flat_png(&mut self, w: u32, h: u32, rgb: (u8, u8, u8)) -> Option<Vec<u8>>;
    /// Cards that arrived in the cache since the last call, so their
    /// pictures go to the terminal before their blocks are typed.
    fn fresh(&mut self) -> Vec<String>;
    /// Cards [`Pictures::fresh`] listed before whose picture has been drawn
    /// again under the same id since, such as in a new theme.
    fn redrawn(&mut self) -> Vec<String>;
}

/// Where a card's entrance stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Play {
    /// Sent, not shown yet.
    Ready,
    /// Its first visible frame shown; its clock waits for the harness to
    /// end its synchronized update, since kitty shows nothing new until
    /// then and the frames of that time would never be seen.
    Waiting(Duration),
    /// Running since then.
    Playing(Duration),
    /// Stopped on the settled card.
    Done,
}

/// A card's entrance, as kitty holds it: an image of its own beside the
/// settled card, drawn at half the card's resolution so it reaches the
/// terminal fast, and deleted once played.
#[derive(Debug, Clone, Copy)]
struct Entrance {
    /// Its image id.
    image: u32,
    /// Its pixel size.
    size_px: (u32, u32),
    /// Kitty's frame count: the settled card, the entrance from its first
    /// visible frame, the settled card.
    last: u32,
    /// Milliseconds from one entrance frame to the next.
    frame_ms: u32,
    /// The frame kitty shows, from 1.
    shown: u32,
    state: Play,
}

impl Entrance {
    /// How long the run takes, from the entrance's first frame, the card at
    /// no opacity, to the last, the settled card.
    fn length(&self) -> Duration {
        Duration::from_millis(u64::from(self.last.saturating_sub(1)) * u64::from(self.frame_ms))
    }

    /// The frame to show `elapsed` into the run: entrance frame
    /// `floor(elapsed / frame_ms)`, which is kitty's frame one higher, the
    /// settled card being kitty's first and entrance frame 0 never sent, up
    /// to the last.
    fn frame_at(&self, elapsed: Duration) -> u32 {
        let step = elapsed.as_millis() / u128::from(self.frame_ms.max(1));
        u32::try_from(step)
            .unwrap_or(u32::MAX)
            .saturating_add(1)
            .min(self.last)
    }
}

/// Where a card placed early sits, and what of it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spot {
    /// First screen row.
    row: u16,
    /// First column.
    col: u16,
    /// Rows covered.
    rows: u16,
    /// Top of the part shown, in the picture's pixels.
    crop_y: u32,
    /// Height of the part shown, in the picture's pixels.
    crop_h: u32,
    /// The backing's colour; no backing when unknown.
    color: Option<(u8, u8, u8)>,
}

/// A card placed early, over the rows its block will take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Early {
    at: Spot,
    /// The backing's image id.
    mask: u32,
    /// The image placed: the entrance's, or the settled card's when the
    /// entrance had not reached the terminal yet.
    image: u32,
}

/// A copy of a block on screen: each member's block row and screen row,
/// keyed by card id, first screen row and column.
type Copies = HashMap<(String, i32, usize), Vec<(u16, u16)>>;

/// Pre-cut: a band of card rows, first and last, with the PNG it encoded to.
type Band = ((u16, u16), Rc<Vec<u8>>);

/// Placement id of a card placed early; the backing is placement 1.
const EARLY_PLACEMENT: u32 = 2;
/// Longest the run waits for the harness's synchronized update to end.
const SYNC_WAIT: Duration = Duration::from_secs(1);
/// Most bytes of entrance frames written to the terminal per tick: kitty
/// took in about a megabyte a second on the 2017 Mac, and a larger write
/// held the harness's own output back while it drained.
const SEND_BUDGET: usize = 64 * 1024;
/// How often the painter's clock moves on without output from the harness:
/// an entrance's frame for the time passed goes up this often while it
/// plays, and a pre-cut card still being typed that moved is placed again
/// at most this often.
pub(super) const TICK: Duration = Duration::from_millis(50);
/// Pre-cut: how long a block still being typed may go without a new row
/// before its card stops reaching past its typed rows: a reply the user
/// interrupted, or a stream the network dropped, leaves a block that never
/// grows again under the harness's next lines.
const STALL: Duration = Duration::from_millis(1500);

/// When an entrance first shown at `now` began, so that its first frame on
/// screen is its second, the first visible one: the frame at 0 ms is the
/// card at no opacity, and showing it left a tick with nothing where the
/// block had been blanked.
fn one_frame_in(now: Duration, frame_ms: u32) -> Duration {
    now.saturating_sub(Duration::from_millis(u64::from(frame_ms)))
}

/// One block copy on screen: card rows from `lo` to `hi`, the first at screen
/// row `y`, typed down to card row `typed`. On the pre-cut route a block
/// still being typed reaches past its typed rows to the rows it will take
/// on screen; everywhere else `typed` is `hi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Copy {
    y: u16,
    col: u16,
    lo: u16,
    hi: u16,
    typed: u16,
}

impl Copy {
    /// The screen rows of its card rows `lo` to `typed`, the ones its
    /// block has typed.
    fn typed_rows(&self) -> std::ops::Range<u16> {
        self.y..self.y + (self.typed.min(self.hi).saturating_sub(self.lo) + 1)
    }
}

/// Pre-cut: how far a block still being typed has got ([`Painter::growing`]).
#[derive(Debug, Clone, Copy)]
struct Typing {
    /// Its last block row typed.
    typed: u16,
    /// When that row was first seen.
    since: Duration,
    /// Whether its card has stopped reaching past its typed rows.
    stopped: bool,
}

/// Pre-cut: a block that waited for room on screen ([`Painter::growing`]),
/// as last seen.
#[derive(Debug, Clone, Copy)]
struct Wait {
    /// The screen row of its card's first row.
    top: i32,
    /// Its last block row typed.
    typed: u16,
    /// Since when a row typed has left it in place; `None` while it moves.
    still: Option<Duration>,
    /// Whether it has room and is held until its card stops moving.
    held: bool,
}

/// A picture on the pre-cut route: card rows `lo` to `hi`, the first at
/// screen row `y`, `width` columns from `col`, under an image id of its
/// own. A card on screen is one strip, or several stacked in order down
/// the card while its block is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Strip {
    y: u16,
    col: u16,
    lo: u16,
    hi: u16,
    width: u16,
    image: u32,
    /// The entrance frame it shows; `None` for the crisp card.
    frame: Option<u16>,
}

/// Pre-cut: a card's entrance, each frame an image of its own, sent ahead.
#[derive(Debug, Clone)]
struct Reel {
    /// Each entrance frame's image id from frame 1, the first visible one,
    /// in order; frame 0, the card at no opacity, is never sent, and the
    /// crisp card follows the last.
    images: Vec<u32>,
    /// Frames in the terminal, from the first.
    sent: usize,
    /// Frames placed once: none is placed twice, since Warp keeps the old
    /// copy of a picture placed again.
    used: Vec<bool>,
    /// The furthest frame placed, from 1: a moved card can show a frame
    /// ahead of its turn ([`Painter::frame_picture`]), and the entrance
    /// never steps back from it.
    reached: u16,
    frame_ms: u32,
    /// When the card first showed, which the frame on screen is timed from.
    since: Option<Duration>,
}

impl Reel {
    /// The entrance frame `elapsed` after the card first showed, never
    /// before the furthest placed, `None` once the entrance is over.
    fn frame_at(&self, elapsed: Duration) -> Option<u16> {
        let step = elapsed.as_millis() / u128::from(self.frame_ms.max(1));
        usize::try_from(step)
            .ok()
            .filter(|i| (1..=self.images.len()).contains(i))
            .and_then(|i| u16::try_from(i).ok())
            .map(|f| f.max(self.reached))
    }

    /// Whether the entrance is over at `now`.
    fn over(&self, now: Duration) -> bool {
        self.since
            .is_some_and(|since| self.frame_at(now.saturating_sub(since)).is_none())
    }
}

impl Strip {
    /// The screen row of the card's first row, above the screen when
    /// negative.
    fn top(&self) -> i32 {
        i32::from(self.y) - i32::from(self.lo) + 1
    }
}

/// One screen read: each row's characters and signature, the tokens on it,
/// and where a half-typed token starts.
struct Scan {
    cells: Vec<Vec<char>>,
    sigs: Vec<u64>,
    found: Vec<(u16, Token)>,
    partials: Vec<(u16, usize)>,
}

/// The paint state.
#[allow(clippy::struct_excessive_bools)] // Independent switches and facts, not a state machine.
pub struct Painter<P: Pictures> {
    parser: vt100::Parser,
    tracker: Tracker,
    route: Route,
    pictures: P,
    whole: HashSet<String>,
    /// The block rows of each card not yet whole that some copy has shown.
    seen: HashMap<String, HashSet<u16>>,
    placed: HashMap<String, (Copy, u32)>,
    uploaded: HashSet<String>,
    painted: HashMap<u16, u64>,
    /// The cells the last paint blanked: row, first column, length.
    blanked: Vec<(u16, u16, u16)>,
    next_image: u32,
    alternate: bool,
    broken: bool,
    pending: Vec<u8>,
    motion: bool,
    default_bg: Option<(u8, u8, u8)>,
    now: Duration,
    entrances: HashMap<String, Entrance>,
    early: HashMap<String, Early>,
    preload: Vec<String>,
    redrawn: Vec<String>,
    /// What happened when, under `TILES_TRACE`; `None` when off.
    trace: Option<Vec<String>>,
    /// The harness's side panel ([`crate::harness::Harness::side_panel`]).
    panel: Option<(u16, u16)>,
    /// Entrance frames waiting to go to the terminal, a kitty command each.
    outbox: std::collections::VecDeque<(String, Vec<u8>)>,
    /// Entrances whose frames are still in the outbox.
    sending: HashMap<String, Entrance>,
    /// Cards shown before their entrance reached the terminal: they stay
    /// settled, and their entrance is dropped.
    shown: HashSet<String>,
    /// Since when the harness has held its synchronized update, while the
    /// paint waits for it.
    sync_since: Option<Duration>,
    /// The conversation's background, from the last screen mostly painted
    /// in a colour of its own ([`screen_background`]): a blank with no
    /// painted cell beside it takes it, never the terminal's default.
    conversation: Option<vt100::Color>,
    /// Pre-cut: each card's pictures on screen, a band of rows each, in
    /// order down the card.
    strips: HashMap<String, Vec<Strip>>,
    /// Pre-cut: plays entrances ([`Painter::with_motion`]).
    precut: bool,
    /// Pre-cut: sends each entrance's frames ahead; otherwise each frame
    /// is sent as it is placed ([`Painter::with_frames_each_tick`]).
    ahead: bool,
    /// Pre-cut: each card's entrance.
    reels: HashMap<String, Reel>,
    /// Pre-cut: the last band of rows encoded for each card, keyed by its
    /// first and last row, sent again as it is when the card only moved.
    bands: HashMap<String, Band>,
    /// Pre-cut: the tick in which each card still being typed was last
    /// placed again for a move; its next move waits for the next tick.
    replaced: HashMap<String, u128>,
    /// Pre-cut: a moved card waits for the tick to be placed again.
    deferred: bool,
    /// Pre-cut: bytes of cards placed again for a move since the last
    /// [`Painter::flush`], which they share [`SEND_BUDGET`] with.
    spent: usize,
    /// Pre-cut: pictures replaced on screen, each deleted once its time
    /// comes, a tick after its replacement went up: Warp shows nothing in
    /// place of a picture deleted in the update that places the new one
    /// until it has decoded the new one, 33 to 100 ms in a recording.
    doomed: Vec<(u32, Duration)>,
    /// Pre-cut: the stacking order of the next picture placed, so each
    /// draws over the one it replaces.
    next_z: i32,
    /// Pre-cut: how far each block still being typed has got, and each
    /// stalled block not yet whole, kept across a resize or a switch of
    /// screen so only a new row restarts its reach.
    typing: HashMap<String, Typing>,
    /// Pre-cut: each block not yet placed that waited for room, held back
    /// until its card stops moving, so its entrance plays in one place.
    waits: HashMap<String, Wait>,
    /// Pre-cut: cards whose pictures were on screen when the terminal
    /// changed size. They stay until the next paint places each again over
    /// them, wherever its block now is, since the terminal may have moved
    /// a picture with its text.
    resized: HashSet<String>,
    /// Where the painter's own image ids wrap back to.
    first_image: u32,
}

/// First image id the painter's own pictures use (pre-cut and sixel
/// pictures, entrances, backings); ids count up from here over even ids
/// only. Every [`card_image_id`] has the low bit of each byte set, so it is
/// odd and never one of them.
const FIRST_IMAGE: u32 = 0x50_0000;
/// Last image id before the painter's own ids wrap.
const LAST_IMAGE: u32 = 0x7f_fffe;
/// Pre-cut: first image id a session's own ids may take, above every
/// [`card_image_id`] (24 bits).
const SESSION_IMAGES: u32 = 0x0100_0000;
/// Pre-cut: last image id a session's own ids may take.
const LAST_SESSION_IMAGE: u32 = 0xffff_fffe;

/// Pre-cut: the first image id of session `session`. Warp keeps a picture
/// by its id across sessions in one tab, and sending an id again repaints
/// every placement of it, an earlier session's card in scrollback included;
/// so each session starts at a point of its own, drawn from its token, in
/// a range of two billion ids it uses a few thousand of.
fn session_first_image(session: u64) -> u32 {
    // splitmix64's finaliser: neighbouring tokens land far apart.
    let mut z = session.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    let even = u64::from((LAST_SESSION_IMAGE - SESSION_IMAGES) / 2 + 1);
    let step = u32::try_from(z % even).unwrap_or(0);
    SESSION_IMAGES + 2 * step
}

fn cup(out: &mut Vec<u8>, y: u16, x: u16) {
    let _ = write!(Sink(out), "\x1b[{};{}H", y + 1, x + 1);
}

struct Sink<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for Sink<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

thread_local! {
    static QUIET: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f`, returning `None` if it panics. The panic message is not printed:
/// the terminal is in raw mode under the harness, and the wrapper carries on.
fn contained<R>(f: impl FnOnce() -> R) -> Option<R> {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.get() {
                previous(info);
            }
        }));
    });
    QUIET.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.set(false);
    result.ok()
}

/// The SGR that resets attributes and sets background `color`; `\x1b[0m`
/// for the default.
fn sgr_background(color: vt100::Color) -> Vec<u8> {
    match color {
        vt100::Color::Default => b"\x1b[0m".to_vec(),
        vt100::Color::Idx(i) => format!("\x1b[0;48;5;{i}m").into_bytes(),
        vt100::Color::Rgb(red, green, blue) => {
            format!("\x1b[0;48;2;{red};{green};{blue}m").into_bytes()
        }
    }
}

/// The screen's own background: the colour most of its cells are on, when
/// that is not the terminal's default. `OpenCode` paints its whole screen
/// near-black; Claude Code leaves it on the default.
fn screen_background(screen: &vt100::Screen) -> Option<vt100::Color> {
    let (rows, cols) = screen.size();
    let mut counts: Vec<(vt100::Color, usize)> = Vec::new();
    for y in 0..rows {
        for x in 0..cols {
            let Some(bg) = screen.cell(y, x).map(vt100::Cell::bgcolor) else {
                continue;
            };
            match counts.iter_mut().find(|(c, _)| *c == bg) {
                Some((_, n)) => *n += 1,
                None => counts.push((bg, 1)),
            }
        }
    }
    let default = counts
        .iter()
        .find(|(c, _)| *c == vt100::Color::Default)
        .map_or(0, |(_, n)| *n);
    counts
        .into_iter()
        .filter(|(c, n)| *c != vt100::Color::Default && *n > default)
        .max_by_key(|(_, n)| *n)
        .map(|(c, _)| c)
}

/// The background of the cell just left of `col`, else just right of
/// `col + width`.
fn beside(screen: &vt100::Screen, row: u16, col: u16, width: usize) -> vt100::Color {
    let cell = if col > 0 {
        screen.cell(row, col - 1)
    } else {
        u16::try_from(usize::from(col) + width)
            .ok()
            .and_then(|right| screen.cell(row, right))
    };
    cell.map_or(vt100::Color::Default, vt100::Cell::bgcolor)
}

/// A colour as RGB: the 256-colour cube and grey ramp by their standard
/// values, the default as the terminal reported it. `None` for the 16
/// themed colours and an unreported default.
fn rgb(color: vt100::Color, default: Option<(u8, u8, u8)>) -> Option<(u8, u8, u8)> {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Some((r, g, b)),
        vt100::Color::Idx(n) if n >= 232 => {
            let v = 8 + 10 * (n - 232);
            Some((v, v, v))
        }
        vt100::Color::Idx(n) if n >= 16 => {
            const LEVEL: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let n = usize::from(n - 16);
            Some((LEVEL[n / 36], LEVEL[n / 6 % 6], LEVEL[n % 6]))
        }
        vt100::Color::Idx(_) => None,
    }
}

/// Whether a row says the harness folded the lines above it, such as
/// `… +20 lines` or `... (+3 more lines)`.
fn folded(row: &[char]) -> bool {
    let text: String = row.iter().collect::<String>().to_lowercase();
    let Some(at) = text.find('…').or_else(|| text.find("...")) else {
        return false;
    };
    let rest = text[at..].trim_start_matches(['…', '.', ' ', '(', '+']);
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let rest = rest[digits..].trim_start();
    digits > 0
        && rest
            .strip_prefix("more ")
            .unwrap_or(rest)
            .starts_with("line")
}

/// Where `row` holds a code fence line left of column `edge`: three or more
/// backticks and at most one word after them (```` ```tiles ````), with
/// nothing else on the row; its first column and width. A model that puts
/// the block inside a fence leaves one on the row above the block's first
/// row and one below its last, and Pi shows both.
fn fence(row: &[char], edge: usize) -> Option<(usize, usize)> {
    let row = &row[..edge.min(row.len())];
    let start = row.iter().position(|c| *c != ' ')?;
    let end = row.iter().rposition(|c| *c != ' ')? + 1;
    let text = &row[start..end];
    let ticks = text.iter().take_while(|c| **c == '`').count();
    let word = text[ticks..]
        .iter()
        .all(|c| c.is_alphanumeric() || (c.is_ascii_punctuation() && *c != '`'));
    (ticks >= 3 && word).then_some((start, end - start))
}

/// The first screen row from `from` up to `end` that is the harness's own
/// footer over `cols` (first column, width): a box border, or a background
/// other than `bg` and the default; `end` when there is none.
fn footer(
    screen: &vt100::Screen,
    (col, width): (u16, u16),
    bg: vt100::Color,
    from: i32,
    end: i32,
) -> i32 {
    (from..end)
        .find(|y| {
            let y = u16::try_from(*y).unwrap_or(u16::MAX);
            (col..col + width).any(|x| {
                screen.cell(y, x).is_some_and(|c| {
                    c.contents()
                        .chars()
                        .next()
                        .is_some_and(|ch| ('\u{2500}'..='\u{257f}').contains(&ch))
                        || (c.bgcolor() != bg && c.bgcolor() != vt100::Color::Default)
                })
            })
        })
        .unwrap_or(end)
}

/// A finite pixel count rounded to a whole number, 0 for anything else.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped to u32 first.
fn whole_px(v: f64) -> u64 {
    if v.is_finite() {
        v.round().clamp(0.0, f64::from(u32::MAX)) as u64
    } else {
        0
    }
}

/// The left `cols` of a band `of` columns wide, for a card cut at a
/// harness's panel.
fn left_part(band: Rgba, cols: u16, of: u16) -> Rgba {
    if cols >= of || of == 0 {
        return band;
    }
    let w = (u64::from(band.width) * u64::from(cols) / u64::from(of)).max(1);
    let w = u32::try_from(w).unwrap_or(band.width);
    let row = band.width as usize * 4;
    let keep = w as usize * 4;
    let data = band
        .data
        .chunks(row)
        .flat_map(|r| &r[..keep])
        .copied()
        .collect();
    Rgba::new(w, band.height, data).unwrap_or(band)
}

fn card_image_id(id: &str) -> u32 {
    kitty::image_id(u64::from_str_radix(id, 16).unwrap_or(1))
}

/// Columns left of a harness's side `panel`
/// ([`crate::harness::Harness::side_panel`]) in a terminal `cols` wide:
/// where its conversation ends.
#[must_use]
pub fn conversation_cols(panel: Option<(u16, u16)>, cols: u16) -> u16 {
    match panel {
        Some((above, width)) if cols > above => cols.saturating_sub(width),
        _ => cols,
    }
}

impl<P: Pictures> Painter<P> {
    /// A painter for a screen of `rows` by `cols`.
    pub fn new(rows: u16, cols: u16, route: Route, pictures: P) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, 0),
            tracker: Tracker::default(),
            route,
            pictures,
            whole: HashSet::new(),
            seen: HashMap::new(),
            placed: HashMap::new(),
            uploaded: HashSet::new(),
            painted: HashMap::new(),
            blanked: Vec::new(),
            next_image: FIRST_IMAGE,
            alternate: false,
            broken: false,
            pending: Vec::new(),
            motion: false,
            default_bg: None,
            now: Duration::ZERO,
            entrances: HashMap::new(),
            early: HashMap::new(),
            preload: Vec::new(),
            redrawn: Vec::new(),
            trace: None,
            panel: None,
            outbox: std::collections::VecDeque::new(),
            sending: HashMap::new(),
            shown: HashSet::new(),
            sync_since: None,
            conversation: None,
            strips: HashMap::new(),
            precut: false,
            ahead: true,
            reels: HashMap::new(),
            bands: HashMap::new(),
            replaced: HashMap::new(),
            deferred: false,
            spent: 0,
            doomed: Vec::new(),
            next_z: 1,
            typing: HashMap::new(),
            waits: HashMap::new(),
            resized: HashSet::new(),
            first_image: FIRST_IMAGE,
        }
    }

    /// Pre-cut: numbers the painter's own pictures from session `session`'s
    /// first image id ([`session_first_image`]), so no earlier session's
    /// card in the same tab shares an id with this one's.
    #[must_use]
    pub fn with_session(mut self, session: u64) -> Self {
        if self.route == Route::KittyPrecut {
            self.first_image = SESSION_IMAGES;
            self.next_image = session_first_image(session);
        }
        self
    }

    /// Keeps every card left of the harness's side `panel`
    /// ([`crate::harness::Harness::side_panel`]).
    #[must_use]
    pub fn with_panel(mut self, panel: Option<(u16, u16)>) -> Self {
        self.panel = panel;
        self
    }

    /// Columns a card may take from column `col` on screen rows `ys`, each
    /// with its block row: up to the harness's side panel, and short of any
    /// text the harness wrote right of a token on those rows (a panel's
    /// lines), within the screen. The first block row is left out, since
    /// the title follows its token.
    fn free_cols(&self, col: u16, ys: impl Iterator<Item = (u16, u16)>) -> u16 {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let mut edge = conversation_cols(self.panel, cols);
        let start = col.saturating_add(u16::try_from(block::TOKEN_LEN).unwrap_or(u16::MAX));
        for (y, row) in ys {
            if row == 1 || y >= rows {
                continue;
            }
            if let Some(x) = (start..edge).find(|x| {
                screen
                    .cell(y, *x)
                    .is_some_and(|c| !c.contents().trim().is_empty())
            }) {
                edge = x;
            }
        }
        edge.saturating_sub(col)
    }

    /// The SGR that resets attributes and sets the conversation's colour
    /// around a block on screen row `row` from column `col`, `width`
    /// columns: the background of the cell just left of it, else just
    /// right of it, else the conversation's remembered background, else
    /// the terminal's default.
    fn surround(&self, row: u16, col: u16, width: usize) -> Vec<u8> {
        let color = match beside(self.parser.screen(), row, col, width) {
            vt100::Color::Default => self.conversation.unwrap_or(vt100::Color::Default),
            color => color,
        };
        sgr_background(color)
    }

    /// Keeps a line per card event (sent, placed early, entrance started,
    /// run, stopped, handed over; on pre-cut also waiting for room, held
    /// while moving, entrance dropped and why, copy lost or scrolled away),
    /// for `TILES_TRACE`.
    #[must_use]
    pub fn with_trace(mut self) -> Self {
        self.trace = Some(Vec::new());
        self
    }

    /// The event lines kept since the last call.
    pub fn take_trace(&mut self) -> Vec<String> {
        self.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    fn note(&mut self, event: impl FnOnce() -> String) {
        let ms = self.now.as_secs_f64() * 1000.0;
        if let Some(t) = &mut self.trace {
            t.push(format!("{ms:9.1} {}", event()));
        }
    }

    /// Plays entrances: in kitty, places cards early, with the terminal's
    /// default background as it reported it for the backing; on the pre-cut
    /// route, a frame per tick.
    #[must_use]
    pub fn with_motion(mut self, default_bg: Option<(u8, u8, u8)>) -> Self {
        self.motion = self.route == Route::KittyPlaceholders;
        self.precut = self.route == Route::KittyPrecut;
        self.default_bg = default_bg;
        self
    }

    /// Pre-cut: sends each entrance frame as it is placed instead of ahead,
    /// for a terminal that will not place a picture sent earlier
    /// (`TILES_PRECUT_SEND=tick`). A frame over [`SEND_BUDGET`], such as a
    /// dashboard's, drops the entrance for the crisp card.
    #[must_use]
    pub fn with_frames_each_tick(mut self) -> Self {
        self.ahead = false;
        self
    }

    /// Moves the painter's clock to `now`, from any fixed start.
    pub fn set_now(&mut self, now: Duration) {
        self.now = self.now.max(now);
    }

    /// Time passed: sends the pictures of new cards ahead, runs, keeps
    /// running and stops entrances, places a moved pre-cut card again, and
    /// cuts a pre-cut card whose block stopped being typed.
    /// Writes only at ground state.
    pub fn tick(&mut self, now: Duration) -> Vec<u8> {
        self.set_now(now);
        let due = self.deferred || !self.doomed.is_empty() || self.stalled();
        if self.broken || !(self.motion || self.precut || due) {
            return self.pass(&[]);
        }
        contained(|| {
            let mut out = Vec::new();
            let fresh = self.pictures.fresh();
            for id in &fresh {
                self.note(|| format!("{id} in the cache"));
            }
            self.preload.extend(fresh);
            if !self.tracker.ground() {
                return out;
            }
            if self.precut || due {
                self.tick_precut(&mut out);
                return out;
            }
            for id in std::mem::take(&mut self.preload) {
                if let Some(card) = self.pictures.card(&id) {
                    self.upload(&mut out, &id, &card);
                }
            }
            self.redrawn.extend(self.pictures.redrawn());
            self.swap(&mut out);
            self.flush(&mut out);
            self.step(&mut out);
            out
        })
        .unwrap_or_else(|| self.give_up(&[]))
    }

    /// Whether a panic stopped the painter; it passes bytes through since.
    #[must_use]
    pub fn broken(&self) -> bool {
        self.broken
    }

    /// The harness's bytes, then whatever paint they call for.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        if self.broken {
            self.tracker.feed(chunk);
            return self.pass(chunk);
        }
        // A panic can come after part of the chunk was read, and the deletes
        // owed go out only at ground, so the tracker reads all of it first.
        let before = self.tracker.clone();
        contained(|| self.feed_painted(chunk)).unwrap_or_else(|| {
            self.tracker = before;
            self.tracker.feed(chunk);
            self.give_up(chunk)
        })
    }

    /// The terminal changed size: follow it and place every card again.
    /// Pre-cut pictures stay on screen until the next paint places their
    /// cards again over them, and go a tick later as on any move: deleted
    /// here, every card blinked out for about 70 ms in Warp before the
    /// harness drew the screen again, and a card whose harness stayed
    /// quiet never came back.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Vec<u8> {
        if self.broken {
            return self.pass(&[]);
        }
        contained(|| {
            self.parser.screen_mut().set_size(rows, cols);
            self.painted.clear();
            if self.route != Route::KittyPrecut {
                return self.forget_placements();
            }
            self.blanked.clear();
            self.resized.extend(self.strips.keys().cloned());
            let kept = self.resized.len();
            self.note(|| {
                format!("resized to {rows} by {cols}; {kept} cards kept until placed again")
            });
            Vec::new()
        })
        .unwrap_or_else(|| self.give_up(&[]))
    }

    /// The screen model panicked, so its picture of the screen is no longer
    /// trusted: remove the pictures placed so far and draw nothing again.
    /// The tracker has already read `chunk`.
    fn give_up(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.broken = true;
        // The same removal as a hand-over: an early placement may be the
        // entrance's image rather than the card's.
        let mut owed = Vec::new();
        for id in self.early.keys().cloned().collect::<Vec<_>>() {
            self.unplace_early(&mut owed, &id);
        }
        self.pending.append(&mut owed);
        for strip in self.strips.drain().flat_map(|(_, s)| s) {
            self.pending.extend_from_slice(&kitty::delete(strip.image));
        }
        for (image, _) in std::mem::take(&mut self.doomed) {
            self.pending.extend_from_slice(&kitty::delete(image));
        }
        for reel in std::mem::take(&mut self.reels).into_values() {
            for image in &reel.images[..reel.sent] {
                self.pending.extend_from_slice(&kitty::delete(*image));
            }
        }
        self.placed.clear();
        self.pass(chunk)
    }

    /// `chunk` unchanged, then the deletes still owed once the stream is at
    /// ground.
    fn pass(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = chunk.to_vec();
        if self.tracker.ground() {
            out.append(&mut self.pending);
        }
        out
    }

    /// While the harness holds a synchronized update its frame is half
    /// drawn, and the screen model shows parts of two frames: a paint then
    /// cut a card at text from the next frame and blanked letters the
    /// harness had just written. So the paint goes in just before the
    /// update ends, inside it, or once the update has been held for
    /// [`SYNC_WAIT`].
    ///
    /// Bytes the harness wrote outside an update of its own go in the
    /// paint's own update with it, so the terminal never shows a token
    /// typed and not yet blanked: a row of `[tile:` text on the default
    /// background flashed before its blank in Warp.
    fn feed_painted(&mut self, chunk: &[u8]) -> Vec<u8> {
        const END: &[u8] = b"\x1b[?2026l";
        let mut out = Vec::with_capacity(chunk.len());
        let end = chunk.windows(END.len()).rposition(|w| w == END);
        let rest = match end {
            Some(at) => {
                let head = &chunk[..at];
                out.extend_from_slice(head);
                self.tracker.feed(head);
                self.parser.process(head);
                if self.tracker.ground() && self.tracker.harness_sync {
                    self.sync_since = None;
                    out.extend(self.paint_now(&[]));
                }
                let (end, rest) = chunk[at..].split_at(END.len());
                out.extend_from_slice(end);
                self.tracker.feed(end);
                self.parser.process(end);
                rest
            }
            None => chunk,
        };
        // A read that ends inside a sequence is painted at its last ground
        // point, and the unfinished sequence follows the paint: a block
        // written in one go reached the wrapper in reads that ended mid
        // sequence, and its typed rows showed unblanked until the next.
        // The next read then begins inside that sequence: its end passes
        // before the paint's update and the rows after it go in the update,
        // else they showed unblanked too.
        let mut probe = self.tracker.clone();
        let mut ground = if probe.ground() { Some(0) } else { None };
        let mut first = ground;
        for (i, b) in rest.iter().enumerate() {
            probe.feed(std::slice::from_ref(b));
            if probe.ground() {
                first.get_or_insert(i + 1);
                ground = Some(i + 1);
            }
        }
        let (rest, tail) = rest.split_at(ground.unwrap_or(0));
        if rest.is_empty() && !tail.is_empty() {
            out.extend_from_slice(tail);
            self.tracker.feed(tail);
            self.parser.process(tail);
            return out;
        }
        let (open, rest) = rest.split_at(first.unwrap_or(0));
        out.extend_from_slice(open);
        self.tracker.feed(open);
        self.parser.process(open);
        out.extend(self.feed_at_ground(rest));
        out.extend_from_slice(tail);
        self.tracker.feed(tail);
        self.parser.process(tail);
        out
    }

    /// [`Painter::feed_painted`] for bytes that end at ground: they, then
    /// the paint, unless the harness holds its update.
    fn feed_at_ground(&mut self, rest: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        // Held back for the paint's own update only from ground with no
        // update of the harness's open, so the update's start never lands
        // inside one of the harness's sequences.
        let free = self.tracker.ground() && !self.tracker.harness_sync;
        self.tracker.feed(rest);
        self.parser.process(rest);
        if self.tracker.harness_sync {
            out.extend_from_slice(rest);
            let since = *self.sync_since.get_or_insert(self.now);
            if self.now.saturating_sub(since) < SYNC_WAIT {
                return out;
            }
        } else {
            self.sync_since = None;
            if !free {
                out.extend_from_slice(rest);
            }
        }
        let before = if free && !self.tracker.harness_sync {
            rest
        } else {
            &[]
        };
        out.extend(self.paint_now(before));
        out
    }

    /// The paint for the screen as it stands, with the harness's cursor and
    /// drawing attributes put back, in a synchronized update of its own
    /// unless the harness holds one, after `before`, the harness's bytes
    /// that update holds too.
    fn paint_now(&mut self, before: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut paint = self.paint();
        if self.motion {
            self.step(&mut paint);
        }
        if paint.is_empty() {
            out.extend_from_slice(before);
            return out;
        }
        let sync = !self.tracker.harness_sync;
        if sync {
            out.extend_from_slice(b"\x1b[?2026h");
        }
        out.extend_from_slice(before);
        out.extend_from_slice(&paint);
        let screen = self.parser.screen();
        out.extend_from_slice(&screen.attributes_formatted());
        let (y, x) = screen.cursor_position();
        cup(&mut out, y, x.min(screen.size().1.saturating_sub(1)));
        if sync {
            out.extend_from_slice(b"\x1b[?2026l");
        }
        out
    }

    fn forget_placements(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        for id in self.early.keys().cloned().collect::<Vec<_>>() {
            self.unplace_early(&mut out, &id);
        }
        self.placed.clear();
        for strip in self.strips.drain().flat_map(|(_, s)| s) {
            out.extend_from_slice(&kitty::delete(strip.image));
        }
        for (image, _) in std::mem::take(&mut self.doomed) {
            out.extend_from_slice(&kitty::delete(image));
        }
        self.resized.clear();
        self.painted.clear();
        self.blanked.clear();
        out
    }

    fn take_image_id(&mut self) -> u32 {
        let id = self.next_image;
        let last = if self.first_image == SESSION_IMAGES {
            LAST_SESSION_IMAGE
        } else {
            LAST_IMAGE
        };
        self.next_image = if id >= last { self.first_image } else { id + 2 };
        id
    }

    /// Pre-cut: the stacking order for the next picture placed, above every
    /// one placed before it.
    fn take_z(&mut self) -> i32 {
        let z = self.next_z;
        self.next_z = z.saturating_add(1);
        z
    }

    /// Pre-cut: deletes `image` a tick from now, once the picture that
    /// replaces it has had time to show.
    fn retire(&mut self, image: u32) {
        self.doomed.push((image, self.now + TICK));
    }

    /// Pre-cut: deletes the replaced pictures whose time has come.
    fn bury(&mut self, out: &mut Vec<u8>) {
        let now = self.now;
        self.doomed.retain(|(image, due)| {
            if *due <= now {
                out.extend_from_slice(&kitty::delete(*image));
                false
            } else {
                true
            }
        });
    }

    fn row_cells(screen: &vt100::Screen, y: u16, cols: u16) -> Vec<char> {
        (0..cols)
            .map(|x| match screen.cell(y, x) {
                Some(c) if c.is_wide_continuation() => ' ',
                Some(c) => {
                    let mut chars = c.contents().chars();
                    match (chars.next(), chars.next()) {
                        (None, _) => ' ',
                        (Some(ch), None) => ch,
                        (Some(_), Some(_)) => '\0',
                    }
                }
                None => ' ',
            })
            .collect()
    }

    fn scan(screen: &vt100::Screen, rows: u16, cols: u16) -> Scan {
        let mut scan = Scan {
            cells: Vec::new(),
            sigs: Vec::new(),
            found: Vec::new(),
            partials: Vec::new(),
        };
        for y in 0..rows {
            let row = Self::row_cells(screen, y, cols);
            let mut h = std::collections::hash_map::DefaultHasher::new();
            row.hash(&mut h);
            scan.sigs.push(h.finish());
            scan.found
                .extend(block::find(&row).into_iter().map(|t| (y, t)));
            if let Some(at) = block::partial(&row) {
                scan.partials.push((y, at));
            }
            scan.cells.push(row);
        }
        scan
    }

    fn paint(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        if screen.alternate_screen() != self.alternate {
            self.alternate = screen.alternate_screen();
            self.conversation = None;
            out.extend(self.forget_placements());
        }
        let Scan {
            cells,
            sigs,
            found,
            partials,
        } = Self::scan(self.parser.screen(), rows, cols);
        if (!found.is_empty() || !partials.is_empty())
            && let Some(bg) = screen_background(self.parser.screen())
        {
            self.conversation = Some(bg);
        }
        // Group tokens into copies: same card, same first screen row, same column.
        let mut copies: Copies = HashMap::new();
        let mut totals: HashMap<String, u16> = HashMap::new();
        for (y, t) in &found {
            let top = i32::from(*y) - i32::from(t.row) + 1;
            copies
                .entry((t.id.clone(), top, t.col))
                .or_default()
                .push((t.row, *y));
            totals.insert(t.id.clone(), t.rows);
        }
        let mut best: HashMap<String, Copy> = HashMap::new();
        for ((id, _, col), members) in &mut copies {
            members.sort_unstable();
            let total = totals[id.as_str()];
            let (lo, first_y) = members[0];
            let (hi, _) = members[members.len() - 1];
            let contiguous = members
                .windows(2)
                .all(|w| w[1].0 == w[0].0 + 1 && w[1].1 == w[0].1 + 1);
            if contiguous {
                self.see(id, (lo, hi), total, first_y);
            }
            if !contiguous || !self.whole.contains(id) || members.len() < usize::from(total.min(2))
            {
                continue;
            }
            let copy = Copy {
                y: first_y,
                col: u16::try_from(*col).unwrap_or(u16::MAX),
                lo,
                hi,
                typed: hi,
            };
            let better = best.get(id).is_none_or(|b| copy.hi - copy.lo > b.hi - b.lo);
            if better {
                best.insert(id.clone(), copy);
            }
        }
        let early = if self.motion {
            self.early_copies(&copies, &best, &cells, rows)
        } else {
            HashMap::new()
        };
        if self.route == Route::KittyPrecut {
            let growing = self.growing(&copies, &totals, &cells, rows);
            best.extend(growing);
        }
        self.blank_tokens(&mut out, &best, (&cells, &sigs), &found, &partials);
        let mut ids: Vec<&String> = best.keys().collect();
        ids.sort();
        let ids: Vec<String> = ids.into_iter().cloned().collect();
        for id in &ids {
            let copy = best[id];
            if let Some(card) = self.pictures.card(id) {
                if !self.whole.contains(id) && totals.get(id) != Some(&card.rows) {
                    continue;
                }
                self.draw(&mut out, id, copy, &card, &sigs, rows);
            }
        }
        self.place_all_early(&mut out, &early, &totals);
        self.drop_gone(&mut out, &best);
        out
    }

    /// Blanks every token that is not drawn over by a copy in `best`, a
    /// waiting block's first row to the end of its text within the width
    /// its card will be drawn at, half-typed tokens and
    /// a fence line right above a block or below any of its rows
    /// ([`fence`]), and gives cells blanked last time back.
    fn blank_tokens(
        &mut self,
        out: &mut Vec<u8>,
        best: &HashMap<String, Copy>,
        (cells, sigs): (&[Vec<char>], &[u64]),
        found: &[(u16, Token)],
        partials: &[(u16, usize)],
    ) {
        let drawn = |y: u16, t: &Token| {
            best.get(&t.id).is_some_and(|c| {
                y >= c.y && y <= c.y + (c.hi - c.lo) && usize::from(c.col) == t.col
            })
        };
        let edge = usize::from(conversation_cols(
            self.panel,
            u16::try_from(cells.first().map_or(0, Vec::len)).unwrap_or(u16::MAX),
        ));
        // Pre-cut: a block waiting for room, or held until its card stops
        // moving, has its first row blanked to the end of its text within
        // the width its card will be drawn at, so the wait shows empty rows
        // rather than its title in the harness's font; text right of the
        // card, or that cuts it, stays. The title is one run of text after
        // the token's space, so the blank stops at a gap of two spaces: a
        // first row alone has no row under it to cut the card, and the
        // harness may scroll it up without writing it again.
        let to_end = |y: u16, at: usize, cols: u16| {
            let row = &cells[usize::from(y)];
            let row = &row[..edge.min(at + usize::from(cols)).min(row.len())];
            let start = at + block::TOKEN_LEN;
            let mut end = start;
            for x in start..row.len() {
                if row[x] != ' ' {
                    end = x + 1;
                } else if x > start && row[x - 1] == ' ' {
                    break;
                }
            }
            end
        };
        let mut blanks: Vec<(u16, usize, usize)> = Vec::new();
        for (y, t) in found.iter().filter(|(y, t)| !drawn(*y, t)) {
            let mut end = t.col + block::TOKEN_LEN;
            if t.row == 1
                && self.waits.contains_key(&t.id)
                && let Some(card) = self.pictures.card(&t.id)
            {
                let typed = found
                    .iter()
                    .filter(|(uy, u)| u.id == t.id && u.col == t.col && *uy == y + (u.row - 1))
                    .map(|(uy, u)| (*uy, u.row));
                let col = u16::try_from(t.col).unwrap_or(u16::MAX);
                end = to_end(*y, t.col, card.cols.min(self.free_cols(col, typed))).max(end);
            }
            blanks.push((*y, t.col, end - t.col));
        }
        blanks.extend(partials.iter().map(|(y, at)| {
            let end = cells[usize::from(*y)]
                .iter()
                .rposition(|c| *c != ' ')
                .map_or(*at, |e| e + 1);
            (*y, *at, end - at)
        }));
        // Pre-cut: the rows a card covers that its block has not typed yet
        // hold the harness's own lines (Pi's editor, `OpenCode`'s "Build"
        // line): blanked like tokens, on the background beside the block,
        // so none shows through the card's margins or its fading frames,
        // and given back once the card leaves them.
        let mut ids: Vec<&String> = best.keys().collect();
        ids.sort();
        for id in ids {
            let copy = best[id];
            if copy.typed >= copy.hi {
                continue;
            }
            let Some(card) = self.pictures.card(id).filter(|c| c.rows >= copy.hi) else {
                continue;
            };
            let typed = copy.typed_rows().map(|y| (y, copy.lo + (y - copy.y)));
            let width = usize::from(card.cols.min(self.free_cols(copy.col, typed)));
            if width == 0 {
                continue;
            }
            for y in copy.typed_rows().end..copy.y + (copy.hi - copy.lo + 1) {
                blanks.push((y, usize::from(copy.col), width));
            }
        }
        // Pi draws a closing fence under the last row typed so far while
        // the block streams, so the row under any token row may hold one;
        // otherwise that row holds the next token.
        for (y, t) in found {
            let above = (t.row == 1).then(|| y.checked_sub(1)).flatten();
            let below = Some(y + 1);
            for fy in [above, below].into_iter().flatten() {
                let Some(row) = cells.get(usize::from(fy)) else {
                    continue;
                };
                if let Some((x, len)) = fence(row, edge).filter(|(x, _)| *x <= t.col)
                    && !blanks.contains(&(fy, x, len))
                {
                    blanks.push((fy, x, len));
                }
            }
        }
        // A row a card covers is drawn over this paint: writing its old
        // text back first would only be blanked again.
        let covered = |y: u16, x: u16| {
            best.values()
                .any(|c| x == c.col && y >= c.y && y <= c.y + (c.hi - c.lo))
        };
        self.unblank(out, &blanks, covered);
        // A row can take several blanks in one paint, as a fence left of
        // the rows a card covers that its block has not typed yet.
        let done: HashSet<u16> = blanks
            .iter()
            .map(|(y, _, _)| *y)
            .filter(|y| self.painted.get(y) == Some(&sigs[usize::from(*y)]))
            .collect();
        for (y, x, len) in blanks {
            if done.contains(&y) {
                continue;
            }
            let x = u16::try_from(x).unwrap_or(0);
            cup(out, y, x);
            out.extend(self.surround(y, x, len));
            out.extend(std::iter::repeat_n(b' ', len));
            self.painted.insert(y, sigs[usize::from(y)]);
        }
    }

    /// Pre-cut: cards still being typed, each as the copy typed furthest
    /// that starts at its block's first row, unless the harness folded it
    /// ("… +20 lines" right under it). A card already on screen follows its
    /// block when the harness scrolls the block's first rows away before
    /// its last is typed, off the top or under its own header (`OpenCode`'s
    /// tab bar): a copy typed at least as far that starts lower in the
    /// block. Each copy reaches past its typed rows to the rows its block
    /// will take, so the whole card shows from the block's first row and the
    /// rows typed under it change nothing (ADR-044); a block with fewer rows
    /// left on screen than it will take is left out, so its card never shows
    /// cut at the screen's last row, and once it has room, until its card
    /// stops moving ([`Painter::hold_moving`]). The reach ends, the card cut to the
    /// typed rows, once the block has gone [`STALL`] without a new row,
    /// until it grows again.
    /// A card counts as whole only once every row has been seen
    /// ([`Painter::see`]).
    fn growing(
        &mut self,
        copies: &Copies,
        totals: &HashMap<String, u16>,
        cells: &[Vec<char>],
        rows: u16,
    ) -> HashMap<String, Copy> {
        let mut out: HashMap<String, Copy> = HashMap::new();
        for ((id, _, col), members) in copies {
            if self.whole.contains(id) {
                continue;
            }
            let (lo, first_y) = members[0];
            let (hi, last_y) = members[members.len() - 1];
            let contiguous = members
                .windows(2)
                .all(|w| w[1].0 == w[0].0 + 1 && w[1].1 == w[0].1 + 1);
            let scrolled = lo > 1
                && self.strips.contains_key(id)
                && self.typing.get(id).is_some_and(|t| hi >= t.typed);
            let Some(total) = totals.get(id).copied().filter(|t| hi <= *t) else {
                continue;
            };
            if (lo != 1 && !scrolled)
                || !contiguous
                || (last_y + 1 < rows && folded(&cells[usize::from(last_y + 1)]))
            {
                continue;
            }
            let room = rows.saturating_sub(first_y);
            if lo + room <= total {
                if !self.strips.contains_key(id) {
                    let top = i32::from(first_y) - i32::from(lo) + 1;
                    let wait = Wait {
                        top,
                        typed: hi,
                        still: None,
                        held: false,
                    };
                    if self.waits.insert(id.clone(), wait).is_none() {
                        let needs = total - lo + 1;
                        self.note(|| {
                            format!("{id} waiting for room: needs {needs} rows, has {room}")
                        });
                    }
                }
                continue;
            }
            let copy = Copy {
                y: first_y,
                col: u16::try_from(*col).unwrap_or(u16::MAX),
                lo,
                hi: total,
                typed: hi,
            };
            if out.get(id).is_none_or(|c| copy.typed > c.typed) {
                out.insert(id.clone(), copy);
            }
        }
        let now = self.now;
        for (id, copy) in &mut out {
            let fresh = Typing {
                typed: copy.typed,
                since: now,
                stopped: false,
            };
            let t = self.typing.entry(id.clone()).or_insert(fresh);
            // A stalled block drawn again row by row shows rows already
            // seen, not new ones.
            if copy.typed > t.typed || (!t.stopped && copy.typed != t.typed) {
                *t = fresh;
            } else if now.saturating_sub(t.since) >= STALL {
                t.stopped = true;
            }
            if t.stopped {
                copy.hi = copy.typed;
            }
        }
        let whole = &self.whole;
        self.typing
            .retain(|id, t| out.contains_key(id) || (t.stopped && !whole.contains(id)));
        self.waits
            .retain(|id, _| !whole.contains(id) && copies.keys().any(|(i, _, _)| i == id));
        self.hold_moving(&mut out);
        out
    }

    /// Pre-cut: leaves out of `out` each block that waited for room and
    /// whose card would still move, so its entrance plays in one place. It
    /// goes up once a row is typed with the block in place and a tick
    /// passes with no move, once its typing stalls, or, outside
    /// [`Painter::growing`], once it is whole. Stillness alone is no sign:
    /// a block typed at the foot of a full screen scrolls up a row with
    /// each row, 100 to 150 ms apart, longer than a tick. A card with no
    /// entrance to play is never held.
    fn hold_moving(&mut self, out: &mut HashMap<String, Copy>) {
        let now = self.now;
        let mut held = Vec::new();
        for (id, w) in &mut self.waits {
            let Some(copy) = out.get(id) else {
                continue;
            };
            let top = i32::from(copy.y) - i32::from(copy.lo) + 1;
            if top != w.top {
                w.still = None;
            } else if copy.typed > w.typed {
                w.still.get_or_insert(now);
            }
            w.top = top;
            w.typed = copy.typed;
            let still = w.still.is_some_and(|s| now.saturating_sub(s) >= TICK);
            let stopped = self.typing.get(id).is_some_and(|t| t.stopped);
            let entrance = self.reels.get(id).is_some_and(|r| r.since.is_none());
            if entrance && !still && !stopped {
                held.push((id.clone(), (!w.held).then_some(copy.y)));
                w.held = true;
                self.deferred |= w.still.is_some();
            }
        }
        for (id, first) in &held {
            out.remove(id);
            if let Some(y) = first {
                self.note(|| {
                    format!("{id} has room at screen row {y}; held until its card stops moving")
                });
            }
        }
        let held: Vec<String> = held.into_iter().map(|(id, _)| id).collect();
        self.waits
            .retain(|id, _| held.contains(id) || !out.contains_key(id));
    }

    /// Pre-cut: whether a block still being typed has gone [`STALL`]
    /// without a new row and its card still reaches past its typed rows.
    fn stalled(&self) -> bool {
        self.typing
            .values()
            .any(|t| !t.stopped && self.now.saturating_sub(t.since) >= STALL)
    }

    /// Records that one copy of card `id` showed its block rows `lo` to
    /// `hi` of `total`, the first at screen row `first_y`, and marks the
    /// card whole once every row has been seen, or once a copy ending on the
    /// last row starts at the top of the screen.
    fn see(&mut self, id: &str, (lo, hi): (u16, u16), total: u16, first_y: u16) {
        if self.whole.contains(id) {
            return;
        }
        let seen = self.seen.entry(id.to_string()).or_default();
        seen.extend(lo..=hi);
        let typed = (1..=total).all(|r| seen.contains(&r));
        if typed || (hi == total && first_y == 0) {
            self.seen.remove(id);
            self.whole.insert(id.to_string());
            self.note(|| format!("{id} whole: rows {lo} to {hi} from screen row {first_y}"));
        }
    }

    /// Gives the cells blanked last time and not in `blanks` (row, first
    /// column, length) the harness's text back. A harness that redraws only
    /// the cells it changed (`OpenCode`) leaves a blanked cell blank when its
    /// new text has the same letter there ("[tile" and "The example" share
    /// the fifth).
    fn unblank(
        &mut self,
        out: &mut Vec<u8>,
        blanks: &[(u16, usize, usize)],
        covered: impl Fn(u16, u16) -> bool,
    ) {
        let blanked: Vec<(u16, u16, u16)> = blanks
            .iter()
            .map(|(y, x, len)| {
                let x = u16::try_from(*x).unwrap_or(u16::MAX);
                (*y, x, u16::try_from(*len).unwrap_or(u16::MAX))
            })
            .collect();
        for (y, x, len) in std::mem::take(&mut self.blanked) {
            if !blanked.contains(&(y, x, len)) && !covered(y, x) {
                self.write_back(out, y, x, len);
            }
        }
        self.blanked = blanked;
    }

    /// Writes back what the harness last wrote in the `len` cells of row
    /// `y` from column `x`, cleared to the terminal's default first.
    fn write_back(&mut self, out: &mut Vec<u8>, y: u16, x: u16, len: u16) {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let len = len.min(cols.saturating_sub(x));
        if y >= rows || len == 0 {
            return;
        }
        let Some(row) = screen.rows_formatted(x, len).nth(usize::from(y)) else {
            return;
        };
        cup(out, y, x);
        out.extend_from_slice(b"\x1b[0m");
        out.extend(std::iter::repeat_n(b' ', usize::from(len)));
        cup(out, y, x);
        out.extend_from_slice(&row);
        self.painted.remove(&y);
        self.note(|| format!("row {y} written back from column {x} over {len} columns"));
    }

    /// Writes back what the harness last wrote in the cells a card's
    /// placeholders took on the rows of `old` not `kept`: a harness that
    /// redraws only the cells it changed (`OpenCode`) never rewrites cells it
    /// thinks blank, so a block that moved left pieces of its card there.
    fn restore(&mut self, out: &mut Vec<u8>, old: Copy, cols: u16, kept: impl Fn(u16) -> bool) {
        let screen = self.parser.screen();
        let (rows, screen_cols) = screen.size();
        let width = cols.min(screen_cols.saturating_sub(old.col));
        if width == 0 {
            return;
        }
        let gone: Vec<u16> = (old.y..old.y + (old.hi - old.lo + 1))
            .filter(|y| *y < rows && !kept(*y))
            .collect();
        if gone.is_empty() {
            return;
        }
        // The formatted row stops at its last written cell, so the cells are
        // cleared to the terminal's default first.
        let formatted: Vec<Vec<u8>> = screen.rows_formatted(old.col, width).collect();
        let (first, last) = (gone[0], gone[gone.len() - 1]);
        for y in gone {
            cup(out, y, old.col);
            out.extend_from_slice(b"\x1b[0m");
            out.extend(std::iter::repeat_n(b' ', usize::from(width)));
            cup(out, y, old.col);
            out.extend_from_slice(&formatted[usize::from(y)]);
            self.painted.remove(&y);
        }
        self.note(|| {
            format!(
                "rows {first} to {last} written back from column {} over {width} columns",
                old.col
            )
        });
    }

    /// Forgets the cards that left the screen, deleting their pre-cut
    /// pictures, with a trace line for each, and writing back the
    /// harness's cells under placeholders.
    fn drop_gone(&mut self, out: &mut Vec<u8>, best: &HashMap<String, Copy>) {
        let gone: Vec<String> = self
            .placed
            .keys()
            .filter(|k| !best.contains_key(*k))
            .cloned()
            .collect();
        for id in gone {
            if let Some((old, _)) = self.placed.remove(&id)
                && self.route == Route::KittyPlaceholders
                && let Some(card) = self.pictures.card(&id)
            {
                self.restore(out, old, card.cols, |_| false);
            }
        }
        let gone: Vec<String> = self
            .strips
            .keys()
            .filter(|k| !best.contains_key(*k))
            .cloned()
            .collect();
        for id in gone {
            let strips = self.strips.remove(&id).unwrap_or_default();
            for strip in &strips {
                out.extend_from_slice(&kitty::delete(strip.image));
            }
            // A card whose first rows had already left the conversation's
            // top, or a whole one whose last rows had left its foot, scrolled
            // away; one that had all of them lost its block's tokens on
            // screen, as when `OpenCode` drew the rows again as links.
            let total = self.pictures.card(&id).map_or(0, |c| c.rows);
            let foot = self.whole.contains(&id) && strips.last().is_some_and(|s| s.hi < total);
            if let Some(s) = strips.first() {
                let y = s.y;
                if s.lo > 1 || foot {
                    self.note(|| format!("{id} scrolled away; pictures deleted"));
                } else {
                    self.note(|| format!("{id} copy lost at screen row {y}; pictures deleted"));
                }
            }
        }
        self.bands.retain(|id, _| best.contains_key(id));
        self.replaced.retain(|id, _| best.contains_key(id));
        let strips = &self.strips;
        self.resized.retain(|id| strips.contains_key(id));
    }

    /// Places each of `early` (first screen row, column, last typed row)
    /// whose block is as tall as its card, and removes the early
    /// placements no longer called for.
    fn place_all_early(
        &mut self,
        out: &mut Vec<u8>,
        early: &HashMap<String, (i32, u16, u16)>,
        totals: &HashMap<String, u16>,
    ) {
        let mut ids: Vec<&String> = early.keys().collect();
        ids.sort();
        for id in ids {
            let (top, col, last) = early[id];
            if let Some(card) = self.pictures.card(id)
                && totals.get(id) == Some(&card.rows)
            {
                self.place_early(out, id, &card, top, col, last);
            }
        }
        let left: Vec<String> = self
            .early
            .keys()
            .filter(|k| !early.contains_key(*k))
            .cloned()
            .collect();
        for id in left {
            self.unplace_early(out, &id);
        }
    }

    /// Cards still being typed: the copy that starts at the block's first
    /// row, unless the harness folded it ("… +20 lines" right under it),
    /// as its first screen row, column and last typed row. A whole card
    /// whose entrance still plays keeps its early placement, following its
    /// block, since handing over mid-entrance left the prototype's picture
    /// stuck on an early frame.
    fn early_copies(
        &self,
        copies: &Copies,
        best: &HashMap<String, Copy>,
        cells: &[Vec<char>],
        rows: u16,
    ) -> HashMap<String, (i32, u16, u16)> {
        let mut early: HashMap<String, (i32, u16, u16)> = HashMap::new();
        for ((id, _, col), members) in copies {
            let (lo, first_y) = members[0];
            let (hi, last_y) = members[members.len() - 1];
            let contiguous = members
                .windows(2)
                .all(|w| w[1].0 == w[0].0 + 1 && w[1].1 == w[0].1 + 1);
            // A card placed early follows its block when the harness scrolls
            // the block's first rows off the top before its last is typed.
            let scrolled = lo > 1 && first_y == 0 && self.early.contains_key(id);
            if self.whole.contains(id)
                || (lo != 1 && !scrolled)
                || !contiguous
                || (last_y + 1 < rows && folded(&cells[usize::from(last_y + 1)]))
            {
                continue;
            }
            let col = u16::try_from(*col).unwrap_or(u16::MAX);
            let top = i32::from(first_y) - i32::from(lo) + 1;
            // The copy typed furthest wins.
            if early
                .get(id)
                .is_none_or(|(t, _, l)| i32::from(*l) - t < i32::from(hi) - 1)
            {
                early.insert(id.clone(), (top, col, last_y));
            }
        }
        for (id, copy) in best {
            // A block typed whole in one go plays its entrance over it too.
            let ready = !self.early.contains_key(id)
                && copy.lo == 1
                && self
                    .entrances
                    .get(id)
                    .is_some_and(|e| e.state == Play::Ready);
            if ready || (self.early.contains_key(id) && self.playing(id)) {
                let top = i32::from(copy.y) - i32::from(copy.lo) + 1;
                early.insert(id.clone(), (top, copy.col, copy.y + (copy.hi - copy.lo)));
            }
        }
        early
    }

    /// Sends card `id` to kitty once, with a virtual placement its
    /// placeholder cells draw through, and in kitty queues its entrance as
    /// an animation of its own (the settled card, the entrance from its
    /// first visible frame, the settled card) for [`Painter::flush`] to
    /// send a frame at a time.
    fn upload(&mut self, out: &mut Vec<u8>, id: &str, card: &CardInfo) {
        if !self.uploaded.insert(id.to_string()) {
            return;
        }
        let at = out.len();
        out.extend(kitty::virtual_png(
            card_image_id(id),
            &card.png,
            card.cols,
            card.rows,
        ));
        let bytes = out.len() - at;
        self.note(|| format!("{id} sent settled, {bytes} bytes"));
        if !self.motion || card.frames < 3 || self.shown.contains(id) {
            return;
        }
        let frames = self.pictures.frames(id);
        let Some((settled, [_, entrance @ ..])) = frames.split_last() else {
            return;
        };
        let Some(size_px) = tiles_term::image::png_size(settled) else {
            return;
        };
        // Kitty moves on one frame per redraw once a frame's time has
        // passed; a gap 10 ms short of it moves on at every redraw
        // (prototype, open/render/motion.py).
        let gap = u32::from(card.frame_ms).saturating_sub(10).max(1);
        let image = self.take_image_id();
        self.outbox
            .push_back((id.to_string(), kitty::transmit_png(image, settled)));
        for frame in entrance {
            self.outbox
                .push_back((id.to_string(), kitty::frame_png(image, frame, gap)));
        }
        self.outbox
            .push_back((id.to_string(), kitty::frame_png(image, settled, gap)));
        let last = u32::try_from(entrance.len()).unwrap_or(u32::MAX - 2) + 2;
        self.sending.insert(
            id.to_string(),
            Entrance {
                image,
                size_px,
                last,
                frame_ms: u32::from(card.frame_ms.max(1)),
                shown: 1,
                state: Play::Ready,
            },
        );
    }

    /// Sends each card drawn again under its id, once it is no longer
    /// placed early or playing its entrance: the new picture and its
    /// virtual placement under the card's image id, which kitty puts in
    /// place of the old under every placeholder cell already drawn. A card
    /// not sent yet goes out new when its block shows.
    fn swap(&mut self, out: &mut Vec<u8>) {
        let mut wait = Vec::new();
        for id in std::mem::take(&mut self.redrawn) {
            if !self.uploaded.contains(&id) {
                continue;
            }
            if self.early.contains_key(&id) || self.playing(&id) {
                wait.push(id);
                continue;
            }
            self.settle(out, &id);
            if let Some(card) = self.pictures.card(&id) {
                out.extend(kitty::virtual_png(
                    card_image_id(&id),
                    &card.png,
                    card.cols,
                    card.rows,
                ));
            }
        }
        self.redrawn = wait;
    }

    /// Writes queued entrance frames, about [`SEND_BUDGET`] bytes less what
    /// pre-cut cards placed again for a move took since the last call, and
    /// at least one frame when nothing did; a card whose last frame goes
    /// out can play.
    fn flush(&mut self, out: &mut Vec<u8>) {
        let mut budget = SEND_BUDGET.saturating_sub(std::mem::take(&mut self.spent));
        while let Some((_, bytes)) = self.outbox.front() {
            if budget < SEND_BUDGET && bytes.len() > budget {
                break;
            }
            let Some((id, bytes)) = self.outbox.pop_front() else {
                break;
            };
            budget = budget.saturating_sub(bytes.len());
            out.extend(bytes);
            if let Some(r) = self.reels.get_mut(&id) {
                r.sent += 1;
            }
            if self.outbox.iter().any(|(i, _)| *i == id) {
                continue;
            }
            if self.reels.contains_key(&id) {
                self.note(|| format!("{id} entrance in the terminal"));
            }
            if let Some(e) = self.sending.remove(&id) {
                self.note(|| format!("{id} entrance in the terminal"));
                self.entrances.insert(id, e);
            }
        }
    }

    /// Card `id` shows settled: its entrance, sent or not, is dropped.
    fn settle(&mut self, out: &mut Vec<u8>, id: &str) {
        if !self.shown.insert(id.to_string()) {
            return;
        }
        let queued = self.outbox.len();
        self.outbox.retain(|(i, _)| i != id);
        let dropped = queued - self.outbox.len();
        if let Some(e) = self.sending.remove(id).or_else(|| {
            self.entrances
                .get(id)
                .filter(|e| e.state == Play::Ready)
                .copied()
        }) {
            self.entrances.remove(id);
            out.extend(kitty::delete(e.image));
            self.note(|| format!("{id} shown settled; entrance dropped, {dropped} frames unsent"));
        }
    }

    /// Starts card `id`'s entrance the first time the card shows: on its
    /// first visible frame ([`one_frame_in`]), its clock running now or once
    /// the harness's synchronized update ends, since kitty shows nothing new
    /// until then.
    fn start(&mut self, out: &mut Vec<u8>, id: &str) {
        let (now, sync) = (self.now, self.tracker.harness_sync);
        let Some(e) = self.entrances.get_mut(id) else {
            return;
        };
        if e.state != Play::Ready {
            return;
        }
        let first = e.frame_at(Duration::from_millis(u64::from(e.frame_ms)));
        out.extend(kitty::show_frame(e.image, first));
        e.shown = first;
        e.state = if sync {
            Play::Waiting(now)
        } else {
            Play::Playing(one_frame_in(now, e.frame_ms))
        };
        self.note(|| format!("{id} entrance cued, harness sync {sync}"));
    }

    fn playing(&self, id: &str) -> bool {
        self.entrances
            .get(id)
            .is_some_and(|e| matches!(e.state, Play::Waiting(_) | Play::Playing(_)))
    }

    /// Starts the clocks of entrances waiting on the harness, shows each
    /// playing entrance's frame for the time passed, stops those over on
    /// the settled card, and hands each card placed early over to its block
    /// once the block is whole and the entrance has played; handing over
    /// deletes the entrance's image, so kitty keeps one picture per card.
    fn step(&mut self, out: &mut Vec<u8>) {
        let (now, sync) = (self.now, self.tracker.harness_sync);
        let mut ids: Vec<&String> = self.entrances.keys().collect();
        ids.sort();
        let ids: Vec<String> = ids.into_iter().cloned().collect();
        for id in &ids {
            let Some(e) = self.entrances.get_mut(id) else {
                continue;
            };
            let image = e.image;
            let event = match e.state {
                Play::Waiting(since) if !sync || now.saturating_sub(since) >= SYNC_WAIT => {
                    e.state = Play::Playing(one_frame_in(now, e.frame_ms));
                    Some("entrance runs".to_string())
                }
                Play::Playing(since) if now.saturating_sub(since) >= e.length() => {
                    out.extend(kitty::show_frame(image, e.last));
                    e.state = Play::Done;
                    Some("entrance stopped on the settled card".to_string())
                }
                Play::Playing(since) => {
                    let frame = e.frame_at(now.saturating_sub(since));
                    (frame != e.shown).then(|| {
                        out.extend(kitty::show_frame(image, frame));
                        e.shown = frame;
                        format!("entrance frame {frame}")
                    })
                }
                _ => None,
            };
            if let Some(event) = event {
                self.note(|| format!("{id} {event}"));
            }
        }
        let over: Vec<String> = self
            .early
            .keys()
            .filter(|id| self.whole.contains(*id) && !self.playing(id))
            .cloned()
            .collect();
        for id in over {
            self.unplace_early(out, &id);
        }
    }

    /// Places the whole of card `id` over the rows its block will take,
    /// from screen row `top` (above the screen when negative) at column
    /// `col`, the block typed down to row `last`. Under it goes a backing
    /// in the conversation's colour, the card's exact size: its corners
    /// and margins are transparent, and the harness's line under the typed
    /// row (`OpenCode`'s "Build" line) showed through, stepping down a row
    /// at a time. It stops at the harness's own footer: the first row,
    /// from the third under the typed row, with a box border or a
    /// background of its own.
    fn place_early(
        &mut self,
        out: &mut Vec<u8>,
        id: &str,
        card: &CardInfo,
        top: i32,
        col: u16,
        last: u16,
    ) {
        let first = u16::try_from(top.max(0)).unwrap_or(u16::MAX);
        let typed = (first..=last).map(|y| {
            let row = i32::from(y) - top + 1;
            (y, u16::try_from(row).unwrap_or(0))
        });
        let width = card.cols.min(self.free_cols(col, typed));
        let screen = self.parser.screen();
        let (rows, _) = screen.size();
        if width == 0 || card.rows == 0 || first >= rows {
            return;
        }
        let bg = beside(screen, first, col, usize::from(card.cols));
        let end = i32::from(rows).min(top + i32::from(card.rows));
        let bottom = footer(screen, (col, width), bg, i32::from(last) + 3, end);
        let covered = u16::try_from(bottom - i32::from(first)).unwrap_or(0);
        if covered == 0 {
            return;
        }
        self.upload(out, id, card);
        let before = self.early.get(id).copied();
        // The entrance when it is in the terminal, else the settled card,
        // which then stays: an entrance arriving later is dropped.
        let (image, (px_w, px_h)) = match (before, self.entrances.get(id)) {
            (Some(e), Some(a)) if e.image == a.image => (a.image, a.size_px),
            (None, Some(a)) if a.state == Play::Ready => (a.image, a.size_px),
            _ => (card_image_id(id), card.size_px),
        };
        if image == card_image_id(id) {
            self.settle(out, id);
        }
        let per_row = f64::from(px_h) / f64::from(card.rows);
        let px = |v: f64| u32::try_from(whole_px(v)).unwrap_or(0);
        let crop_y = px(f64::from(i32::from(first) - top) * per_row).min(px_h);
        let crop_h = px(f64::from(bottom - top) * per_row)
            .min(px_h)
            .saturating_sub(crop_y);
        let crop_w = px(f64::from(width) * f64::from(px_w) / f64::from(card.cols)).min(px_w);
        let color = rgb(bg, self.default_bg);
        let at = Spot {
            row: first,
            col,
            rows: covered,
            crop_y,
            crop_h,
            color,
        };
        if image != card_image_id(id) {
            self.start(out, id);
        }
        if before.is_some_and(|e| e.at == at) {
            return;
        }
        if before.is_none() {
            let what = if image == card_image_id(id) {
                "settled"
            } else {
                "with its entrance"
            };
            self.note(|| {
                format!(
                    "{id} placed early {what} at row {first}, {covered} rows by {width} columns"
                )
            });
        }
        let mask = before.map_or_else(|| self.take_image_id(), |e| e.mask);
        let backing = match color {
            Some(c) if before.map(|e| e.at.color) != Some(color) => self
                .pictures
                .flat_png(px_w, px_h, c)
                .map(|png| kitty::transmit_png(mask, &png)),
            _ => None,
        };
        let crop = kitty::Crop {
            x: 0,
            y: crop_y,
            w: crop_w,
            h: crop_h,
        };
        if let Some(bytes) = backing {
            out.extend(bytes);
        } else if color.is_none() && before.is_some() {
            out.extend(kitty::delete(mask));
        }
        cup(out, first, col);
        if color.is_some() {
            out.extend(kitty::place(mask, 1, width, covered, 1, Some(crop)));
        }
        out.extend(kitty::place(
            image,
            EARLY_PLACEMENT,
            width,
            covered,
            2,
            Some(crop),
        ));
        self.early.insert(id.to_string(), Early { at, mask, image });
    }

    /// Removes card `id`'s early placement and its backing, and the
    /// entrance's image once it has played.
    fn unplace_early(&mut self, out: &mut Vec<u8>, id: &str) {
        if let Some(early) = self.early.remove(id) {
            self.note(|| format!("{id} early placement removed"));
            out.extend(kitty::delete_placement(early.image, EARLY_PLACEMENT));
            out.extend(kitty::delete(early.mask));
            if early.image != card_image_id(id) {
                out.extend(kitty::delete(early.image));
                self.entrances.remove(id);
            }
        }
    }

    /// Card `id`'s rows `copy.lo` to `copy.hi` as a picture of its own for
    /// the pre-cut and sixel routes, its left `width` columns, and the
    /// image id it goes under. A pre-cut band is encoded once per set of
    /// rows and sent again as it is while the card only moves.
    fn picture(
        &mut self,
        id: &str,
        card: &CardInfo,
        copy: Copy,
        width: u16,
    ) -> Option<(u32, Vec<u8>)> {
        let count = copy.hi - copy.lo + 1;
        let image = self.take_image_id();
        let (px_w, px_h) = card.size_px;
        let cut_w = u32::try_from(whole_px(
            f64::from(width) * f64::from(px_w) / f64::from(card.cols.max(1)),
        ))
        .unwrap_or(px_w)
        .min(px_w);
        let band_h = u32::try_from(whole_px(
            f64::from(count) * f64::from(px_h) / f64::from(card.rows.max(1)),
        ))
        .unwrap_or(px_h);
        let cut = (width < card.cols).then_some(kitty::Crop {
            x: 0,
            y: 0,
            w: cut_w,
            h: band_h,
        });
        let bytes = if self.route == Route::Sixel {
            self.pictures
                .band(id, copy.lo - 1, count)
                .map(|b| sixel::encode(&left_part(b, width, card.cols)))
        } else if copy.lo == 1 && copy.hi == card.rows {
            let z = self.take_z();
            Some(kitty::show_png_over(image, &card.png, width, count, cut, z))
        } else {
            let rows = (copy.lo, copy.hi);
            let png = match self.bands.get(id) {
                Some((at, png)) if *at == rows => Rc::clone(png),
                _ => {
                    let png = Rc::new(self.pictures.band_png(id, copy.lo - 1, count)?);
                    self.bands.insert(id.to_string(), (rows, Rc::clone(&png)));
                    png
                }
            };
            let z = self.take_z();
            Some(kitty::show_png_over(image, &png, width, count, cut, z))
        };
        bytes.map(|b| (image, b))
    }

    /// [`Painter::draw`] on the placeholder route: the card's placeholder
    /// cells over the copy's rows, `width` columns of them.
    #[allow(clippy::too_many_arguments)] // The draw's own state, passed on whole.
    fn draw_placeholders(
        &mut self,
        out: &mut Vec<u8>,
        id: &str,
        copy: Copy,
        card: &CardInfo,
        sigs: &[u64],
        width: u16,
        moved: bool,
    ) {
        let visible = copy.hi - copy.lo + 1;
        if !self.placed.contains_key(id) {
            self.note(|| format!("{id} drawn in its block at row {}", copy.y));
        }
        self.upload(out, id, card);
        let ready = self
            .entrances
            .get(id)
            .is_some_and(|e| e.state == Play::Ready);
        if !self.early.contains_key(id) && (!self.motion || !ready || copy.lo != 1) {
            self.settle(out, id);
        }
        // Cells past `width` are the harness's own: a card cut there
        // shows its left part rather than drawing over a panel.
        let lines = kitty::placeholder_rows(card_image_id(id), width, card.rows);
        for i in 0..visible {
            let y = copy.y + i;
            if self.painted.get(&y) == Some(&sigs[usize::from(y)]) && !moved {
                continue;
            }
            cup(out, y, copy.col);
            out.extend(self.surround(y, copy.col, usize::from(card.cols)));
            out.extend_from_slice(lines[usize::from(copy.lo + i - 1)].as_bytes());
            self.painted.insert(y, sigs[usize::from(y)]);
        }
        // Remembered so an unchanged card is not repainted; placeholder
        // images are never deleted, since scrollback still shows them.
        if let Some((old, _)) = self
            .placed
            .insert(id.to_string(), (copy, card_image_id(id)))
            && old != copy
        {
            let now = copy.y..copy.y + visible;
            self.restore(out, old, card.cols, |y| now.contains(&y));
        }
    }

    /// Pre-cut, per tick: queues the entrances of new cards, sends queued
    /// frames, and while an entrance plays, a moved card waits or a block
    /// has just stalled paints the screen again, so the frame for the time
    /// passed, the card in its new place, or the card cut to its typed rows
    /// goes up.
    fn tick_precut(&mut self, out: &mut Vec<u8>) {
        self.bury(out);
        for id in std::mem::take(&mut self.preload) {
            if let Some(card) = self.pictures.card(&id) {
                self.send_reel(&id, &card);
            }
        }
        // Cards drawn again under their id change only in kitty (ADR-021).
        let _ = self.pictures.redrawn();
        self.flush(out);
        let now = self.now;
        let gone: Vec<String> = self
            .reels
            .iter()
            .filter(|(id, r)| r.over(now) && !self.strips.contains_key(*id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            self.retire_reel(out, &id);
        }
        let playing = self.reels.values().any(|r| r.since.is_some());
        if (playing || self.deferred || self.stalled()) && !self.tracker.harness_sync {
            self.deferred = false;
            out.extend(self.paint_now(&[]));
        }
    }

    /// Pre-cut: gives card `id`'s entrance frames from the first visible
    /// one image ids of their own and queues them for [`Painter::flush`],
    /// once, unless the card has already shown settled. Sending each frame
    /// as it is placed, they count as in the terminal now and none is
    /// placed from ahead.
    fn send_reel(&mut self, id: &str, card: &CardInfo) {
        if card.frames < 3 || self.shown.contains(id) || self.reels.contains_key(id) {
            return;
        }
        let n = usize::from(card.frames) - 2;
        let images: Vec<u32> = (0..n).map(|_| self.take_image_id()).collect();
        let sent = if self.ahead {
            let frames = self.pictures.frames(id);
            if frames.len() != usize::from(card.frames) {
                return;
            }
            for (image, png) in images.iter().zip(&frames[1..=n]) {
                self.outbox
                    .push_back((id.to_string(), kitty::transmit_png(*image, png)));
            }
            0
        } else {
            n
        };
        self.reels.insert(
            id.to_string(),
            Reel {
                images,
                sent,
                used: vec![!self.ahead; n],
                reached: 0,
                frame_ms: u32::from(card.frame_ms.max(1)),
                since: None,
            },
        );
        self.note(|| format!("{id} entrance of {n} frames queued"));
    }

    /// Pre-cut: the entrance frame card `id` shows now, `None` for the
    /// crisp card. The card's first showing starts its entrance's clock,
    /// or, when its frames have not all reached the terminal, drops the
    /// entrance; a card with no entrance then never gets one.
    fn reel_frame(&mut self, id: &str) -> Option<u16> {
        let now = self.now;
        self.shown.insert(id.to_string());
        let r = self.reels.get_mut(id)?;
        if let Some(since) = r.since {
            return r.frame_at(now.saturating_sub(since));
        }
        if r.sent < r.images.len() {
            let why = format!("{} of {} frames in the terminal", r.sent, r.images.len());
            self.drop_reel(id, &why);
            return None;
        }
        let since = one_frame_in(now, r.frame_ms);
        r.since = Some(since);
        let frame = r.frame_at(now.saturating_sub(since));
        self.note(|| format!("{id} entrance starts"));
        frame
    }

    /// Pre-cut: drops card `id`'s entrance, for the reason `why` the trace
    /// gives: its unsent frames, and those in the terminal, a tick from
    /// now, since one may be on screen until the crisp card replaces it.
    fn drop_reel(&mut self, id: &str, why: &str) {
        let queued = self.outbox.len();
        self.outbox.retain(|(i, _)| i != id);
        let unsent = queued - self.outbox.len();
        if let Some(r) = self.reels.remove(id) {
            for image in &r.images[..r.sent] {
                self.retire(*image);
            }
        }
        self.note(|| {
            format!("{id} shown settled; entrance dropped: {why}, {unsent} frames unsent")
        });
    }

    /// Pre-cut: forgets card `id`'s entrance once it is over, deleting
    /// every frame in the terminal; those placed went with their strips.
    fn retire_reel(&mut self, out: &mut Vec<u8>, id: &str) {
        if let Some(r) = self.reels.remove(id) {
            if self.ahead {
                for (image, used) in r.images[..r.sent].iter().zip(&r.used) {
                    if !used {
                        out.extend(kitty::delete(*image));
                    }
                }
            }
            self.note(|| format!("{id} entrance over"));
        }
    }

    /// Pre-cut: the picture of card rows `piece.lo` to `piece.hi` showing
    /// entrance frame `frame`, `width` columns: the frame it shows, its
    /// image id and its bytes. The frame sent ahead is placed when it is
    /// the whole card and has not been placed before; else the frame's rows
    /// are sent now. A whole card's frame already placed whose rows are too
    /// large to send in a tick, as a moved dashboard's are, gives way to
    /// the next frame sent ahead and never placed, ahead of its turn, so
    /// the entrance goes on. Else why the frame cannot be shown.
    fn frame_picture(
        &mut self,
        id: &str,
        card: &CardInfo,
        piece: Copy,
        width: u16,
        frame: u16,
    ) -> Result<(u16, u32, Vec<u8>), String> {
        let count = piece.hi - piece.lo + 1;
        let whole = piece.lo == 1 && piece.hi == card.rows;
        let missing = || format!("frame {frame} cannot be had");
        let i = usize::from(frame).checked_sub(1).ok_or_else(missing)?;
        // The first frame from `from` sent ahead and never placed.
        let free = |r: &Reel, from: usize| (from..r.sent.min(r.used.len())).find(|j| !r.used[*j]);
        if whole && self.reels.get(id).and_then(|r| free(r, i)) == Some(i) {
            return self.place_frame(id, i, width, count).ok_or_else(missing);
        }
        let png = self
            .pictures
            .frame_band_png(id, frame, piece.lo - 1, count)
            .ok_or_else(missing)?;
        if png.len() <= SEND_BUDGET {
            let image = self.take_image_id();
            let z = self.take_z();
            return Ok((
                frame,
                image,
                kitty::show_png_over(image, &png, width, count, None, z),
            ));
        }
        let next = self
            .reels
            .get(id)
            .filter(|_| whole)
            .and_then(|r| free(r, i + 1));
        match next.and_then(|j| self.place_frame(id, j, width, count)) {
            Some(placed) => Ok(placed),
            None => Err(format!("frame {frame} is {} bytes", png.len())),
        }
    }

    /// Pre-cut: places card `id`'s entrance frame `i` (from 0), sent ahead,
    /// over `count` rows of `width` columns, and records it placed: the
    /// frame it shows, from 1, its image id and its bytes.
    fn place_frame(
        &mut self,
        id: &str,
        i: usize,
        width: u16,
        count: u16,
    ) -> Option<(u16, u32, Vec<u8>)> {
        let frame = u16::try_from(i + 1).ok()?;
        let z = self.take_z();
        let r = self.reels.get_mut(id)?;
        let image = *r.images.get(i)?;
        r.used[i] = true;
        r.reached = r.reached.max(frame);
        Some((frame, image, kitty::place(image, 1, width, count, z, None)))
    }

    /// Blanks screen rows `ys` from column `col`, `width` columns, on the
    /// background around a card `cols` wide.
    fn blank_rows(
        &mut self,
        out: &mut Vec<u8>,
        ys: std::ops::Range<u16>,
        col: u16,
        cols: u16,
        width: u16,
        sigs: &[u64],
    ) {
        for y in ys {
            cup(out, y, col);
            out.extend(self.surround(y, col, usize::from(cols)));
            out.extend(std::iter::repeat_n(b' ', usize::from(width)));
            self.painted.insert(y, sigs[usize::from(y)]);
        }
    }

    /// [`Painter::draw`] on the pre-cut route: the copy's rows as strips.
    /// A block still being typed reaches the rows it will take on screen
    /// ([`Painter::growing`]), so its first row places the whole card and
    /// the rows typed under it place nothing; a copy that reaches further
    /// than the strips in place gets one more under them. When the card
    /// moved, lost rows off the top, changed width, moved on to its
    /// entrance's next frame, or the terminal changed size, one strip of
    /// its visible rows goes up under a new image id over the old ones,
    /// which go a tick later, since Warp keeps old copies of a moved
    /// picture and shows nothing until it has decoded a new one. A whole
    /// card in several strips goes up again as one. A card still being
    /// typed is placed again for a move at most once a [`TICK`]: a second
    /// move within the tick blanks its new rows, keeps its strips where
    /// they are and waits for the tick's paint; what a move sends counts
    /// against the frames' [`SEND_BUDGET`].
    fn draw_precut(
        &mut self,
        out: &mut Vec<u8>,
        id: &str,
        copy: Copy,
        card: &CardInfo,
        sigs: &[u64],
        width: u16,
    ) {
        let mut frame = if self.precut && width == card.cols {
            self.reel_frame(id)
        } else {
            if self.reels.contains_key(id) {
                let why = format!("cut to {width} of {} columns", card.cols);
                self.drop_reel(id, &why);
            }
            self.shown.insert(id.to_string());
            None
        };
        let top = i32::from(copy.y) - i32::from(copy.lo) + 1;
        let old = self.strips.remove(id).unwrap_or_default();
        // A whole card in several strips, or about to gain one, goes up
        // again as one picture once its entrance is over: no seams, no
        // band drawn on rows of another height.
        let grows = old.last().is_some_and(|s| copy.hi > s.hi);
        let merge = (old.len() > 1 || grows) && frame.is_none() && self.whole.contains(id);
        let stays = !old.is_empty()
            && !merge
            && !self.resized.contains(id)
            && old.first().is_some_and(|s| s.lo == copy.lo)
            && old.iter().all(|s| {
                s.top() == top
                    && s.col == copy.col
                    && s.width == width
                    && s.hi <= copy.hi
                    && s.frame == frame
            });
        let moved = !old.is_empty() && !stays && old.iter().all(|s| s.frame == frame);
        let paced = moved && !self.whole.contains(id);
        let tick = self.now.as_millis() / TICK.as_millis();
        let waits = paced && self.replaced.get(id) == Some(&tick);
        let from = match old.last() {
            Some(last) if stays => last.hi + 1,
            _ if waits => copy.hi + 1,
            _ => copy.lo,
        };
        // Typed rows under a strip already in place whose text the harness
        // rewrote are blanked again; the strip stands. Rows not typed yet
        // were blanked with the tokens ([`Painter::blank_tokens`]).
        let typed = copy.typed_rows();
        for y in copy.y..copy.y + (from - copy.lo) {
            if typed.contains(&y) && self.painted.get(&y) != Some(&sigs[usize::from(y)]) {
                self.blank_rows(out, y..y + 1, copy.col, card.cols, width, sigs);
            }
        }
        if from > copy.hi {
            self.deferred |= waits;
            self.keep_strips(id, old);
            return;
        }
        if paced {
            self.replaced.insert(id.to_string(), tick);
        }
        let piece = Copy {
            y: copy.y + (from - copy.lo),
            lo: from,
            ..copy
        };
        let rows = piece.y.max(typed.start)..(piece.y + (piece.hi - piece.lo + 1)).min(typed.end);
        self.blank_rows(out, rows, copy.col, card.cols, width, sigs);
        let picture = if let Some(f) = frame {
            match self.frame_picture(id, card, piece, width, f) {
                Ok((shown, image, bytes)) => {
                    frame = Some(shown);
                    Some((image, bytes))
                }
                Err(why) => {
                    // A frame that cannot be sent in a tick, with none
                    // free to take its place: the crisp card, and no
                    // entrance.
                    self.drop_reel(id, &why);
                    self.keep_strips(id, old);
                    return self.draw_precut(out, id, copy, card, sigs, width);
                }
            }
        } else {
            self.picture(id, card, piece, width)
        };
        let Some((image, bytes)) = picture else {
            self.keep_strips(id, old);
            return;
        };
        if paced {
            self.spent += bytes.len();
        }
        cup(out, piece.y, piece.col);
        out.extend(bytes);
        let strip = Strip {
            y: piece.y,
            col: piece.col,
            lo: piece.lo,
            hi: piece.hi,
            width,
            image,
            frame,
        };
        let kept = if stays { old } else { self.replace(old) };
        self.resized.remove(id);
        self.put_strip(out, id, strip, kept);
    }

    /// Pre-cut: retires `old`, the strips a new picture has just gone up
    /// over, and keeps none of them: Warp showed no card for up to 100 ms
    /// when the old ones were deleted in the update that placed the new
    /// one, so they go a tick later ([`Painter::retire`]).
    fn replace(&mut self, old: Vec<Strip>) -> Vec<Strip> {
        for gone in old {
            self.retire(gone.image);
        }
        Vec::new()
    }

    /// Pre-cut: records `strip`, just placed, under the strips `kept` for
    /// card `id`, and forgets the card's entrance once it is over.
    fn put_strip(&mut self, out: &mut Vec<u8>, id: &str, strip: Strip, kept: Vec<Strip>) {
        let frame = strip.frame;
        self.note(|| {
            let what = frame.map_or_else(|| "settled".to_string(), |f| format!("frame {f}"));
            format!(
                "{id} rows {} to {} placed {what} at screen row {}",
                strip.lo, strip.hi, strip.y
            )
        });
        self.strips
            .insert(id.to_string(), [kept, vec![strip]].concat());
        if frame.is_none() && self.reels.get(id).is_some_and(|r| r.over(self.now)) {
            self.retire_reel(out, id);
        }
    }

    /// Pre-cut: keeps card `id`'s strips `old` on record, none when empty.
    fn keep_strips(&mut self, id: &str, old: Vec<Strip>) {
        if !old.is_empty() {
            self.strips.insert(id.to_string(), old);
        }
    }

    fn draw(
        &mut self,
        out: &mut Vec<u8>,
        id: &str,
        copy: Copy,
        card: &CardInfo,
        sigs: &[u64],
        rows: u16,
    ) {
        if copy.hi > card.rows {
            return;
        }
        let visible = copy.hi - copy.lo + 1;
        // Rows not typed yet hold the harness's own lines, which the card
        // covers, so only the typed rows bound its width.
        let width = card.cols.min(self.free_cols(
            copy.col,
            copy.typed_rows().map(|y| (y, copy.lo + (y - copy.y))),
        ));
        if width == 0 {
            self.note(|| format!("{id} not drawn: no free column at {}", copy.col));
            return;
        }
        let touched =
            (copy.y..copy.y + visible).any(|y| self.painted.get(&y) != Some(&sigs[usize::from(y)]));
        let moved = self.placed.get(id).is_none_or(|(c, _)| *c != copy);
        match self.route {
            Route::KittyPlaceholders => {
                self.draw_placeholders(out, id, copy, card, sigs, width, moved);
            }
            Route::KittyPrecut => self.draw_precut(out, id, copy, card, sigs, width),
            Route::Sixel => {
                if !moved && !touched {
                    return;
                }
                let mut copy = copy;
                // A sixel reaching the last row would scroll the screen.
                let room = rows.saturating_sub(copy.y + 1);
                copy.hi = copy.hi.min(copy.lo + room.saturating_sub(1));
                if copy.hi < copy.lo {
                    return;
                }
                let count = copy.hi - copy.lo + 1;
                self.blank_rows(
                    out,
                    copy.y..copy.y + count,
                    copy.col,
                    card.cols,
                    width,
                    sigs,
                );
                let Some((image, bytes)) = self.picture(id, card, copy, width) else {
                    return;
                };
                cup(out, copy.y, copy.col);
                out.extend(bytes);
                self.placed.insert(id.to_string(), (copy, image));
            }
            Route::Text => {}
        }
    }
}

#[cfg(test)]
mod tests;
