//! The `tiles` command line: `tiles render` draws an intent to a PNG,
//! `tiles forms` lists the chart forms and their roles, `tiles omni` reads
//! Omni topics (ADR-008), `tiles mcp` is the MCP server, `tiles card` draws
//! one card for an add-on, `tiles run` launches a harness with Tiles loaded,
//! `tiles install` adds Tiles to a harness for good, `tiles setup` runs
//! that install for every harness on `PATH` after one question, `tiles doctor`
//! reports the terminal, route, harnesses and Omni setup (ADR-004), and
//! `tiles check` prints a pass or fail line per check of how cards will be
//! drawn here (ADR-025), and `tiles update` installs a newer release
//! (ADR-032).
//!
//! The binary's entry point calls [`main`] with the data sources it
//! registers ([`tiles_mcp::source::Source`], ADR-031): each adds a `tiles
//! <name>` command, a usage line, `tiles doctor` lines and a `tiles run
//! --allow-<name>` flag, and while configured its MCP tools, instruction
//! sentences and rows, so a crate outside this workspace adds a source
//! without editing this one.
//!
//! Every failure prints its message to stderr and exits non-zero: a usage
//! error on one line, a command's refusal whole, wrapped to the terminal.
//! Text that came from the intent reaches the terminal only through
//! [`tiles_intent::one_line`] or [`refusal`], which replace control and
//! bidi characters (ADR-009).

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use tiles_engine::{Engine, Options, Theme};
use tiles_intent::{ARGUMENTS, MAX_PAYLOAD_BYTES, forbidden, one_line};
use tiles_mcp::source::Source;

mod addons;
mod check;
mod companion;
mod copies;
mod doctor;
mod harness;
mod install;
mod motion;
mod omni;
mod opencode;
mod proved;
mod run;
mod serve;
mod setup;
mod update;
mod window;
mod wrap;

const USAGE: &str = "usage: tiles render <intent.json|-> -o <out.png> [--theme <name>] [--width px] [--scale n] [--frames <dir>] [--timings]
       tiles forms
       tiles theme [<name>]
       tiles open <card id>
       tiles cache clear
       tiles omni <topics | fields <topic> | rows <intent.json | ->>
       tiles run [--wrapper] [--allow-omni] [--terminal <name>] [--route <name>] [--dry-run] <pi|opencode|claude|codex> [harness arguments]
       tiles setup [--yes] [--dry-run]
       tiles install [--dry-run] <pi|opencode|claude|codex>
       tiles doctor
       tiles check [--tag <release tag>]
       tiles update [--check] [--allow-older] [vX.Y.Z]
       tiles mcp
       tiles card <intent.json|->
       tiles --version
       tiles --licence";

/// The usage text, with a line for each registered source and its
/// `--allow-<name>` on the `tiles run` line.
fn usage(sources: &[&dyn Source]) -> String {
    let names: Vec<&str> = sources.iter().map(|s| s.name()).collect();
    let mut text = USAGE.replacen(run::USAGE, &run::usage(&names), 1);
    for source in sources {
        text.push_str("\n       ");
        text.push_str(&one_line(source.usage()));
    }
    text
}

/// What `tiles --licence` prints: one line each for the code, the design
/// assets and the Inter font (ADR-036). A build with a design pack carries it
/// under the Tiles Design Licence; a build from source draws the plain
/// theme, which is part of the code (ADR-040).
fn licence_lines() -> String {
    let design = match tiles_engine::SEAL_SOURCE.strip_prefix("pack ") {
        Some(n) => format!("Design pack {n}: the Tiles Design Licence (LICENSE-design)"),
        None => "Design assets (built from source): the plain theme, under the code's licence"
            .to_string(),
    };
    format!(
        "Code: FSL-1.1-ALv2, the Functional Source License (LICENSE)\n\
         {design}\n\
         Inter font: the SIL Open Font License 1.1"
    )
}

fn version_line() -> String {
    format!(
        "tiles {} (intent schema {})",
        tiles_intent::TILES_VERSION,
        tiles_intent::SCHEMA_VERSION
    )
}

/// A parsed `tiles render` command.
#[derive(Debug, PartialEq)]
struct Render {
    input: String,
    output: PathBuf,
    opts: Options,
    /// Whether `--theme` chose the theme; without it the settings decide.
    themed: bool,
    scale: f32,
    frames: Option<PathBuf>,
    timings: bool,
}

#[derive(Debug, PartialEq)]
enum Command {
    Render(Render),
    Forms,
    Omni(Vec<String>),
    Run(Vec<String>),
    Install(Vec<String>),
    Setup(Vec<String>),
    Doctor,
    Check(Option<String>),
    Update(Vec<String>),
    Mcp,
    Card(String),
    Theme(Option<String>),
    Open(String),
    CacheClear,
    /// A registered source's command: its index and the arguments after
    /// its name.
    Source(usize, Vec<String>),
    Version,
    Licence,
    Help,
}

#[cfg(test)]
fn parse_args(args: &[String]) -> Result<Command, String> {
    parse(args, &[])
}

/// Parses the command line. A word no built-in command uses runs the
/// source of that name, the index of its name in `sources`.
fn parse(args: &[String], sources: &[&str]) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Ok(Command::Help);
    };
    match first.as_str() {
        "--version" | "-V" | "version" => Ok(Command::Version),
        "--licence" if args.len() == 1 => Ok(Command::Licence),
        "--licence" => Err("tiles --licence takes no arguments".into()),
        "--help" | "-h" | "help" => Ok(Command::Help),
        "forms" if args.len() == 1 => Ok(Command::Forms),
        "forms" => Err("tiles forms takes no arguments".into()),
        "render" => parse_render(&args[1..]).map(Command::Render),
        "omni" => Ok(Command::Omni(args[1..].to_vec())),
        "run" => Ok(Command::Run(args[1..].to_vec())),
        "install" => Ok(Command::Install(args[1..].to_vec())),
        "setup" => Ok(Command::Setup(args[1..].to_vec())),
        "update" => Ok(Command::Update(args[1..].to_vec())),
        "doctor" if args.len() == 1 => Ok(Command::Doctor),
        "check" if args.len() == 1 => Ok(Command::Check(None)),
        "check" if args.len() == 3 && args[1] == "--tag" => {
            Ok(Command::Check(Some(args[2].clone())))
        }
        "mcp" if args.len() == 1 => Ok(Command::Mcp),
        "card" if args.len() == 2 => Ok(Command::Card(args[1].clone())),
        "theme" if args.len() <= 2 => Ok(Command::Theme(args.get(1).cloned())),
        "open" if args.len() == 2 => Ok(Command::Open(args[1].clone())),
        "cache" if args.len() == 2 && args[1] == "clear" => Ok(Command::CacheClear),
        "cache" => Err(format!("usage: {}", serve::CACHE_USAGE)),
        "doctor" | "check" | "mcp" | "card" | "theme" | "open" => {
            Err(format!("wrong arguments for {first}"))
        }
        other => {
            if let Some(i) = sources.iter().position(|name| *name == other) {
                return Ok(Command::Source(i, args[1..].to_vec()));
            }
            let mut names = vec![
                "render", "forms", "theme", "open", "cache", "omni", "run", "setup", "install",
                "doctor", "check", "update", "mcp", "card",
            ];
            names.extend(sources);
            let last = names.pop().unwrap_or_default();
            Err(format!(
                "unknown command \"{}\"; use {} or {last}",
                one_line(other),
                names.join(", ")
            ))
        }
    }
}

fn parse_render(args: &[String]) -> Result<Render, String> {
    let mut input = None;
    let mut output = None;
    let mut opts = Options::default();
    let mut themed = false;
    let mut scale = 2.0;
    let mut frames = None;
    let mut timings = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match a.as_str() {
            "-o" | "--out" => output = Some(PathBuf::from(value("-o")?)),
            "--theme" => {
                let name = value("--theme")?;
                themed = true;
                opts.theme = Theme::from_name(&name).ok_or_else(|| {
                    let names: Vec<&str> = Theme::all().iter().map(|t| t.name()).collect();
                    format!(
                        "--theme \"{}\" is not a theme; use {}",
                        one_line(&name),
                        names.join(", ")
                    )
                })?;
            }
            "--width" => opts.width = number("--width", &value("--width")?)?,
            "--scale" => scale = number("--scale", &value("--scale")?)?,
            "--frames" => frames = Some(PathBuf::from(value("--frames")?)),
            "--timings" => timings = true,
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option \"{}\"", one_line(flag)));
            }
            path => {
                if input.replace(path.to_string()).is_some() {
                    return Err("render takes one intent file".into());
                }
            }
        }
    }
    Ok(Render {
        input: input.ok_or("render needs an intent file, or - for stdin")?,
        output: output.ok_or("render needs -o <out.png>")?,
        opts,
        themed,
        scale,
        frames,
        timings,
    })
}

fn number(name: &str, v: &str) -> Result<f32, String> {
    v.parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{name} \"{}\" is not a number", one_line(v)))
}

/// Reads the intent, refusing anything over the payload limit before it is
/// all in memory.
pub(crate) fn read_input(input: &str) -> Result<String, String> {
    let limit = u64::try_from(MAX_PAYLOAD_BYTES).unwrap_or(u64::MAX) + 1;
    let mut bytes = Vec::new();
    let read = if input == "-" {
        std::io::stdin().take(limit).read_to_end(&mut bytes)
    } else {
        std::fs::File::open(input)
            .map_err(|e| format!("cannot open {}: {e}", one_line(input)))?
            .take(limit)
            .read_to_end(&mut bytes)
    };
    read.map_err(|e| format!("cannot read {}: {e}", one_line(input)))?;
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(format!(
            "{ARGUMENTS}: larger than the {MAX_PAYLOAD_BYTES} byte limit; send fewer rows"
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| format!("{ARGUMENTS}: not UTF-8 text; save the file as UTF-8"))
}

/// Writes through a temporary file in the target directory and renames it
/// into place, so a reader never sees half a PNG and a planted symlink at
/// the temporary name is never followed (ADR-009).
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let name = path.file_name().ok_or_else(|| {
        format!(
            "{} is not a file path",
            one_line(&path.display().to_string())
        )
    })?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("cannot write {}: {e}", one_line(&tmp.display().to_string())))?;
    let result = f.write_all(bytes).and_then(|()| f.sync_all());
    drop(f);
    if let Err(e) = result.and_then(|()| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "cannot write {}: {e}",
            one_line(&path.display().to_string())
        ));
    }
    Ok(())
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn render(r: &Render, sources: &[&dyn Source]) -> Result<String, String> {
    let json = read_input(&r.input)?;
    let t0 = Instant::now();
    let engine = Engine::new().map_err(|e| e.to_string())?;
    let load = ms(t0);
    let request = tiles_intent::parse_request(&json).map_err(|e| e.to_string())?;
    let mut fetched_notes = Vec::new();
    let intent = if let Some(source) = request.omni() {
        let client = tiles_omni::OmniConfig::from_system()
            .map(tiles_omni::OmniClient::new)
            .map_err(|e| format!("omni: {e}"))?;
        let table = client.query(source).map_err(|e| format!("omni: {e}"))?;
        request.with_table(&table)
    } else if let Some(source) = request.warehouse() {
        let offered = serve::offered(sources);
        let fetched = serve::fetch_warehouse(&offered, source, Instant::now())
            .map_err(|e| format!("warehouse: {e}"))?;
        fetched_notes = fetched.notes;
        request.with_table(&fetched.table)
    } else {
        request.into_intent()
    }
    .map_err(|e| e.to_string())?;
    for note in &fetched_notes {
        eprintln!("tiles: {}", one_line(note));
    }
    for note in &intent.notes {
        eprintln!("tiles: {}", one_line(note));
    }
    let mut opts = r.opts;
    if !r.themed {
        opts.theme = tiles_mcp::settings::theme(&serve::env).0;
    }
    let t1 = Instant::now();
    let scene = engine.scene(&intent, &opts).map_err(|e| e.to_string())?;
    let png = engine.png(&scene, r.scale).map_err(|e| e.to_string())?;
    let card = ms(t1);
    write_atomic(&r.output, &png)?;
    let mut report = format!(
        "{} ({} form, {} theme, {}x{} px)",
        one_line(&r.output.display().to_string()),
        scene.form,
        scene.theme.name(),
        (scene.width * r.scale).round(),
        (scene.height * r.scale).round()
    );
    if let Some(dir) = &r.frames {
        std::fs::create_dir_all(dir).map_err(|e| {
            format!(
                "cannot create {}: {e}",
                one_line(&dir.display().to_string())
            )
        })?;
        let t2 = Instant::now();
        let mut count = 0;
        for (i, f) in engine
            .frames(&scene, r.scale)
            .map_err(|e| e.to_string())?
            .enumerate()
        {
            let f = f.map_err(|e| e.to_string())?;
            let png = tiles_engine::frame_png(&f).ok_or("engine: Skia could not encode a frame")?;
            write_atomic(&dir.join(format!("frame-{i:02}.png")), &png)?;
            count += 1;
        }
        let frame_ms = ms(t2);
        let _ = write!(report, ", {count} frames");
        if r.timings {
            let _ = write!(
                report,
                "; {count} frames drawn and written in {frame_ms:.1} ms"
            );
        }
    }
    if r.timings {
        let _ = write!(report, "; engine load {load:.1} ms, card {card:.1} ms");
    }
    Ok(report)
}

fn forms() -> Result<String, String> {
    let engine = Engine::new().map_err(|e| e.to_string())?;
    let mut out = String::new();
    for f in engine.forms() {
        let _ = writeln!(out, "{:<12} {}", f.name, f.summary);
        for r in &f.roles {
            let need = if r.required { "required" } else { "optional" };
            let _ = writeln!(
                out,
                "{:<12}   {:<7} {:?}, {need}: {}",
                "", r.name, r.kind, r.doc
            );
        }
    }
    Ok(out.trim_end().to_string())
}

/// `tiles <name> ...` for a registered source: its output with only line
/// breaks kept, or its error whole, as [`refusal`] cleans it. A command that leaves the
/// source configured where it was not, such as its setup, gives the
/// harness installs Tiles made under `dirs` the approval settings an
/// install with it writes ([`install::reapply_approvals`]), and says so.
fn source_command(
    source: &dyn Source,
    args: &[String],
    dirs: Option<&install::Dirs>,
) -> Result<String, String> {
    let before = source.offer().is_some();
    let mut text = source
        .command(args)
        .map(|text| tiles_term::sanitize::lines(&text))
        .map_err(|e| clean(&e))?;
    if let (false, Some(offer), Some(dirs)) = (before, source.offer(), dirs) {
        let tools: Vec<&str> = offer.tools.iter().map(|t| t.name).collect();
        let timeout = Some(harness::wait_secs(offer.tool_timeout));
        for line in install::reapply_approvals(dirs, &tools, timeout) {
            text.push('\n');
            text.push_str(&one_line(&line));
        }
        if install::pi_extension(&dirs.home).join("tool.json").exists() {
            let _ = write!(
                text,
                "\nPi's installed Tiles extension predates this and has no {} tools; run tiles install pi to add them",
                offer.name
            );
        }
    }
    Ok(text)
}

/// The registered sources' `tiles doctor` lines, each made one plain line.
fn source_lines(sources: &[&dyn Source]) -> Vec<String> {
    sources
        .iter()
        .flat_map(|s| s.doctor())
        .map(|line| tiles_term::sanitize::line(&line, 200))
        .collect()
}

/// Readers of each harness's `--version` but `OpenCode`'s, which
/// [`opencode::Found::read`] asks, one thread each.
type Asking = Vec<(harness::Harness, std::thread::JoinHandle<Option<String>>)>;

/// Starts asking every harness on `PATH` but `OpenCode` for its version.
fn ask_versions(which: &dyn Fn(&str) -> Option<PathBuf>) -> Asking {
    harness::Harness::ALL
        .into_iter()
        .filter(|h| *h != harness::Harness::OpenCode)
        .filter_map(|h| {
            let path = which(h.name())?;
            Some((h, std::thread::spawn(move || proved::read(&path))))
        })
        .collect()
}

/// Each harness on `PATH` with the version it named.
fn versions(
    asking: Asking,
    opencode: Option<&opencode::Found>,
) -> Vec<(harness::Harness, Option<String>)> {
    let mut out: Vec<_> = asking
        .into_iter()
        .map(|(h, t)| (h, t.join().ok().flatten()))
        .collect();
    out.extend(opencode.map(|f| (harness::Harness::OpenCode, f.version.clone())));
    out
}

/// Asks the terminal `tiles check` and `tiles doctor` describe
/// ([`wrap::Probe`]) and lets it go: its answers, its size and window cell,
/// and why it was not asked.
fn ask_terminal() -> (
    Option<tiles_term::detect::Replies>,
    run::Screen,
    Option<String>,
) {
    let mut probe = wrap::Probe::open();
    let replies = probe.as_mut().ok().map(wrap::Probe::query);
    let screen = run::Screen::of(probe.as_ref().ok());
    (replies, screen, probe.err())
}

fn doctor_main(sources: &[&dyn Source]) -> String {
    let (replies, screen, not_asked) = ask_terminal();
    let asking = ask_versions(&which);
    let dirs = install::Dirs::from_env(&serve::env);
    let setup = dirs
        .as_ref()
        .map(|d| setup::doctor_hints(d, sources))
        .unwrap_or_default();
    let opencode = which("opencode").map(|p| opencode::Found::read(&p));
    let versions = versions(asking, opencode.as_ref());
    let update = update::refresh(
        &serve::env,
        tiles_intent::TILES_VERSION,
        update::now(),
        &update::fetch_latest,
    );
    let reach = serve::reach(sources);
    let mut lines = doctor::report(&doctor::Inputs {
        env: &serve::env,
        replies: replies.as_ref(),
        not_asked: not_asked.as_deref(),
        window: screen.window,
        which: &which,
        secrets: &tiles_omni::Keychain,
        cache: serve::cache_dir(),
        dirs,
        opencode: opencode.as_ref(),
        reduce_motion: motion::system_reduce_motion(),
        versions: &versions,
        update: update.as_ref(),
        data_tools: reach
            .offers
            .iter()
            .flat_map(|o| o.tools.iter().map(|t| t.name))
            .collect(),
        codex_timeout: reach.tool_timeout().map(harness::wait_secs),
        network: sources.iter().filter_map(|s| s.network()).collect(),
        setup,
        copies: copies::here().doctor(),
    });
    lines.extend(source_lines(sources));
    lines.join("\n")
}

/// Prints one line per check and fails when any check fails.
fn check_main(tag: Option<&str>) -> ExitCode {
    let (replies, screen, not_asked) = ask_terminal();
    let asking = ask_versions(&which);
    let opencode = which("opencode").map(|p| opencode::Found::read(&p));
    let versions = versions(asking, opencode.as_ref());
    let lines = check::report(&check::Inputs {
        env: &serve::env,
        replies: replies.as_ref(),
        not_asked: not_asked.as_deref(),
        screen,
        which: &which,
        cache: serve::cache_dir(),
        tag,
        opencode: opencode.as_ref(),
        opencode_install: install::Dirs::from_env(&serve::env).and_then(|d| d.opencode_install()),
        reduce_motion: motion::system_reduce_motion(),
        versions: &versions,
    });
    for l in &lines {
        println!("{l}");
    }
    if lines.iter().all(|l| l.starts_with("pass ")) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn install_main(args: &[String], sources: &[&dyn Source]) -> ExitCode {
    let dry_run = args.first().is_some_and(|a| a == "--dry-run");
    let rest = if dry_run { &args[1..] } else { args };
    let [name] = rest else {
        eprintln!("usage: tiles install [--dry-run] <pi|opencode|claude|codex>");
        return ExitCode::from(2);
    };
    let Some(harness) = harness::Harness::from_name(name) else {
        eprintln!(
            "tiles: \"{}\" is not a harness; use pi, opencode, claude or codex",
            one_line(name)
        );
        return ExitCode::from(2);
    };
    let Some(dirs) = install::Dirs::from_env(&serve::env) else {
        eprintln!("tiles: HOME is not set");
        return ExitCode::FAILURE;
    };
    let forms = match engine_forms() {
        Ok(forms) => forms,
        Err(e) => {
            eprintln!("tiles: {}", one_line(&e));
            return ExitCode::FAILURE;
        }
    };
    let spec = install::spec(harness, this_tiles(), serve::reach(sources), forms);
    if let Err(e) = install::refusal(harness, &dirs) {
        eprintln!("tiles: {}", one_line(&e));
        return ExitCode::FAILURE;
    }
    let steps = install::steps(
        harness,
        &dirs.home,
        &dirs.config_home,
        &dirs.codex_home,
        &spec,
    );
    for step in &steps {
        println!("{}", one_line(&step.describe()));
    }
    if dry_run {
        return ExitCode::SUCCESS;
    }
    match install::apply(&steps, &install::run_program) {
        Ok(()) => {
            println!("{}", install_done(harness));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("tiles: {}", one_line(&e));
            ExitCode::FAILURE
        }
    }
}

/// The line after a harness is installed.
fn install_done(harness: harness::Harness) -> String {
    format!(
        "installed Tiles for {}; tiles run {} adds the inline card",
        harness.name(),
        harness.name()
    )
}

/// The engine's forms, which the add-ons list.
fn engine_forms() -> Result<Vec<tiles_intent::schema::FormEntry>, String> {
    use tiles_mcp::card::Painter as _;
    tiles_mcp::card::EnginePainter::new().map(|p| p.forms())
}

/// Where `name` is on `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

/// This `tiles`, which the installed add-ons run.
fn this_tiles() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tiles"))
}

/// The `tiles` command with `sources` registered: reads the command line,
/// runs the command and returns its exit code.
#[must_use]
pub fn main(sources: &[&dyn Source]) -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let names: Vec<&str> = sources.iter().map(|s| s.name()).collect();
    let result = match parse(&args, &names) {
        Ok(Command::Version) => Ok(version_line()),
        Ok(Command::Licence) => Ok(licence_lines()),
        Ok(Command::Help) => Ok(usage(sources)),
        Ok(Command::Forms) => forms(),
        Ok(Command::Render(r)) => render(&r, sources),
        Ok(Command::Omni(rest)) => return omni::main(&rest),
        Ok(Command::Run(rest)) => return run::main(&rest, sources),
        Ok(Command::Install(rest)) => return install_main(&rest, sources),
        Ok(Command::Setup(rest)) => return setup::main(&rest, sources),
        Ok(Command::Doctor) => Ok(doctor_main(sources)),
        Ok(Command::Check(tag)) => return check_main(tag.as_deref()),
        Ok(Command::Update(rest)) => return update::main(&rest),
        Ok(Command::Mcp) => return serve::mcp(sources),
        Ok(Command::Card(input)) => return serve::card(&input, sources),
        Ok(Command::Theme(name)) => serve::theme(name.as_deref()),
        Ok(Command::Open(id)) => serve::open(&id, sources),
        Ok(Command::CacheClear) => serve::cache_clear(),
        Ok(Command::Source(i, rest)) => source_command(
            sources[i],
            &rest,
            install::Dirs::from_env(&serve::env).as_ref(),
        ),
        Err(e) => {
            eprintln!("tiles: {e}");
            eprintln!("{}", usage(sources));
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            let cols = rustix::termios::tcgetwinsize(rustix::stdio::stderr())
                .ok()
                .map(|w| usize::from(w.ws_col));
            eprintln!("{}", refusal(&e, cols));
            ExitCode::FAILURE
        }
    }
}

/// The longest refusal Tiles prints, in characters: far past any of its
/// own, so only a runaway message is cut.
const REFUSAL_CHARS: usize = 4_000;

/// `text` cleaned as [`one_line`] cleans it, but cut only past
/// [`REFUSAL_CHARS`].
fn clean(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| if forbidden(c) { '?' } else { c })
        .take(REFUSAL_CHARS)
        .collect();
    if text.chars().count() > REFUSAL_CHARS {
        out.push('…');
    }
    out
}

/// A command's refusal for standard error: `tiles: ` and the whole text,
/// cleaned, and when `cols` gives the terminal's width, wrapped at spaces
/// to it, each later line indented under the first's text. A span in
/// backticks, such as a command or a path, is one unbreakable word, so a
/// copied command or path never holds a line break; a word longer than a
/// line keeps a line of its own whole. A backtick that no other closes
/// opens no span.
fn refusal(text: &str, cols: Option<usize>) -> String {
    const LEAD: &str = "tiles: ";
    let text = clean(text);
    let Some(cols) = cols.filter(|c| *c > LEAD.len() + 20) else {
        return format!("{LEAD}{text}");
    };
    let room = cols - LEAD.len();
    let open = |w: &str| w.matches('`').count() % 2 == 1;
    let mut words: Vec<String> = Vec::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        match words.last_mut() {
            Some(span) if open(span) => {
                span.push(' ');
                span.push_str(word);
            }
            _ => words.push(word.to_string()),
        }
    }
    if let Some(span) = words.pop_if(|w| open(w)) {
        words.extend(span.split(' ').map(str::to_string));
    }
    let mut lines: Vec<String> = vec![String::new()];
    for word in &words {
        let line = lines.last_mut().expect("a line");
        let used = line.chars().count();
        if used == 0 {
            line.push_str(word);
        } else if used + 1 + word.chars().count() <= room {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(word.clone());
        }
    }
    let indent = " ".repeat(LEAD.len());
    let mut out = String::from(LEAD);
    out.push_str(&lines.join(&format!("\n{indent}")));
    out
}

#[cfg(test)]
mod tests;
