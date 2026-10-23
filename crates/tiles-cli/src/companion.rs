//! The companion window (ADR-046): on the text route on macOS, where the
//! terminal draws no pictures, `tiles run` shows each card its session
//! draws in one window beside the terminal. The window opens with the
//! session's first card and takes each later one in place, never taking
//! focus from the terminal. Closing it leaves the harness running, and the
//! next card opens it again; it closes when `tiles run` ends.
//!
//! A thread watches the card cache as the wrapper does
//! ([`crate::wrap::pictures`]): a card shows only when its geometry carries
//! this session's token, so another session's cards and those `tiles open`
//! draws in a plain shell never reach the window. The card it shows goes to
//! the window through a file of three lines under the session's add-ons,
//! which `window.js` reads every 250 ms.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use tiles_term::detect::Route;

/// How often the cache is looked at.
const POLL: Duration = Duration::from_millis(250);

/// Largest geometry file read.
const MAX_META_BYTES: u64 = 64 * 1024;

/// Whether `tiles run` keeps a companion window: on macOS, on the text
/// route, unless `TILES_WINDOW` turns windows off.
pub fn wanted(route: Route, env: &dyn Fn(&str) -> Option<String>) -> bool {
    cfg!(target_os = "macos") && route == Route::Text && crate::window::on(env)
}

/// The terminals that draw cards inline, for [`text_route_line`].
const INLINE: &str = "Warp, Ghostty, kitty, iTerm2 and WezTerm";

/// The line `tiles setup` and `tiles doctor` print on the text route: cards
/// will not draw inline in this terminal, which terminals draw them, and
/// how they open here. `None` on a route that draws pictures.
#[must_use]
pub fn text_route_line(
    detected: &tiles_term::detect::Detected,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    if detected.route != Route::Text {
        return None;
    }
    let here = if wanted(detected.route, env) {
        "tiles run shows each session's cards in one window beside this terminal \
         (TILES_WINDOW=off keeps it closed)"
    } else if cfg!(target_os = "macos") {
        "TILES_WINDOW is off, so each reply gives the card's PNG path and tiles open <id> shows it"
    } else {
        "each reply gives the card's PNG path"
    };
    Some(format!(
        "pictures: none in this terminal, so cards will not draw inline here; \
         {INLINE} draw them inline. Here {here}."
    ))
}

/// What the window needs of a card's geometry file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// The session token it names, if any.
    pub session: Option<u64>,
    /// The card's title.
    pub title: String,
    /// Whether it was drawn in a light theme.
    pub light: bool,
}

impl Meta {
    /// The fields of a geometry file's JSON; `None` when it is not one.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let meta: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let text = |k: &str| meta.get(k).and_then(serde_json::Value::as_str);
        Some(Self {
            session: text("session")
                .filter(|s| (1..=16).contains(&s.len()))
                .and_then(|s| u64::from_str_radix(s, 16).ok()),
            title: text("title").unwrap_or_default().to_string(),
            light: text("theme")
                .and_then(tiles_engine::Theme::from_name)
                .is_some_and(tiles_engine::Theme::is_light),
        })
    }
}

/// The session's cards as they land in the cache.
pub struct Watch {
    session: u64,
    since: SystemTime,
    /// Each card looked at, with when its geometry was written.
    seen: HashMap<String, SystemTime>,
}

impl Watch {
    /// A watch for `session`'s cards written from `since` on.
    #[must_use]
    pub fn new(session: u64, since: SystemTime) -> Self {
        Self {
            session,
            since,
            seen: HashMap::new(),
        }
    }

    /// This session's cards among `listed` (each card id with when its
    /// geometry was written) that are new or written again since the last
    /// call, oldest first, with their geometry from `meta`. A card whose
    /// geometry cannot be read yet is looked at again next time.
    pub fn fresh(
        &mut self,
        listed: &[(String, SystemTime)],
        meta: &dyn Fn(&str) -> Option<Meta>,
    ) -> Vec<(String, Meta)> {
        let mut out: Vec<(SystemTime, String, Meta)> = Vec::new();
        for (id, written) in listed {
            if *written < self.since || self.seen.get(id) == Some(written) {
                continue;
            }
            let Some(m) = meta(id) else {
                continue;
            };
            self.seen.insert(id.clone(), *written);
            if m.session == Some(self.session) {
                out.push((*written, id.clone(), m));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        out.into_iter().map(|(_, id, m)| (id, m)).collect()
    }
}

/// Each card's geometry file under `cards`, with when it was written.
fn list(cards: &Path) -> Vec<(String, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(cards) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|x| x != "json") {
                return None;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| tiles_term::block::is_card_id(s))?;
            let written = entry.metadata().and_then(|m| m.modified()).ok()?;
            Some((id.to_string(), written))
        })
        .collect()
}

fn read_meta(cards: &Path, id: &str) -> Option<Meta> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(cards.join(format!("{id}.json")))
        .ok()?
        .take(MAX_META_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Meta::parse(&bytes)
}

/// The three lines `window.js` reads: the PNG, the title and the look. The
/// title passes the sanitizer, which also keeps it to one line.
#[must_use]
pub fn window_file(png: &Path, meta: &Meta) -> String {
    format!(
        "{}\n{}\n{}\n",
        png.display(),
        tiles_term::sanitize::line(&meta.title, 200),
        if meta.light { "light" } else { "dark" }
    )
}

/// Writes `text` to `file` whole: a file beside it, then a rename, so the
/// window never reads half of it.
fn write_whole(file: &Path, text: &str) -> std::io::Result<()> {
    let part = file.with_extension("part");
    let mut f = std::fs::File::create(&part)?;
    f.write_all(text.as_bytes())?;
    drop(f);
    std::fs::rename(&part, file)
}

/// A line for the `TILES_TRACE` file, when it is set.
fn trace(line: &str) {
    if let Some(path) = std::env::var_os("TILES_TRACE")
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(f, "window: {line}");
    }
}

/// The window process and the file it follows.
struct Window {
    file: PathBuf,
    bundle: String,
    child: Option<Child>,
}

impl Window {
    /// Shows card `id` from `png`: in place when the window is up, else in
    /// a new one.
    fn show(&mut self, id: &str, png: &Path, meta: &Meta) {
        if write_whole(&self.file, &window_file(png, meta)).is_err() {
            return;
        }
        if let Some(child) = &mut self.child
            && matches!(child.try_wait(), Ok(None))
        {
            trace(&format!("swapped in {id}"));
            return;
        }
        if self.child.take().is_some() {
            trace("closed by the user");
        }
        let mut cmd = crate::window::follow_command(&self.file, &self.bundle);
        // Its own process group, so no signal the terminal sends the
        // harness reaches it; `tiles run` ends it on the way out, and it
        // ends itself when `tiles run` is gone.
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(child) => {
                trace(&format!("opened with {id}"));
                self.child = Some(child);
            }
            Err(e) => trace(&format!("cannot start osascript: {e}")),
        }
    }

    fn close(&mut self) {
        if let Some(mut child) = self.child.take() {
            if matches!(child.try_wait(), Ok(None)) {
                trace("closed as tiles run ended");
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.file);
    }
}

/// The running companion window; dropping it closes the window.
pub struct Companion {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Companion {
    /// Watches `cache` for `session`'s cards and shows them, through a file
    /// under `dir`, beside the front window of the app `bundle` names.
    #[must_use]
    pub fn start(cache: &Path, dir: &Path, session: u64, bundle: String) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let cards = cache.join("cards");
        let file = dir.join("window");
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut watch = Watch::new(session, SystemTime::now());
            let mut window = Window {
                file,
                bundle,
                child: None,
            };
            let mut listed_at = None;
            while !stopping.load(Ordering::Relaxed) {
                // Listed again only when the directory changed: a card's
                // geometry is written last, after its picture.
                let changed = std::fs::metadata(&cards).and_then(|m| m.modified()).ok();
                if changed.is_some() && changed != listed_at {
                    listed_at = changed;
                    let fresh = watch.fresh(&list(&cards), &|id| read_meta(&cards, id));
                    if let Some((id, meta)) = fresh.last() {
                        window.show(id, &cards.join(format!("{id}.png")), meta);
                    }
                }
                std::thread::sleep(POLL);
            }
            window.close();
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Companion {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use tiles_term::detect::Route;

    use super::{Meta, Watch, wanted, window_file};

    fn meta(session: Option<u64>) -> Meta {
        Meta {
            session,
            title: "Signups".into(),
            light: false,
        }
    }

    #[test]
    fn only_this_sessions_cards_show_oldest_first_and_each_once() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut watch = Watch::new(7, t0);
        let sessions = |id: &str| match id {
            "aaaaaa" | "cccccc" | "eeeeee" => Some(meta(Some(7))),
            "bbbbbb" => Some(meta(Some(8))),
            "dddddd" => Some(meta(None)),
            _ => None,
        };
        // Listed out of order: the window takes them by when each was
        // written, not by id or by listing.
        let listed = vec![
            ("cccccc".to_string(), at(3)),
            ("bbbbbb".to_string(), at(2)),
            ("aaaaaa".to_string(), at(1)),
            ("dddddd".to_string(), at(4)),
            ("ffffff".to_string(), at(5)),
            ("999999".to_string(), at(0) - Duration::from_secs(1)),
        ];
        let ids = |v: Vec<(String, Meta)>| v.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        assert_eq!(
            ids(watch.fresh(&listed, &sessions)),
            ["aaaaaa", "cccccc"],
            "another session's card, one made outside any session, one not yet readable \
             and one from before the session never show"
        );
        assert!(
            watch.fresh(&listed, &sessions).is_empty(),
            "each shows once"
        );
        let mut more = listed.clone();
        more.push(("eeeeee".to_string(), at(6)));
        more[0].1 = at(7);
        assert_eq!(
            ids(watch.fresh(&more, &sessions)),
            ["eeeeee", "cccccc"],
            "a new card, then one drawn again under its id"
        );
    }

    #[test]
    fn a_card_written_late_is_picked_up_once_readable() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let mut watch = Watch::new(7, t0);
        let listed = vec![("aaaaaa".to_string(), t0)];
        assert_eq!(watch.fresh(&listed, &|_| None), Vec::new());
        assert_eq!(watch.fresh(&listed, &|_| Some(meta(Some(7)))).len(), 1);
    }

    #[test]
    fn the_geometry_gives_the_session_title_and_theme() {
        let m = Meta::parse(
            br#"{"cols": 4, "rows": 1, "title": "Rev", "session": "0000000000000007", "theme": "light"}"#,
        )
        .unwrap();
        assert_eq!(
            m,
            Meta {
                session: Some(7),
                title: "Rev".into(),
                light: true
            }
        );
        let m = Meta::parse(br#"{"session": null, "theme": "obsidian"}"#).unwrap();
        assert_eq!((m.session, m.light), (None, false));
        assert_eq!(Meta::parse(br#"{"session": "zz"}"#).unwrap().session, None);
        assert!(Meta::parse(b"not json").is_none());
    }

    #[test]
    fn the_window_file_is_three_lines_and_the_title_cannot_break_them() {
        let m = Meta {
            session: Some(7),
            title: "Rev\nfake/path.png\u{1b}]0;x\u{7}".into(),
            light: true,
        };
        let text = window_file(Path::new("/c/cards/aaaaaa.png"), &m);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text:?}");
        assert_eq!(lines[0], "/c/cards/aaaaaa.png");
        assert!(!lines[1].contains('\u{1b}'));
        assert_eq!(lines[2], "light");
    }

    #[test]
    fn the_text_route_says_cards_will_not_draw_inline_and_where_they_open() {
        use tiles_term::detect::{Detected, Terminal};
        let d = |terminal, route| Detected {
            terminal,
            multiplexer: None,
            route,
            reason: String::new(),
        };
        let none = |_: &str| None;
        let line = super::text_route_line(&d(Terminal::AppleTerminal, Route::Text), &none).unwrap();
        assert!(line.starts_with("pictures: none in this terminal, so cards will not draw inline here; Warp, Ghostty, kitty, iTerm2 and WezTerm draw them inline. Here "), "{line}");
        if cfg!(target_os = "macos") {
            assert!(line.ends_with("Here tiles run shows each session's cards in one window beside this terminal (TILES_WINDOW=off keeps it closed)."), "{line}");
            let off = super::text_route_line(&d(Terminal::AppleTerminal, Route::Text), &|k| {
                (k == "TILES_WINDOW").then(|| "off".into())
            })
            .unwrap();
            assert!(off.ends_with("Here TILES_WINDOW is off, so each reply gives the card's PNG path and tiles open <id> shows it."), "{off}");
        } else {
            assert!(
                line.ends_with("Here each reply gives the card's PNG path."),
                "{line}"
            );
        }
        assert_eq!(
            super::text_route_line(&d(Terminal::Warp, Route::KittyPrecut), &none),
            None
        );
    }

    #[test]
    fn the_window_is_kept_on_the_text_route_on_macos_unless_turned_off() {
        let none = |_: &str| None;
        assert_eq!(wanted(Route::Text, &none), cfg!(target_os = "macos"));
        for route in Route::ALL.into_iter().filter(|r| *r != Route::Text) {
            assert!(!wanted(route, &none), "{route:?} draws in the terminal");
        }
        for off in ["off", "0", "false", "no"] {
            assert!(!wanted(Route::Text, &|k| (k == "TILES_WINDOW").then(|| off.into())));
        }
    }
}
