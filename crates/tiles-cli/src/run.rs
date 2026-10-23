//! `tiles run [options] <harness> [harness arguments]`: the front door. It
//! asks the terminal what it is, picks the route, writes this session's
//! add-ons, and launches the harness with Tiles loaded through the harness's
//! own options, under the wrapper only where no hook draws the card.

use std::path::PathBuf;
use std::process::ExitCode;

use tiles_mcp::card::Painter as _;
use tiles_mcp::source::Source;
use tiles_term::detect::{self, Replies};

use crate::addons::{self, Spec};
use crate::harness::{self, Harness, Setup};
use crate::serve;
use crate::wrap::{self, Session};

/// A parsed `tiles run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The harness.
    pub harness: Harness,
    /// Its own arguments, passed through.
    pub rest: Vec<String>,
    /// `--wrapper`: draw through the wrapper even where the harness draws.
    pub force_wrapper: bool,
    /// `--terminal <name>`.
    pub terminal: Option<String>,
    /// `--route <name>`.
    pub route: Option<String>,
    /// `--dry-run`: print the launch and stop.
    pub dry_run: bool,
    /// `--allow-omni` and `--allow-<source>`: the data sources whose tools
    /// and charts the harness may run without asking, by name (`omni`,
    /// `warehouse`).
    pub allow: Vec<String>,
}

/// Usage of `tiles run`.
pub const USAGE: &str = "tiles run [--wrapper] [--allow-omni] [--terminal <name>] [--route <name>] [--dry-run] <pi|opencode|claude|codex> [harness arguments]";

/// Parses the arguments after `run`, where `sources` names the registered
/// sources `--allow-<name>` takes. Tiles options come before the harness;
/// everything after it belongs to the harness.
///
/// # Errors
///
/// A missing or unknown harness, an `--allow-` naming no source, or an
/// option without its value.
pub fn parse(args: &[String], sources: &[&str]) -> Result<Args, String> {
    let mut out = Args {
        harness: Harness::Pi,
        rest: Vec::new(),
        force_wrapper: false,
        terminal: None,
        route: None,
        dry_run: false,
        allow: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wrapper" => out.force_wrapper = true,
            "--dry-run" => out.dry_run = true,
            "--allow-omni" => out.allow.push("omni".into()),
            flag if flag.starts_with("--allow-") => {
                let name = &flag["--allow-".len()..];
                if !sources.contains(&name) {
                    let mut known = vec!["--allow-omni".to_string()];
                    known.extend(sources.iter().map(|s| format!("--allow-{s}")));
                    return Err(format!(
                        "{} names no data source; use {}",
                        tiles_term::sanitize::line(flag, 60),
                        known.join(" or ")
                    ));
                }
                out.allow.push(name.to_string());
            }
            "--terminal" => {
                out.terminal = Some(it.next().ok_or("--terminal needs a name")?.clone());
            }
            "--route" => out.route = Some(it.next().ok_or("--route needs a name")?.clone()),
            name => {
                out.harness = Harness::from_name(name).ok_or_else(|| {
                    format!(
                        "\"{}\" is not a harness; use pi, opencode, claude or codex",
                        tiles_term::sanitize::line(name, 40)
                    )
                })?;
                out.rest = it.cloned().collect();
                return Ok(out);
            }
        }
    }
    Err(format!("run needs a harness: {USAGE}"))
}

/// The cell size `tiles run` passes as `TILES_CELL_PX`, and where it came
/// from: the variable when set, else [`detect::cell_px`] over the terminal's
/// reply and window size at the session's scale.
#[must_use]
pub fn cell(
    replies: Option<&Replies>,
    env: &dyn Fn(&str) -> Option<String>,
    terminal: detect::Terminal,
    window: Option<(u16, u16)>,
) -> Option<(String, &'static str)> {
    if let Some(v) = env("TILES_CELL_PX") {
        return Some((v, "TILES_CELL_PX"));
    }
    let scale = tiles_mcp::look_from_env(env).scale;
    detect::cell_px(terminal, replies.and_then(|r| r.cell_px), window, scale)
        .map(|((w, h), source)| (format!("{w}x{h}"), source.name()))
}

/// The terminal's size, as `tiles run` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    /// Rows and columns.
    pub size: (u16, u16),
    /// One cell from the window size, when the terminal reports it.
    pub window: Option<(u16, u16)>,
}

impl Screen {
    /// The terminal on stdout now.
    #[must_use]
    pub fn now() -> Self {
        Self {
            size: rustix::termios::tcgetwinsize(rustix::stdio::stdout())
                .map_or((24, 100), |w| (w.ws_row, w.ws_col)),
            window: wrap::winsize_cell_px(),
        }
    }

    /// The terminal `probe` asks, or stdout's when there is no probe.
    #[must_use]
    pub fn of(probe: Option<&wrap::Probe>) -> Self {
        let Some(probe) = probe else {
            return Self::now();
        };
        Self {
            size: probe.rows_cols().unwrap_or((24, 100)),
            window: probe.cell_px(),
        }
    }
}

/// The geometry and theme variables `tiles run` gives the MCP server.
#[must_use]
pub fn look_env(
    replies: Option<&Replies>,
    env: &dyn Fn(&str) -> Option<String>,
    cache: &std::path::Path,
    harness: Harness,
    detected: &detect::Detected,
    screen: Screen,
) -> Vec<(String, String)> {
    let (rows, cols) = screen.size;
    let cell = cell(replies, env, detected.terminal, screen.window).map(|(c, _)| c);
    // A theme the user set for this session passes through; otherwise the
    // server reads the saved theme on every card, so `/theme` applies to
    // the next one (ADR-021). The terminal's background no longer picks it.
    let theme = env("TILES_THEME");
    let mut out = vec![
        (
            "TILES_COLS".to_string(),
            wrap::paint::conversation_cols(harness.side_panel(), cols)
                .max(20)
                .to_string(),
        ),
        ("TILES_CACHE_DIR".to_string(), cache.display().to_string()),
    ];
    out.extend(cell.map(|c| ("TILES_CELL_PX".to_string(), c)));
    // On the pre-cut route the terminal scales a card's visible rows to its
    // cells, so a card taller than the screen is fitted to the rows and keeps
    // its headline; a sixel is drawn at its pixel size and would spill.
    if detected.route == detect::Route::KittyPrecut {
        out.push(("TILES_ROWS".to_string(), rows.to_string()));
    }
    out.extend(theme.map(|t| ("TILES_THEME".to_string(), t)));
    out
}

/// A token for this run, told apart from any other run on the machine by its
/// start time and process id.
fn session_token() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    #[allow(clippy::cast_possible_truncation)] // The low bits change fastest.
    let nanos = nanos as u64;
    nanos ^ (u64::from(std::process::id()) << 40)
}

fn fail(e: &str) -> ExitCode {
    eprintln!("tiles: {}", tiles_term::sanitize::line(e, 400));
    ExitCode::FAILURE
}

fn print_plan(detected: &detect::Detected, plan: &harness::Plan) {
    println!(
        "{}",
        tiles_term::sanitize::line(&format!("terminal: {detected}"), 400)
    );
    println!(
        "draw: {}, wrapper: {}",
        plan.draw.name(),
        if plan.wrap { "yes" } else { "no" }
    );
    println!(
        "program: {}",
        tiles_term::sanitize::line(&plan.launch.program, 80)
    );
    for a in &plan.launch.args {
        println!("  arg: {}", tiles_term::sanitize::line(a, 2000));
    }
    for (k, v) in &plan.launch.env {
        println!("  env: {k}={}", tiles_term::sanitize::line(v, 2000));
    }
    for (path, text) in &plan.files {
        println!(
            "  file: {} = {}",
            tiles_term::sanitize::line(&path.display().to_string(), 400),
            tiles_term::sanitize::lines(text)
        );
    }
}

/// What `tiles run` asks while the terminal is queried and the engine
/// loads: which `OpenCode` command line it is (ADR-029), the harness's
/// version, and, unless this is a dry run, the daily update check (ADR-032).
struct Asked {
    opencode: Option<std::thread::JoinHandle<Option<crate::opencode::Cli>>>,
    version: std::thread::JoinHandle<Option<String>>,
    update: Option<crate::update::Pending>,
}

impl Asked {
    fn start(harness: Harness, dry_run: bool) -> Self {
        let program = std::path::Path::new(harness.name());
        Self {
            opencode: (harness == Harness::OpenCode)
                .then(|| std::thread::spawn(move || crate::opencode::read_cli(program))),
            version: std::thread::spawn(move || crate::proved::read(program)),
            update: (!dry_run).then(crate::update::Pending::start),
        }
    }

    /// The `OpenCode` command line, once its `--help` answered.
    fn opencode(&mut self) -> Option<crate::opencode::Cli> {
        self.opencode.take().and_then(|t| t.join().ok().flatten())
    }

    /// The lines for the user before the harness starts and again after it
    /// exits: a version Tiles was not proved on, and the update notice.
    fn lines(self, harness: Harness, cli: Option<crate::opencode::Cli>) -> Vec<String> {
        let version = self.version.join().ok().flatten();
        let mut out: Vec<String> = crate::proved::run_line(harness, cli, version.as_deref())
            .into_iter()
            .collect();
        out.extend(self.update.and_then(crate::update::Pending::notice));
        out
    }
}

/// Prints [`Asked::lines`] on stderr; with the terminal in raw mode a line
/// ends `\r\n`.
fn say(lines: &[String], raw: bool) {
    let end = if raw { "\r\n" } else { "\n" };
    for l in lines {
        eprint!("{}{end}", tiles_term::sanitize::line(l, 400));
    }
}

/// Writes the add-ons under `dir`, then the files the launch reads.
fn write_addons(dir: &std::path::Path, spec: &Spec, plan: &harness::Plan) -> Result<(), String> {
    addons::write_all(dir, spec)?;
    plan.files
        .iter()
        .try_for_each(|(path, text)| addons::write_file(path, text.as_bytes()))
}

/// The user's own Claude Code settings, from a `--settings` among the
/// harness arguments ([`harness::settings_value`]). Read first, before the
/// terminal is asked anything and before the engine: a value that cannot be
/// read or parsed then refuses the launch with one line and nothing else on
/// stderr. The engine's font manager wakes CoreFoundation, which reads the
/// process's own `-key value` arguments as defaults and, on macOS 15, logs
/// a value that is not a property list, such as `{oops`.
fn user_settings(args: &Args) -> Result<Option<serde_json::Value>, String> {
    if args.harness != Harness::Claude {
        return Ok(None);
    }
    let (_, spec) = harness::take_settings(&args.rest)?;
    spec.map(|s| harness::settings_value(&s, &|p| std::fs::read_to_string(p)))
        .transpose()
}

/// The parsed arguments with the user's settings ([`user_settings`]), or
/// the exit code after the line that refuses them.
fn arguments(
    args: &[String],
    sources: &[&str],
) -> Result<(Args, Option<serde_json::Value>), ExitCode> {
    let args = parse(args, sources).map_err(|e| {
        eprintln!(
            "tiles: {}\nusage: {}",
            tiles_term::sanitize::line(&e, 400),
            usage(sources)
        );
        ExitCode::from(2)
    })?;
    let settings = user_settings(&args).map_err(|e| fail(&e))?;
    Ok((args, settings))
}

/// The user's home, for the cache's home-relative spellings.
fn home() -> Option<PathBuf> {
    serve::env("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The companion window where `setup` keeps one: on the text route the
/// session's cards show in one window beside the terminal, closed when the
/// harness exits (ADR-046).
fn companion(
    setup: &Setup,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<crate::companion::Companion> {
    setup.companion.then(|| {
        crate::companion::Companion::start(
            &setup.cache,
            &setup.addons,
            setup.session,
            env("__CFBundleIdentifier").unwrap_or_default(),
        )
    })
}

/// Starts the harness with no wrapper and returns its exit code.
fn launch_direct(launch: &wrap::Launch) -> Result<i32, String> {
    std::process::Command::new(&launch.program)
        .args(&launch.args)
        .envs(launch.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .status()
        .map(|s| s.code().unwrap_or(1))
        .map_err(|e| format!("cannot start {}: {e}", launch.program))
}

/// The environment with `--terminal` and `--route` laid over it.
fn overridden(args: &Args) -> impl Fn(&str) -> Option<String> + use<> {
    let overrides = [
        ("TILES_TERMINAL", args.terminal.clone()),
        ("TILES_ROUTE", args.route.clone()),
    ];
    move |k: &str| {
        overrides
            .iter()
            .find(|(o, _)| *o == k)
            .and_then(|(_, v)| v.clone())
            .or_else(|| serve::env(k))
    }
}

/// The usage of `tiles run` with each registered source's `--allow-<name>`.
#[must_use]
pub fn usage(sources: &[&str]) -> String {
    let allow = sources
        .iter()
        .fold("[--allow-omni]".to_string(), |line, s| {
            format!("{line} [--allow-{s}]")
        });
    USAGE.replacen("[--allow-omni]", &allow, 1)
}

/// Runs `tiles run` with the registered `sources`.
pub fn main(args: &[String], sources: &[&dyn Source]) -> ExitCode {
    let names: Vec<&str> = sources.iter().map(|s| s.name()).collect();
    let (args, settings) = match arguments(args, &names) {
        Ok(a) => a,
        Err(code) => return code,
    };
    let mut asked = Asked::start(args.harness, args.dry_run);
    let mut session = Session::start();
    let replies = session.as_mut().map(Session::query);
    let env = overridden(&args);
    let detected = match detect::detect(&env, replies.as_ref()) {
        Ok(d) => d,
        Err(e) => return fail(&e.to_string()),
    };
    let cache = match serve::open_cache().and_then(|_| serve::cache_dir()) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    let tiles = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tiles"));
    let addons = cache.join("run").join(std::process::id().to_string());
    let forms = match tiles_mcp::card::EnginePainter::new() {
        Ok(p) => p.forms(),
        Err(e) => return fail(&e),
    };
    let reach = serve::reach(sources);
    let look_env = look_env(
        replies.as_ref(),
        &env,
        &cache,
        args.harness,
        &detected,
        Screen::now(),
    );
    let setup = Setup {
        tiles: tiles.clone(),
        addons: addons.clone(),
        cache: cache.clone(),
        home: home(),
        detected: detected.clone(),
        reach: reach.clone(),
        allow: args.allow.clone(),
        settings,
        look_env,
        force_wrapper: args.force_wrapper,
        session: session_token(),
        opencode: asked.opencode(),
        motion: crate::motion::setting(&env, crate::motion::system_reduce_motion).0,
        companion: crate::companion::wanted(detected.route, &env),
    };
    let plan = match harness::plan(args.harness, &setup, &args.rest) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let spec = Spec {
        tiles,
        draw: plan.draw,
        companion: setup.companion,
        reach,
        env: harness::server_env(plan.draw, &setup.look_env),
        forms,
    };
    let said = asked.lines(args.harness, setup.opencode);
    if args.dry_run {
        drop(session);
        print_plan(&detected, &plan);
        say(&said, false);
        return ExitCode::SUCCESS;
    }
    if let Err(e) = write_addons(&addons, &spec, &plan) {
        return fail(&e);
    }
    say(&said, session.is_some());
    let code = match (plan.wrap, session) {
        (true, Some(session)) => {
            let motion = wrap::Motion {
                on: harness::animates(&detected),
                each_tick: env("TILES_PRECUT_SEND").as_deref() == Some("tick"),
                background: replies.as_ref().and_then(|r| r.background),
                session: setup.session,
                panel: args.harness.side_panel(),
            };
            wrap::run(session, &plan.launch, detected.route, motion, &cache)
                .map_err(|e| e.to_string())
        }
        (_, session) => {
            drop(session);
            let _window = companion(&setup, &env);
            launch_direct(&plan.launch)
        }
    };
    say(&said, false);
    let _ = std::fs::remove_dir_all(&addons);
    match code {
        Ok(c) => ExitCode::from(u8::try_from(c).unwrap_or(1)),
        Err(e) => fail(&e),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tiles_term::detect::{Detected, Route, Terminal};

    use super::{Screen, look_env, parse};
    use crate::harness::Harness;

    fn rows_for(terminal: Terminal, route: Route) -> Option<String> {
        let detected = Detected {
            terminal,
            multiplexer: None,
            route,
            reason: String::new(),
        };
        let screen = Screen {
            size: (30, 120),
            window: Some((16, 32)),
        };
        let env = |_: &str| None;
        look_env(
            None,
            &env,
            Path::new("/c"),
            Harness::Claude,
            &detected,
            screen,
        )
        .into_iter()
        .find(|(k, _)| k == "TILES_ROWS")
        .map(|(_, v)| v)
    }

    #[test]
    fn only_the_precut_route_gets_the_rows() {
        assert_eq!(
            rows_for(Terminal::Warp, Route::KittyPrecut),
            Some("30".to_string())
        );
        assert_eq!(
            rows_for(Terminal::Foot, Route::Sixel),
            None,
            "a sixel is drawn at its pixel size, so a fitted card would spill past its block"
        );
        assert_eq!(rows_for(Terminal::Kitty, Route::KittyPlaceholders), None);
        assert_eq!(rows_for(Terminal::AppleTerminal, Route::Text), None);
    }

    #[test]
    fn tiles_options_come_before_the_harness_and_the_rest_passes_through() {
        let args: Vec<String> = [
            "--wrapper",
            "--allow-omni",
            "--route",
            "sixel",
            "codex",
            "--route",
            "x",
            "-m",
            "o4",
        ]
        .map(String::from)
        .to_vec();
        let a = parse(&args, &[]).unwrap();
        assert_eq!(a.harness, Harness::Codex);
        assert!(a.force_wrapper);
        assert_eq!(a.allow, ["omni"]);
        assert_eq!(a.route.as_deref(), Some("sixel"));
        assert_eq!(a.rest, ["--route", "x", "-m", "o4"]);
    }

    #[test]
    fn a_missing_or_unknown_harness_is_refused() {
        assert!(parse(&[], &[]).unwrap_err().contains("needs a harness"));
        assert!(
            parse(&["vim".into()], &[])
                .unwrap_err()
                .contains("not a harness")
        );
        assert!(
            parse(&["--terminal".into()], &[])
                .unwrap_err()
                .contains("needs a name")
        );
    }

    #[test]
    fn allow_takes_each_registered_source_and_nothing_else() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let a = parse(
            &args(&["--allow-warehouse", "--allow-omni", "claude"]),
            &["warehouse"],
        )
        .unwrap();
        assert_eq!(a.allow, ["warehouse", "omni"]);
        let e = parse(&args(&["--allow-warehouse", "claude"]), &[]).unwrap_err();
        assert_eq!(
            e,
            "--allow-warehouse names no data source; use --allow-omni"
        );
        let e = parse(&args(&["--allow-\u{1b}[2J", "claude"]), &["warehouse"]).unwrap_err();
        assert!(!e.contains('\u{1b}'), "{e}");
        assert!(e.ends_with("use --allow-omni or --allow-warehouse"), "{e}");
        assert_eq!(
            super::usage(&["warehouse"]),
            "tiles run [--wrapper] [--allow-omni] [--allow-warehouse] [--terminal <name>] [--route <name>] [--dry-run] <pi|opencode|claude|codex> [harness arguments]"
        );
    }
}
