//! Which terminal Tiles is drawing in, and the image route it takes there
//! (ADR-004).
//!
//! Detection is explicit: an override (`TILES_TERMINAL`, `TILES_ROUTE`, or the
//! `--terminal` and `--route` flags that set them) wins; otherwise the
//! environment names the terminal and any multiplexer; replies to the kitty
//! graphics query, XTVERSION and device attributes fill in what the
//! environment leaves unknown. Every result carries the reason it was chosen,
//! so `tiles doctor` can show it.

use std::fmt;

/// A terminal Tiles knows the rules for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    /// kitty.
    Kitty,
    /// Ghostty.
    Ghostty,
    /// iTerm2.
    Iterm2,
    /// Warp.
    Warp,
    /// `WezTerm`.
    Wezterm,
    /// The VS Code terminal.
    Vscode,
    /// macOS Terminal.app.
    AppleTerminal,
    /// Alacritty.
    Alacritty,
    /// foot.
    Foot,
    /// Windows Terminal.
    WindowsTerminal,
    /// A VTE terminal such as GNOME Terminal.
    Vte,
    /// xterm.
    Xterm,
    /// Not identified.
    Unknown,
}

impl Terminal {
    /// Every terminal, for overrides and docs.
    pub const ALL: [Terminal; 13] = [
        Terminal::Kitty,
        Terminal::Ghostty,
        Terminal::Iterm2,
        Terminal::Warp,
        Terminal::Wezterm,
        Terminal::Vscode,
        Terminal::AppleTerminal,
        Terminal::Alacritty,
        Terminal::Foot,
        Terminal::WindowsTerminal,
        Terminal::Vte,
        Terminal::Xterm,
        Terminal::Unknown,
    ];

    /// The name overrides use.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Terminal::Kitty => "kitty",
            Terminal::Ghostty => "ghostty",
            Terminal::Iterm2 => "iterm2",
            Terminal::Warp => "warp",
            Terminal::Wezterm => "wezterm",
            Terminal::Vscode => "vscode",
            Terminal::AppleTerminal => "apple-terminal",
            Terminal::Alacritty => "alacritty",
            Terminal::Foot => "foot",
            Terminal::WindowsTerminal => "windows-terminal",
            Terminal::Vte => "vte",
            Terminal::Xterm => "xterm",
            Terminal::Unknown => "unknown",
        }
    }

    /// The terminal an override names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Terminal> {
        Terminal::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The route this terminal takes on its own, outside any multiplexer.
    #[must_use]
    pub fn native_route(self) -> Route {
        match self {
            Terminal::Kitty | Terminal::Ghostty | Terminal::Iterm2 => Route::KittyPlaceholders,
            Terminal::Warp | Terminal::Wezterm | Terminal::Vscode => Route::KittyPrecut,
            Terminal::Foot | Terminal::WindowsTerminal => Route::Sixel,
            Terminal::AppleTerminal
            | Terminal::Alacritty
            | Terminal::Vte
            | Terminal::Xterm
            | Terminal::Unknown => Route::Text,
        }
    }
}

/// A multiplexer between Tiles and the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multiplexer {
    /// herdr: passes kitty graphics through, crop rectangles included, but
    /// ignores the row mark on placeholder cells (ADR-004).
    Herdr,
    /// tmux: passes no graphics without `allow-passthrough`, so Tiles sends none.
    Tmux,
    /// GNU screen: passes no graphics.
    Screen,
}

impl Multiplexer {
    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Multiplexer::Herdr => "herdr",
            Multiplexer::Tmux => "tmux",
            Multiplexer::Screen => "screen",
        }
    }
}

/// How a card reaches the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Kitty graphics drawn through Unicode placeholder cells, which scroll
    /// with the text and survive redraws.
    KittyPlaceholders,
    /// Kitty graphics placed at the cursor; a partly visible card is sent as a
    /// new image of its visible rows and a moved card is deleted and placed
    /// again under a new id.
    KittyPrecut,
    /// One sixel picture, redrawn when it scrolls back into view.
    Sixel,
    /// No inline image: a text summary and the saved PNG's path.
    Text,
}

impl Route {
    /// Every route.
    pub const ALL: [Route; 4] = [
        Route::KittyPlaceholders,
        Route::KittyPrecut,
        Route::Sixel,
        Route::Text,
    ];

    /// The name overrides use.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Route::KittyPlaceholders => "kitty-placeholders",
            Route::KittyPrecut => "kitty-precut",
            Route::Sixel => "sixel",
            Route::Text => "text",
        }
    }

    /// The route an override names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Route> {
        Route::ALL.into_iter().find(|r| r.name() == name)
    }

    /// Whether the route draws pixels.
    #[must_use]
    pub fn draws_images(self) -> bool {
        self != Route::Text
    }
}

/// What a terminal said back to the queries in [`QUERIES`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replies {
    /// The kitty graphics query was answered `OK`.
    pub kitty_graphics: bool,
    /// The XTVERSION name, such as `kitty(0.39.1)`, cleaned to plain text.
    pub xtversion: Option<String>,
    /// Primary device attributes list 4 (sixel).
    pub sixel: bool,
    /// The cell size in pixels (width, height), from `CSI 16 t`.
    pub cell_px: Option<(u16, u16)>,
    /// The background colour, from `OSC 11`.
    pub background: Option<(u8, u8, u8)>,
    /// The device attributes reply arrived, so every earlier reply has too.
    pub complete: bool,
}

/// The queries Tiles sends, kitty graphics and XTVERSION first and primary
/// device attributes last: every terminal answers DA1, and answers in order,
/// so its reply marks the end.
pub const QUERIES: &str = concat!(
    "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\",
    "\x1b[>0q",
    "\x1b[16t",
    "\x1b]11;?\x1b\\",
    "\x1b[c"
);

impl Replies {
    /// Whether the background reads as light (relative luminance over 140 of
    /// 255, the prototype's threshold).
    #[must_use]
    pub fn light_background(&self) -> Option<bool> {
        self.background.map(|(r, g, b)| {
            0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b) > 140.0
        })
    }

    /// Reads the replies out of whatever the terminal sent back. Unrelated
    /// bytes (keys the user pressed) are skipped.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Replies {
        let text = String::from_utf8_lossy(bytes);
        let mut out = Replies::default();
        if let Some(at) = text.find("\x1b_Gi=31;") {
            out.kitty_graphics = text[at + 8..].starts_with("OK");
        }
        if let Some(at) = text.find("\x1bP>|") {
            let rest = &text[at + 4..];
            if let Some(end) = rest.find("\x1b\\") {
                let name = crate::sanitize::line(&rest[..end], 60);
                out.xtversion = (!name.is_empty()).then_some(name);
            }
        }
        if let Some(at) = text.find("\x1b[6;") {
            let rest = &text[at + 4..];
            if let Some(end) = rest.find('t') {
                let mut parts = rest[..end].split(';').map(|p| p.parse::<u16>().ok());
                if let (Some(Some(h)), Some(Some(w)), None) =
                    (parts.next(), parts.next(), parts.next())
                    && w > 0
                    && h > 0
                {
                    out.cell_px = Some((w, h));
                }
            }
        }
        if let Some(at) = text.find("\x1b]11;rgb:") {
            let channel = |s: &str| u8::from_str_radix(s.get(..2)?, 16).ok();
            let rest = text.get(at + 9..).unwrap_or_default();
            let mut parts = rest.split('/');
            if let (Some(r), Some(g), Some(b)) = (parts.next(), parts.next(), parts.next())
                && let (Some(r), Some(g), Some(b)) = (channel(r), channel(g), channel(b))
            {
                out.background = Some((r, g, b));
            }
        }
        if let Some(at) = text.find("\x1b[?") {
            let rest = &text[at + 3..];
            if let Some(end) = rest.find('c') {
                let params = &rest[..end];
                if params.bytes().all(|b| b.is_ascii_digit() || b == b';') {
                    out.complete = true;
                    out.sixel = params.split(';').any(|p| p == "4");
                }
            }
        }
        out
    }
}

/// Where a cell size came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellSource {
    /// The terminal's `CSI 16 t` reply, in device pixels.
    Reply,
    /// The window size in pixels over its rows and columns (`TIOCGWINSZ`).
    WindowSize,
    /// The window size, read as points and multiplied by the scale: Warp
    /// reports it in points at every font, and so did any terminal whose
    /// cell would be under [`MIN_CELL_CSS_HEIGHT`] tall.
    WindowPoints,
}

impl CellSource {
    /// How `tiles doctor` names it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            CellSource::Reply => "the terminal's CSI 16 t reply",
            CellSource::WindowSize => "the window size in pixels",
            CellSource::WindowPoints => "the window size, read as points",
        }
    }
}

/// The shortest cell, in CSS pixels, a window size is read as device
/// pixels in a terminal not known to report points: no terminal font draws
/// a line under 10 points tall, so a shorter cell was reported in points.
pub const MIN_CELL_CSS_HEIGHT: f32 = 10.0;

/// The cell size in device pixels at `scale` device pixels per CSS pixel,
/// and where it came from: the `CSI 16 t` reply as it is, else the window
/// size (`TIOCGWINSZ` over rows and columns), read as points in Warp, which
/// reports points at every font, and elsewhere when a cell would be under
/// [`MIN_CELL_CSS_HEIGHT`] tall. `None` leaves the MCP server's default.
#[must_use]
pub fn cell_px(
    terminal: Terminal,
    reply: Option<(u16, u16)>,
    window: Option<(u16, u16)>,
    scale: f32,
) -> Option<((u16, u16), CellSource)> {
    let usable = |(w, h): (u16, u16)| w >= 4 && h >= 8;
    if let Some(c) = reply.filter(|c| usable(*c)) {
        return Some((c, CellSource::Reply));
    }
    let times = |v: u16| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped first.
        let px = (f32::from(v) * scale)
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16;
        px
    };
    let (w, h) = window.filter(|(w, h)| *w > 0 && *h > 0)?;
    if terminal == Terminal::Warp || f32::from(h) / scale < MIN_CELL_CSS_HEIGHT {
        let c = (times(w), times(h));
        return usable(c).then_some((c, CellSource::WindowPoints));
    }
    usable((w, h)).then_some(((w, h), CellSource::WindowSize))
}

/// The detected terminal and the route Tiles will use, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// The terminal.
    pub terminal: Terminal,
    /// The multiplexer in between, if any.
    pub multiplexer: Option<Multiplexer>,
    /// The route.
    pub route: Route,
    /// Why, in one line.
    pub reason: String,
}

impl fmt::Display for Detected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.terminal.name())?;
        if let Some(m) = self.multiplexer {
            write!(f, " inside {}", m.name())?;
        }
        write!(f, ", route {} ({})", self.route.name(), self.reason)
    }
}

/// The environment override for the terminal.
pub const ENV_TERMINAL: &str = "TILES_TERMINAL";
/// The environment override for the route.
pub const ENV_ROUTE: &str = "TILES_ROUTE";

/// Why an override was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadOverride(pub String);

impl fmt::Display for BadOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn terminal_from_env(env: &dyn Fn(&str) -> Option<String>) -> Option<(Terminal, &'static str)> {
    let has = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    let program = env("TERM_PROGRAM").unwrap_or_default().to_ascii_lowercase();
    let term = env("TERM").unwrap_or_default().to_ascii_lowercase();
    // Most specific first: a terminal's own variables survive a `TERM` that
    // a multiplexer or SSH rewrote.
    let found = if has("KITTY_WINDOW_ID") || program == "kitty" || term == "xterm-kitty" {
        (Terminal::Kitty, "KITTY_WINDOW_ID or TERM")
    } else if program == "ghostty" || term.contains("ghostty") || has("GHOSTTY_RESOURCES_DIR") {
        (Terminal::Ghostty, "TERM_PROGRAM=ghostty")
    } else if program == "warpterminal"
        || has("WARP_SESSION_ID")
        || has("WARP_TERMINAL_SESSION_UUID")
        || has("WARP_IS_LOCAL_SHELL_SESSION")
    {
        (Terminal::Warp, "TERM_PROGRAM=WarpTerminal or WARP_*")
    } else if program == "iterm.app" || has("ITERM_SESSION_ID") {
        (Terminal::Iterm2, "TERM_PROGRAM=iTerm.app")
    } else if program == "wezterm" || has("WEZTERM_PANE") {
        (Terminal::Wezterm, "TERM_PROGRAM=WezTerm")
    } else if program == "vscode" {
        (Terminal::Vscode, "TERM_PROGRAM=vscode")
    } else if program == "apple_terminal" {
        (Terminal::AppleTerminal, "TERM_PROGRAM=Apple_Terminal")
    } else if has("WT_SESSION") {
        (Terminal::WindowsTerminal, "WT_SESSION")
    } else if has("ALACRITTY_WINDOW_ID") || term == "alacritty" {
        (Terminal::Alacritty, "ALACRITTY_WINDOW_ID or TERM")
    } else if term.starts_with("foot") {
        (Terminal::Foot, "TERM=foot")
    } else if has("VTE_VERSION") {
        (Terminal::Vte, "VTE_VERSION")
    } else {
        return None;
    };
    Some(found)
}

fn multiplexer_from_env(env: &dyn Fn(&str) -> Option<String>) -> Option<Multiplexer> {
    let has = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    let term = env("TERM").unwrap_or_default().to_ascii_lowercase();
    let program = env("TERM_PROGRAM").unwrap_or_default().to_ascii_lowercase();
    // Innermost first: a multiplexer inherits the variables of the one it
    // runs in, but sets TERM_PROGRAM or TERM itself.
    if program == "tmux" || term.starts_with("tmux") {
        Some(Multiplexer::Tmux)
    } else if program == "herdr" {
        Some(Multiplexer::Herdr)
    } else if has("TMUX") {
        Some(Multiplexer::Tmux)
    } else if has("STY") || term.starts_with("screen") {
        Some(Multiplexer::Screen)
    } else if has("HERDR_PANE_ID") || has("HERDR_SESSION") {
        Some(Multiplexer::Herdr)
    } else {
        None
    }
}

fn terminal_from_replies(r: &Replies) -> Option<Terminal> {
    let name = r.xtversion.as_deref()?.to_ascii_lowercase();
    if name.contains("libghostty") {
        // herdr answers XTVERSION as libghostty: it is the multiplexer, not
        // the terminal.
        return None;
    }
    [
        ("kitty", Terminal::Kitty),
        ("ghostty", Terminal::Ghostty),
        ("iterm2", Terminal::Iterm2),
        ("wezterm", Terminal::Wezterm),
        ("foot", Terminal::Foot),
        ("xterm", Terminal::Xterm),
    ]
    .into_iter()
    .find(|(n, _)| name.starts_with(n))
    .map(|(_, t)| t)
}

/// Detects the terminal and route from the environment (`env` looks up one
/// variable) and the query replies, if any were read.
///
/// # Errors
///
/// When `TILES_TERMINAL` or `TILES_ROUTE` names nothing Tiles knows.
pub fn detect(
    env: &dyn Fn(&str) -> Option<String>,
    replies: Option<&Replies>,
) -> Result<Detected, BadOverride> {
    let multiplexer = match replies.and_then(|r| r.xtversion.as_deref()) {
        Some(v) if v.to_ascii_lowercase().starts_with("tmux") => Some(Multiplexer::Tmux),
        _ => multiplexer_from_env(env),
    };
    let forced_terminal = match env(ENV_TERMINAL).filter(|v| !v.is_empty()) {
        Some(name) => Some(Terminal::from_name(&name).ok_or_else(|| {
            BadOverride(format!(
                "{ENV_TERMINAL}={} is not a terminal Tiles knows; use one of: {}",
                crate::sanitize::line(&name, 40),
                Terminal::ALL.map(Terminal::name).join(", ")
            ))
        })?),
        None => None,
    };
    let forced_route = match env(ENV_ROUTE).filter(|v| !v.is_empty()) {
        Some(name) => Some(Route::from_name(&name).ok_or_else(|| {
            BadOverride(format!(
                "{ENV_ROUTE}={} is not a route; use one of: {}",
                crate::sanitize::line(&name, 40),
                Route::ALL.map(Route::name).join(", ")
            ))
        })?),
        None => None,
    };
    let (terminal, mut reason) = if let Some(t) = forced_terminal {
        (t, format!("{ENV_TERMINAL} override"))
    } else if let Some((t, why)) = terminal_from_env(env) {
        (t, why.to_string())
    } else if let Some(t) = replies.and_then(terminal_from_replies) {
        (t, "XTVERSION reply".to_string())
    } else {
        (Terminal::Unknown, "nothing identified it".to_string())
    };
    let mut route = terminal.native_route();
    if terminal == Terminal::Unknown
        && let Some(r) = replies
    {
        if r.kitty_graphics {
            route = Route::KittyPrecut;
            reason.push_str("; answered the kitty graphics query");
        } else if r.sixel {
            route = Route::Sixel;
            reason.push_str("; device attributes list sixel");
        }
    }
    match multiplexer {
        Some(Multiplexer::Herdr) if terminal == Terminal::Unknown && forced_terminal.is_none() => {
            route = Route::KittyPrecut;
            reason.push_str("; herdr passes kitty graphics through");
        }
        Some(Multiplexer::Herdr) if route == Route::KittyPlaceholders => {
            route = Route::KittyPrecut;
            reason.push_str("; herdr ignores the placeholder row mark");
        }
        Some(Multiplexer::Tmux | Multiplexer::Screen) if route.draws_images() => {
            route = Route::Text;
            reason.push_str("; the multiplexer passes no graphics");
        }
        _ => {}
    }
    if let Some(r) = forced_route {
        route = r;
        reason = format!("{ENV_ROUTE} override");
    }
    Ok(Detected {
        terminal,
        multiplexer,
        route,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{Multiplexer, Replies, Route, Terminal, detect};

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn route(pairs: &[(&str, &str)]) -> Route {
        detect(&env(pairs), None).unwrap().route
    }

    #[test]
    fn each_terminal_takes_its_adr_004_route() {
        assert_eq!(route(&[("KITTY_WINDOW_ID", "1")]), Route::KittyPlaceholders);
        assert_eq!(
            route(&[("TERM_PROGRAM", "ghostty")]),
            Route::KittyPlaceholders
        );
        assert_eq!(
            route(&[("TERM_PROGRAM", "iTerm.app")]),
            Route::KittyPlaceholders
        );
        assert_eq!(
            route(&[("TERM_PROGRAM", "WarpTerminal")]),
            Route::KittyPrecut
        );
        assert_eq!(route(&[("TERM_PROGRAM", "WezTerm")]), Route::KittyPrecut);
        assert_eq!(route(&[("TERM_PROGRAM", "vscode")]), Route::KittyPrecut);
        assert_eq!(route(&[("TERM", "foot")]), Route::Sixel);
        assert_eq!(route(&[("WT_SESSION", "x")]), Route::Sixel);
        assert_eq!(route(&[("TERM_PROGRAM", "Apple_Terminal")]), Route::Text);
        assert_eq!(route(&[("ALACRITTY_WINDOW_ID", "1")]), Route::Text);
        assert_eq!(route(&[]), Route::Text);
    }

    #[test]
    fn herdr_inside_warp_is_precut_and_inside_kitty_too() {
        let d = detect(
            &env(&[("TERM_PROGRAM", "WarpTerminal"), ("HERDR_PANE_ID", "p1")]),
            None,
        )
        .unwrap();
        assert_eq!(
            (d.terminal, d.multiplexer, d.route),
            (Terminal::Warp, Some(Multiplexer::Herdr), Route::KittyPrecut)
        );
        let d = detect(
            &env(&[("KITTY_WINDOW_ID", "1"), ("HERDR_PANE_ID", "p1")]),
            None,
        )
        .unwrap();
        assert_eq!(d.route, Route::KittyPrecut);
        assert!(d.reason.contains("herdr"));
    }

    #[test]
    fn herdr_hiding_the_outer_terminal_still_draws() {
        let d = detect(&env(&[("TERM_PROGRAM", "herdr")]), None).unwrap();
        assert_eq!(
            (d.terminal, d.multiplexer, d.route),
            (
                Terminal::Unknown,
                Some(Multiplexer::Herdr),
                Route::KittyPrecut
            )
        );
        let r = Replies::parse(b"\x1bP>|libghostty 1.2\x1b\\\x1b[?62c");
        let d = detect(&env(&[("TERM_PROGRAM", "herdr")]), Some(&r)).unwrap();
        assert_eq!(d.route, Route::KittyPrecut);
    }

    #[test]
    fn the_innermost_multiplexer_wins() {
        let tmux_in_herdr = [
            ("TERM_PROGRAM", "tmux"),
            ("TMUX", "/t,1,0"),
            ("HERDR_PANE_ID", "p1"),
            ("WARP_SESSION_ID", "w"),
        ];
        let d = detect(&env(&tmux_in_herdr), None).unwrap();
        assert_eq!(
            (d.multiplexer, d.route),
            (Some(Multiplexer::Tmux), Route::Text)
        );
        let r = Replies::parse(b"\x1bP>|tmux 3.7\x1b\\\x1b[?62;4c");
        let d = detect(
            &env(&[("HERDR_PANE_ID", "p1"), ("WARP_SESSION_ID", "w")]),
            Some(&r),
        )
        .unwrap();
        assert_eq!(
            (d.multiplexer, d.route),
            (Some(Multiplexer::Tmux), Route::Text)
        );
        let herdr = [("TERM_PROGRAM", "herdr"), ("TMUX", "/stale,1,0")];
        assert_eq!(
            detect(&env(&herdr), None).unwrap().multiplexer,
            Some(Multiplexer::Herdr)
        );
    }

    #[test]
    fn tmux_and_screen_fall_back_to_text() {
        assert_eq!(
            route(&[("KITTY_WINDOW_ID", "1"), ("TMUX", "/tmp/t,1,0")]),
            Route::Text
        );
        assert_eq!(
            route(&[
                ("TERM_PROGRAM", "WarpTerminal"),
                ("TERM", "screen-256color")
            ]),
            Route::Text
        );
    }

    #[test]
    fn overrides_win_and_are_reported() {
        let d = detect(
            &env(&[
                ("TERM_PROGRAM", "WarpTerminal"),
                ("TILES_TERMINAL", "kitty"),
            ]),
            None,
        )
        .unwrap();
        assert_eq!(
            (d.terminal, d.route),
            (Terminal::Kitty, Route::KittyPlaceholders)
        );
        let d = detect(
            &env(&[("KITTY_WINDOW_ID", "1"), ("TILES_ROUTE", "sixel")]),
            None,
        )
        .unwrap();
        assert_eq!(d.route, Route::Sixel);
        assert_eq!(d.reason, "TILES_ROUTE override");
        assert_eq!(d.to_string(), "kitty, route sixel (TILES_ROUTE override)");
    }

    #[test]
    fn a_bad_override_is_refused_without_echoing_escapes() {
        let e = detect(&env(&[("TILES_TERMINAL", "\x1b]52;c;x\x07kitty")]), None).unwrap_err();
        assert!(!e.0.contains('\x1b') && !e.0.contains('\x07'));
        assert!(e.0.contains("use one of: kitty, ghostty"));
        assert!(detect(&env(&[("TILES_ROUTE", "magic")]), None).is_err());
    }

    #[test]
    fn replies_identify_an_unknown_terminal() {
        let bytes = b"\x1b_Gi=31;OK\x1b\\\x1bP>|kitty(0.39.1)\x1b\\\x1b[?62;4;22c";
        let r = Replies::parse(bytes);
        assert!(r.kitty_graphics && r.sixel && r.complete);
        assert_eq!(r.xtversion.as_deref(), Some("kitty(0.39.1)"));
        let d = detect(&env(&[]), Some(&r)).unwrap();
        assert_eq!(
            (d.terminal, d.route),
            (Terminal::Kitty, Route::KittyPlaceholders)
        );
        let r = Replies::parse(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;22c");
        assert_eq!(
            detect(&env(&[]), Some(&r)).unwrap().route,
            Route::KittyPrecut
        );
        let r = Replies::parse(b"\x1b[?62;4c");
        assert_eq!(detect(&env(&[]), Some(&r)).unwrap().route, Route::Sixel);
    }

    #[test]
    fn replies_carry_cell_size_and_background() {
        let r = Replies::parse(b"\x1b[6;36;18t\x1b]11;rgb:ffff/fafa/f0f0\x1b\\\x1b[?62c");
        assert_eq!(r.cell_px, Some((18, 36)));
        assert_eq!(r.background, Some((255, 250, 240)));
        assert_eq!(r.light_background(), Some(true));
        let r = Replies::parse(b"\x1b]11;rgb:1e1e/1e1e/2e2e\x07\x1b[6;0;9t");
        assert_eq!(r.light_background(), Some(false));
        assert_eq!(r.cell_px, None);
    }

    #[test]
    fn a_cell_size_comes_from_the_reply_then_the_window_read_as_points_in_warp() {
        use super::{CellSource, cell_px};
        let kitty = cell_px(Terminal::Kitty, Some((18, 36)), Some((9, 18)), 2.0);
        assert_eq!(
            kitty,
            Some(((18, 36), CellSource::Reply)),
            "kitty as before"
        );
        let device = cell_px(Terminal::Ghostty, None, Some((16, 32)), 2.0);
        assert_eq!(device, Some(((16, 32), CellSource::WindowSize)));
        // Warp's window size is in points at every font: 8x16 at its default
        // font, 10x20 at a 16 pt font, which no threshold tells from pixels.
        let warp = cell_px(Terminal::Warp, None, Some((8, 16)), 2.0);
        assert_eq!(warp, Some(((16, 32), CellSource::WindowPoints)));
        let warp = cell_px(Terminal::Warp, None, Some((10, 20)), 2.0);
        assert_eq!(warp, Some(((20, 40), CellSource::WindowPoints)));
        // Elsewhere only a cell too short for any font was reported in points.
        let short = cell_px(Terminal::Wezterm, None, Some((8, 16)), 2.0);
        assert_eq!(short, Some(((16, 32), CellSource::WindowPoints)));
        let tall = cell_px(Terminal::Wezterm, None, Some((10, 20)), 2.0);
        assert_eq!(tall, Some(((10, 20), CellSource::WindowSize)));
        assert_eq!(cell_px(Terminal::Warp, None, None, 2.0), None);
        assert_eq!(cell_px(Terminal::Wezterm, None, None, 2.0), None);
        assert_eq!(
            cell_px(Terminal::Warp, Some((0, 9)), Some((0, 0)), 2.0),
            None
        );
    }

    #[test]
    fn background_reads_two_and_four_digit_channels() {
        let r = Replies::parse(b"\x1b]11;rgb:ff/fa/f0\x1b\\");
        assert_eq!(r.background, Some((255, 250, 240)));
        let r = Replies::parse(b"\x1b]11;rgb:12ab/34cd/56ef\x07");
        assert_eq!(r.background, Some((0x12, 0x34, 0x56)));
    }

    #[test]
    fn a_short_or_split_background_reply_is_no_background() {
        for reply in [
            b"\x1b]11;rgb:".as_slice(),
            b"\x1b]11;rgb:f",
            b"\x1b]11;rgb:ff",
            b"\x1b]11;rgb:ff/ff/f",
            b"\x1b]11;rgb:\xe2\x80",
        ] {
            assert_eq!(Replies::parse(reply).background, None, "{reply:?}");
        }
    }

    #[test]
    fn replies_ignore_noise_and_errors() {
        let r = Replies::parse(b"abc\x1b_Gi=31;ENOTSUPPORTED\x1b\\");
        assert!(!r.kitty_graphics && !r.complete);
        let r = Replies::parse(b"\x1bP>|evil\x1b]52;c;x\x07\x1b\\\x1b[?1;2c");
        assert!(!r.xtversion.unwrap().contains('\x1b'));
        assert!(r.complete && !r.sixel);
    }
}
