//! `tiles setup [--yes] [--dry-run]`: one command that sets Tiles up on this
//! Mac (ADR-035). It finds each harness Tiles supports on `PATH`, the
//! terminal, and the data logins it can see without reading or copying a
//! secret, prints one list of what it would change, and asks once, no by
//! default. On yes it runs the steps `tiles install` runs for each harness
//! not yet set up, then prints the commands that finish each data source it
//! found. A harness whose install already matches what `tiles install` would
//! write is left as it is, so a second run changes nothing. A Claude Code
//! whose user-scope `tiles` server runs another `tiles` has it removed and
//! this one added, listed with the other steps before the question
//! (ADR-039). A server in a project's or a folder's config, one that sets a
//! variable of the user's, or one that lacks `mcp` and the draw variable, is
//! reported with the commands that replace it, and left as it is. Each
//! other `tiles` on `PATH` is named with its version, and an older one ahead
//! of this one that the user owns is offered to be moved aside after a
//! question of its own, in a terminal only ([`crate::copies`]). `--yes` answers for a script;
//! `--dry-run` prints the plan and changes nothing. `tiles doctor` gets a
//! hint from the same plan, reading Claude Code's registration from
//! `~/.claude.json` so it starts no server.

use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tiles_intent::schema::FormEntry;
use tiles_mcp::source::{Found, Reach, Source};
use tiles_omni::{BaseUrl, ENV_API_KEY, ENV_URL, KEYCHAIN_URL, SecretStore, keychain_key_account};
use tiles_term::sanitize;

use crate::addons::Spec;
use crate::copies::{Copies, Other};
use crate::harness::Harness;
use crate::install::{self, Dirs, Step};
use crate::proved::title;

/// Usage of `tiles setup`.
pub const USAGE: &str = "tiles setup [--yes] [--dry-run]";

/// The longest `claude mcp get tiles` may take; it starts the server it
/// finds to check it.
const CLAUDE_PATIENCE: Duration = Duration::from_secs(20);

/// A parsed `tiles setup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Args {
    /// `--yes`: change without asking.
    pub yes: bool,
    /// `--dry-run`: print the plan and change nothing.
    pub dry_run: bool,
}

/// Parses the arguments after `tiles setup`.
///
/// # Errors
///
/// For an argument it does not take.
pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    for a in args {
        match a.as_str() {
            "--yes" => out.yes = true,
            "--dry-run" => out.dry_run = true,
            other => {
                return Err(format!(
                    "tiles setup does not take \"{}\"; usage: {USAGE}",
                    sanitize::line(other, 40)
                ));
            }
        }
    }
    Ok(out)
}

/// What setup does for one harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Not on `PATH`.
    Missing,
    /// On `PATH` here, with Tiles set up as `tiles install` would set it up.
    Current(PathBuf),
    /// On `PATH` here, with the install steps that set Tiles up.
    Install(PathBuf, Vec<Step>),
    /// On `PATH` here, but an install would overwrite a setting of the
    /// user's; why, and what to do instead.
    Refused(PathBuf, String),
    /// On `PATH` here, with a `tiles` server `tiles install` would not
    /// add and setup leaves; what it runs instead, and what replaces it.
    Other(PathBuf, String),
    /// On `PATH` here, with a user-scope `tiles` server for another `tiles`;
    /// what it runs, and the steps that remove it and add this one.
    Replace(PathBuf, String, Vec<Step>),
}

/// What a harness's plan is read from.
pub struct Inputs<'a> {
    /// Where a program is on `PATH`.
    pub which: &'a dyn Fn(&str) -> Option<PathBuf>,
    /// Where the harnesses keep their config.
    pub dirs: &'a Dirs,
    /// The add-on spec an install of a harness writes.
    pub spec: &'a dyn Fn(Harness) -> Spec,
    /// The `tiles` server the Claude Code at this path has, `None` when it
    /// has none.
    pub claude_tiles: &'a dyn Fn(&Path) -> Option<Registered>,
}

/// A `tiles` MCP server registered in Claude Code: its command, its
/// arguments, and its environment as `KEY=value`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registered {
    /// The config Claude Code keeps it in, `user`, `project` or `local`;
    /// `None` when not said.
    pub scope: Option<String>,
    /// What it runs; `None` for a server that runs no command.
    pub command: Option<String>,
    /// Its arguments.
    pub args: Vec<String>,
    /// Its environment, one `KEY=value` each.
    pub env: Vec<String>,
}

impl Registered {
    /// The server in `answer`, `claude mcp get tiles`'s output.
    #[must_use]
    pub fn printed(answer: &str) -> Self {
        let mut out = Self::default();
        let mut env = false;
        for line in answer.lines() {
            match line.strip_prefix("    ") {
                Some(var) if env => out.env.push(var.to_string()),
                _ => {
                    env = line == "  Environment:";
                    if let Some(s) = line.strip_prefix("  Scope: ") {
                        out.scope = s.split_whitespace().next().map(str::to_ascii_lowercase);
                    }
                    if let Some(c) = line.strip_prefix("  Command: ") {
                        out.command = Some(c.to_string());
                    }
                    if let Some(a) = line.strip_prefix("  Args: ") {
                        out.args = a.split_whitespace().map(str::to_string).collect();
                    }
                }
            }
        }
        out
    }

    /// The user-scope server in `text`, Claude Code's `~/.claude.json`:
    /// `mcpServers.tiles`, `Ok(None)` when there is none, `Err` for a file
    /// of another shape.
    fn configured(text: &str) -> Result<Option<Self>, ()> {
        let config: serde_json::Value = serde_json::from_str(text).map_err(|_| ())?;
        let config = config.as_object().ok_or(())?;
        let Some(servers) = config.get("mcpServers") else {
            return Ok(None);
        };
        let Some(tiles) = servers.as_object().ok_or(())?.get("tiles") else {
            return Ok(None);
        };
        let tiles = tiles.as_object().ok_or(())?;
        let text = |v: &serde_json::Value| v.as_str().map(str::to_string).ok_or(());
        let command = tiles.get("command").map(text).transpose()?;
        let args = match tiles.get("args") {
            None => Vec::new(),
            Some(a) => a
                .as_array()
                .ok_or(())?
                .iter()
                .map(text)
                .collect::<Result<_, _>>()?,
        };
        let env = match tiles.get("env") {
            None => Vec::new(),
            Some(e) => e
                .as_object()
                .ok_or(())?
                .iter()
                .map(|(k, v)| text(v).map(|v| format!("{k}={v}")))
                .collect::<Result<_, _>>()?,
        };
        Ok(Some(Self {
            scope: Some("user".into()),
            command,
            args,
            env,
        }))
    }
}

/// The plan for `harness`.
#[must_use]
pub fn plan(harness: Harness, i: &Inputs<'_>) -> Plan {
    let Some(path) = (i.which)(harness.name()) else {
        return Plan::Missing;
    };
    let spec = (i.spec)(harness);
    let steps = install::steps(
        harness,
        &i.dirs.home,
        &i.dirs.config_home,
        &i.dirs.codex_home,
        &spec,
    );
    if harness == Harness::Claude
        && let Some(registered) = (i.claude_tiles)(&path)
    {
        return claude_plan(path, &registered, &spec, steps);
    }
    if steps.iter().all(|s| current(s, &spec, i)) {
        return Plan::Current(path);
    }
    match install::refusal(harness, i.dirs) {
        Ok(()) => Plan::Install(path, steps),
        Err(why) => Plan::Refused(path, why),
    }
}

/// Whether `step` would change nothing: the file already holds those
/// bytes, Codex already asks and waits long enough, or Codex already has
/// the server.
fn current(step: &Step, spec: &Spec, i: &Inputs<'_>) -> bool {
    match step {
        Step::Write(path, bytes) => fs::read(path).is_ok_and(|b| b == *bytes),
        Step::CodexPrompt(path) => fs::read_to_string(path)
            .is_ok_and(|c| install::codex_prompt(&c).is_ok_and(|edited| edited == c)),
        Step::CodexTimeout(path, secs) => fs::read_to_string(path)
            .is_ok_and(|c| install::codex_timeout(&c, *secs).is_ok_and(|edited| edited == c)),
        Step::Run(program, _) if program == "codex" => codex_has_tiles(&i.dirs.codex_file(), spec),
        Step::Run(..) => false,
    }
}

/// What `registered` runs when it is not what `claude mcp add` writes for
/// `spec`: this `tiles`, `mcp`, and the draw variable among its
/// environment. `None` when it is.
fn claude_other(registered: &Registered, spec: &Spec) -> Option<String> {
    let tiles = spec.tiles.display().to_string();
    let draw = format!("TILES_DRAW={}", spec.draw.name());
    match registered.command.as_deref() {
        None => Some("set up with a tiles server that runs no command".into()),
        Some(c) if c != tiles => Some(format!("set up with another tiles, {c}")),
        Some(_) if registered.args != ["mcp"] || !registered.env.contains(&draw) => Some(format!(
            "set up with this tiles, but not with mcp and {draw}"
        )),
        Some(_) => None,
    }
}

/// What replaces a Claude Code `tiles` server `tiles setup` leaves.
const CLAUDE_FIX: &str = "run claude mcp remove tiles --scope user, then tiles setup";

/// The step that removes Claude Code's user-scope `tiles` server.
fn claude_remove() -> Step {
    Step::Run(
        "claude".into(),
        ["mcp", "remove", "tiles", "--scope", "user"]
            .map(String::from)
            .to_vec(),
    )
}

/// The plan for the Claude Code at `path` with `registered` as its `tiles`
/// server, where `steps` add the one `spec` describes. A user-scope server
/// for another `tiles` is replaced, unless it holds a variable of the
/// user's that `claude mcp add` would not write back; a server in a
/// project's or a folder's config is never touched.
fn claude_plan(path: PathBuf, registered: &Registered, spec: &Spec, steps: Vec<Step>) -> Plan {
    let Some(other) = claude_other(registered, spec) else {
        return Plan::Current(path);
    };
    let tiles = spec.tiles.display().to_string();
    let another = registered.command.as_deref().is_some_and(|c| c != tiles);
    let theirs = registered.env.iter().any(|v| !v.starts_with("TILES_DRAW="));
    match registered.scope.as_deref() {
        Some("user") if another && !theirs => {
            Plan::Replace(path, other, [vec![claude_remove()], steps].concat())
        }
        Some("user") if another => Plan::Other(
            path,
            format!("{other}, with variables of yours; {CLAUDE_FIX}"),
        ),
        Some(s @ ("project" | "local")) => Plan::Other(
            path,
            format!(
                "{other}, in Claude Code's {s} config; Tiles leaves it: run claude mcp remove tiles --scope {s}, then tiles setup"
            ),
        ),
        _ => Plan::Other(path, format!("{other}; {CLAUDE_FIX}")),
    }
}

/// Carries out one harness's steps through `run`, as [`install::apply`]
/// does. When they start by removing Claude Code's old `tiles` server, a
/// failed removal stops before the add, so Claude Code keeps the server it
/// had, and the error says what to run by hand.
///
/// # Errors
///
/// The first step that failed.
pub fn apply(steps: &[Step], run: &install::RunCommand<'_>) -> Result<(), String> {
    match steps.split_first() {
        Some((first, rest)) if *first == claude_remove() => {
            install::apply(std::slice::from_ref(first), run).map_err(|e| {
                format!("{e}; Claude Code keeps its old tiles server: {CLAUDE_FIX}")
            })?;
            install::apply(rest, run)
                .map_err(|e| format!("{e}; the old tiles server is removed: run tiles setup again"))
        }
        _ => install::apply(steps, run),
    }
}

/// Whether Codex's config has the tiles server `codex mcp add` writes for
/// `spec`: this `tiles`, `mcp`, and the draw variable.
fn codex_has_tiles(file: &Path, spec: &Spec) -> bool {
    let Some(config) = fs::read_to_string(file)
        .ok()
        .and_then(|c| c.parse::<toml::Table>().ok())
    else {
        return false;
    };
    let Some(tiles) = config.get("mcp_servers").and_then(|s| s.get("tiles")) else {
        return false;
    };
    let tiles_path = spec.tiles.display().to_string();
    tiles.get("command").and_then(toml::Value::as_str) == Some(tiles_path.as_str())
        && tiles
            .get("args")
            .and_then(toml::Value::as_array)
            .is_some_and(|a| a.len() == 1 && a[0].as_str() == Some("mcp"))
        && tiles
            .get("env")
            .and_then(|e| e.get("TILES_DRAW"))
            .and_then(toml::Value::as_str)
            == Some(spec.draw.name())
}

/// The `tiles` MCP server of the Claude Code at `claude`, by its own
/// `claude mcp get tiles`, which owns its config; `None` when it has none.
#[must_use]
pub fn claude_tiles(claude: &Path) -> Option<Registered> {
    crate::opencode::ask_with(claude, &["mcp", "get", "tiles"], CLAUDE_PATIENCE)
        .map(|answer| Registered::printed(&answer))
}

/// The user-scope `tiles` server in Claude Code's `~/.claude.json` under
/// `home`, read without starting anything: `Ok(None)` when it has none,
/// `Err` when the file is missing, unreadable or of another shape.
fn claude_configured(home: &Path) -> Result<Option<Registered>, ()> {
    let text = fs::read_to_string(home.join(".claude.json")).map_err(|_| ())?;
    Registered::configured(&text)
}

/// What setup reports of Omni, and whether Omni is set up, read without
/// reading the API key: the URL from `TILES_OMNI_URL` or the Keychain,
/// which is no secret, whether `TILES_OMNI_API_KEY` is set (`present` says
/// so without giving its value), and whether the Keychain holds a key item
/// for the URL's host.
pub fn omni(
    env: &dyn Fn(&str) -> Option<String>,
    present: &dyn Fn(&str) -> bool,
    secrets: &dyn SecretStore,
) -> (Found, bool) {
    let found = omni_found(env, present, secrets);
    let set_up = found.line.starts_with("omni: set up for ");
    (found, set_up)
}

fn omni_found(
    env: &dyn Fn(&str) -> Option<String>,
    present: &dyn Fn(&str) -> bool,
    secrets: &dyn SecretStore,
) -> Found {
    let clean = |s: &str| sanitize::line(s, 200);
    let found = |line: String, next: Vec<String>| Found { line, next };
    let set_key = |base: &BaseUrl| format!("tiles omni set-key https://{}", base.authority());
    if let Some(url) = env(ENV_URL).filter(|v| !v.trim().is_empty()) {
        return match BaseUrl::parse(&url) {
            Err(e) => found(
                format!("omni: {ENV_URL} is set, but {}", clean(&e.to_string())),
                Vec::new(),
            ),
            Ok(base) if present(ENV_API_KEY) => found(
                format!(
                    "omni: set up for {}, in the environment",
                    clean(base.authority())
                ),
                Vec::new(),
            ),
            Ok(base) => found(
                format!(
                    "omni: {ENV_URL} names {} but {ENV_API_KEY} is not set",
                    clean(base.authority())
                ),
                vec![format!(
                    "set {ENV_API_KEY} beside {ENV_URL}, or remove {ENV_URL} and run {}",
                    set_key(&base)
                )],
            ),
        };
    }
    if present(ENV_API_KEY) {
        return found(
            format!("omni: {ENV_API_KEY} is set without {ENV_URL}"),
            vec![format!(
                "set {ENV_URL} to your Omni URL beside {ENV_API_KEY}"
            )],
        );
    }
    let stored = match secrets.get(KEYCHAIN_URL) {
        Ok(url) => url.filter(|u| !u.trim().is_empty()),
        Err(e) => return found(format!("omni: {}", clean(&e.to_string())), Vec::new()),
    };
    let Some(url) = stored else {
        return found(
            format!("omni: no Omni URL in the Keychain or {ENV_URL}"),
            Vec::new(),
        );
    };
    let base = match BaseUrl::parse(&url) {
        Ok(base) => base,
        Err(e) => {
            return found(
                format!("omni: the Keychain's Omni URL {}", clean(&e.to_string())),
                Vec::new(),
            );
        }
    };
    match secrets.has(&keychain_key_account(&base)) {
        Ok(true) => found(
            format!(
                "omni: set up for {}, key in the Keychain",
                clean(base.authority())
            ),
            Vec::new(),
        ),
        Ok(false) => found(
            format!(
                "omni: the Keychain names {} but holds no API key for it",
                clean(base.authority())
            ),
            vec![set_key(&base)],
        ),
        Err(e) => found(format!("omni: {}", clean(&e.to_string())), Vec::new()),
    }
}

/// Everything setup prints and does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// What it found, one line each.
    pub found: Vec<String>,
    /// The harnesses it would set up, each with its steps.
    pub installs: Vec<(Harness, Vec<Step>)>,
    /// The commands that finish the data sources, one line each.
    pub next: Vec<String>,
    /// Whether any harness Tiles supports is on `PATH`.
    pub on_path: bool,
    /// Whether a harness on `PATH` is left for the user: refused, or set
    /// up with another `tiles`.
    pub left: bool,
    /// The running `tiles` and the others on `PATH`, some of which setup
    /// offers to move aside.
    pub copies: Copies,
}

/// The report from each harness's plan, the terminal line, a line for each
/// other `tiles` on `PATH`, and what each data source found.
#[must_use]
pub fn report(
    plans: Vec<(Harness, Plan)>,
    terminal: &str,
    copies: &Copies,
    data: &[Found],
) -> Report {
    let clean = |s: &str| sanitize::line(s, 300);
    let shown = |p: &Path| clean(&p.display().to_string());
    let mut found = vec![clean(terminal)];
    found.extend(copies.found());
    let mut installs = Vec::new();
    let on_path = plans.iter().any(|(_, p)| *p != Plan::Missing);
    let left = plans
        .iter()
        .any(|(_, p)| matches!(p, Plan::Refused(..) | Plan::Other(..)));
    for (h, plan) in plans {
        found.push(match plan {
            Plan::Missing => format!("{}: not on PATH", h.name()),
            Plan::Current(path) => format!("{}: {}, already set up", h.name(), shown(&path)),
            Plan::Install(path, steps) => {
                installs.push((h, steps));
                format!("{}: {}, not set up", h.name(), shown(&path))
            }
            Plan::Refused(path, why) => format!(
                "{}: {}, not set up; Tiles leaves it: {}",
                h.name(),
                shown(&path),
                clean(&why)
            ),
            Plan::Other(path, other) => {
                format!("{}: {}, {}", h.name(), shown(&path), clean(&other))
            }
            Plan::Replace(path, other, steps) => {
                installs.push((h, steps));
                format!(
                    "{}: {}, {}; Tiles will replace it",
                    h.name(),
                    shown(&path),
                    clean(&other)
                )
            }
        });
    }
    found.extend(data.iter().map(|f| clean(&f.line)));
    let next = data
        .iter()
        .flat_map(|f| f.next.iter().map(|n| clean(n)))
        .collect();
    Report {
        found,
        installs,
        next,
        on_path,
        left,
        copies: copies.clone(),
    }
}

/// Sets one harness up with its steps.
pub type Apply<'a> = dyn Fn(Harness, &[Step]) -> Result<(), String> + 'a;

/// `a`, `a and b`, `a, b and c`.
fn listed(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Prints the report, asks once unless `args` answers, and installs through
/// `apply`. Then, with `tty` saying a person can answer, it asks once per
/// copy of `tiles` it offers to move aside, listed with the installs;
/// without one, with `--yes` or `--dry-run`, it prints the `mv` command. `answer` reads the user's reply; `None` is no
/// reply. Returns whether every install and move it ran succeeded.
pub fn run(
    args: Args,
    report: &Report,
    tty: bool,
    out: &mut dyn Write,
    answer: &mut dyn FnMut() -> Option<String>,
    apply: &Apply<'_>,
) -> std::io::Result<bool> {
    writeln!(out, "Tiles found:")?;
    for line in &report.found {
        writeln!(out, "  {line}")?;
    }
    let moves = report.copies.moves();
    let ask = tty && !args.yes && !args.dry_run;
    let asked: &[&Other] = if ask { &moves } else { &[] };
    let yes = |r: Option<String>| {
        r.is_some_and(|r| matches!(r.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
    };
    let mut ok = true;
    if !report.installs.is_empty() || !asked.is_empty() {
        writeln!(out, "Tiles will change:")?;
        for (h, steps) in &report.installs {
            for step in steps {
                writeln!(
                    out,
                    "  {}: {}",
                    h.name(),
                    sanitize::line(&step.describe(), 400)
                )?;
            }
        }
        for m in asked {
            writeln!(out, "  tiles: {}", moving(m))?;
        }
    }
    if report.installs.is_empty() {
        writeln!(out, "{}", unchanged(report, !asked.is_empty()))?;
    } else {
        let names: Vec<&str> = report.installs.iter().map(|(h, _)| title(*h)).collect();
        let go = if args.dry_run {
            writeln!(out, "Dry run: nothing changed.")?;
            false
        } else if args.yes {
            true
        } else {
            write!(out, "Set up Tiles for {}? [y/N] ", listed(&names))?;
            out.flush()?;
            let reply = answer();
            if reply.is_none() {
                writeln!(out)?;
            }
            yes(reply)
        };
        if go {
            for (h, steps) in &report.installs {
                match apply(*h, steps) {
                    Ok(()) => writeln!(out, "{}", crate::install_done(*h))?,
                    Err(e) => {
                        ok = false;
                        writeln!(
                            out,
                            "tiles: {} not set up: {}",
                            h.name(),
                            sanitize::line(&e, 400)
                        )?;
                    }
                }
            }
        } else if !args.dry_run {
            let what = if asked.is_empty() {
                "Nothing"
            } else {
                "No harness"
            };
            writeln!(
                out,
                "{what} changed. Run tiles setup again to set it up, or tiles install <harness> for one harness."
            )?;
        }
    }
    for m in &moves {
        ok &= move_one(m, &report.copies.this, ask, out, answer, &yes)?;
    }
    if !report.next.is_empty() {
        writeln!(out, "To finish the data sources, run:")?;
        for line in &report.next {
            writeln!(out, "  {line}")?;
        }
        writeln!(
            out,
            "Then run tiles setup again, so each harness gets the data tools and asks before each Tiles tool call."
        )?;
    }
    Ok(ok)
}

/// The line when no harness needs an install; `moving` says a move is
/// listed under `Tiles will change:`.
fn unchanged(report: &Report, moving: bool) -> &'static str {
    match (report.left, report.on_path, moving) {
        (true, _, false) => "Nothing for Tiles to change; see the lines above.",
        (true, _, true) => "Tiles changes no harness; see the lines above.",
        (false, true, false) => {
            "Every harness on PATH that Tiles can set up already has it; nothing to change."
        }
        (false, true, true) => "Every harness on PATH that Tiles can set up already has it.",
        (false, false, _) => {
            "No harness Tiles supports is on PATH; install Claude Code, Codex, OpenCode or Pi, then run tiles setup again."
        }
    }
}

/// `move <path> aside, to <name> beside it`, for the copy `m`.
fn moving(m: &Other) -> String {
    let name = m
        .aside
        .as_deref()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    sanitize::line(
        &format!("move {} aside, to {name} beside it", m.path.display()),
        crate::copies::LONGEST,
    )
}

/// What a shell needs to find `tiles` afresh after a move.
const REHASH: &str = "open a new terminal tab, or run hash -r (rehash in zsh)";

/// Moves `m` aside after a yes when `ask`, else prints the command that
/// does. Returns whether nothing failed.
fn move_one(
    m: &Other,
    this: &Path,
    ask: bool,
    out: &mut dyn Write,
    answer: &mut dyn FnMut() -> Option<String>,
    yes: &dyn Fn(Option<String>) -> bool,
) -> std::io::Result<bool> {
    let clean = |s: &str| sanitize::line(s, crate::copies::LONGEST);
    let mv = clean(&m.mv().unwrap_or_default());
    let from = clean(&m.path.display().to_string());
    if !ask {
        writeln!(out, "To move {from} aside, run: {mv}, then {REHASH}.")?;
        return Ok(true);
    }
    let to = clean(&m.aside.as_deref().unwrap_or(&m.path).display().to_string());
    write!(out, "Move {from} aside, to {to}? [y/N] ")?;
    out.flush()?;
    let reply = answer();
    if reply.is_none() {
        writeln!(out)?;
    }
    if !yes(reply) {
        writeln!(out, "Left {from} where it is; to move it aside, run: {mv}")?;
        return Ok(true);
    }
    match m.move_aside() {
        Ok(to) => {
            writeln!(
                out,
                "Moved {from} to {}. To run {} as tiles, {REHASH}.",
                clean(&to.display().to_string()),
                clean(&this.display().to_string())
            )?;
            Ok(true)
        }
        Err(e) => {
            writeln!(
                out,
                "tiles: could not move {from}: {}; run: {mv}",
                clean(&e)
            )?;
            Ok(false)
        }
    }
}

/// The terminal line: its name, and that setup changes nothing there.
fn terminal(env: &dyn Fn(&str) -> Option<String>) -> String {
    match tiles_term::detect::detect(env, None) {
        Ok(d) => format!(
            "terminal: {}, nothing to change (tiles doctor shows how cards draw here)",
            d.terminal.name()
        ),
        Err(e) => format!("terminal: {e}"),
    }
}

/// The cards line on the text route ([`crate::companion::text_route_line`]),
/// for a terminal named by its environment: setup asks the terminal nothing,
/// and one it cannot name may still answer `tiles run`'s picture queries.
fn cards(env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let d = tiles_term::detect::detect(env, None).ok()?;
    let named = d.terminal != tiles_term::detect::Terminal::Unknown
        || env(tiles_term::detect::ENV_ROUTE).is_some_and(|r| !r.is_empty());
    named
        .then(|| crate::companion::text_route_line(&d, env))
        .flatten()
}

/// Puts the [`cards`] line under the terminal line, when there is one.
fn add_cards(report: &mut Report, env: &dyn Fn(&str) -> Option<String>) {
    if let Some(line) = cards(env) {
        let at = report.found.len().min(1);
        report.found.insert(at, sanitize::line(&line, 300));
    }
}

/// Each of `harnesses` with its plan on this machine, for installs that run
/// this `tiles` and reach `reach`.
fn this_machine(
    dirs: &Dirs,
    reach: &Reach,
    forms: &[FormEntry],
    harnesses: &[Harness],
    claude_tiles: &dyn Fn(&Path) -> Option<Registered>,
) -> Vec<(Harness, Plan)> {
    let this = crate::this_tiles();
    let spec = |h: Harness| install::spec(h, this.clone(), reach.clone(), forms.to_vec());
    let inputs = Inputs {
        which: &crate::which,
        dirs,
        spec: &spec,
        claude_tiles,
    };
    harnesses.iter().map(|h| (*h, plan(*h, &inputs))).collect()
}

/// What setup finds of Omni on this machine, and what its installs reach:
/// Omni when [`omni`] finds it set up, and each configured source's offer.
fn here(sources: &[&dyn Source]) -> (Found, Reach) {
    let present = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    let (omni, omni_set_up) = omni(&crate::serve::env, &present, &tiles_omni::Keychain);
    let reach = Reach {
        omni: omni_set_up,
        offers: crate::serve::offered(sources)
            .into_iter()
            .map(|(_, o)| o)
            .collect(),
    };
    (omni, reach)
}

/// `tiles doctor`'s hint from the plans: the harnesses `tiles setup` would
/// set up, and a Claude Code set up with another `tiles`, which setup
/// replaces or leaves.
#[must_use]
pub fn hints(plans: &[(Harness, Plan)]) -> Vec<String> {
    let names: Vec<&str> = plans
        .iter()
        .filter(|(_, p)| matches!(p, Plan::Install(..)))
        .map(|(h, _)| title(*h))
        .collect();
    let mut out = Vec::new();
    if !names.is_empty() {
        out.push(format!(
            "  setup: Tiles is not set up, or is out of date, for {}; run tiles setup",
            listed(&names)
        ));
    }
    for (h, plan) in plans {
        match plan {
            Plan::Other(_, other) => out.push(format!(
                "  setup: {} is {}",
                title(*h),
                sanitize::line(other, 300)
            )),
            Plan::Replace(_, other, _) => out.push(format!(
                "  setup: {} is {}; run tiles setup to replace it",
                title(*h),
                sanitize::line(other, 300)
            )),
            _ => {}
        }
    }
    out
}

/// [`hints`] on this machine, for `tiles doctor`. Claude Code's
/// registration comes from `~/.claude.json`, so no server starts; without
/// that file in the shape it expects, Claude Code is left out.
pub(crate) fn doctor_hints(dirs: &Dirs, sources: &[&dyn Source]) -> Vec<String> {
    let Ok(forms) = crate::engine_forms() else {
        return Vec::new();
    };
    let claude = claude_configured(&dirs.home);
    let harnesses: Vec<Harness> = Harness::ALL
        .into_iter()
        .filter(|h| *h != Harness::Claude || claude.is_ok())
        .collect();
    let registered = claude.ok().flatten();
    let plans = this_machine(dirs, &here(sources).1, &forms, &harnesses, &|_| {
        registered.clone()
    });
    hints(&plans)
}

/// Runs `tiles setup` on this machine.
pub fn main(args: &[String], sources: &[&dyn Source]) -> std::process::ExitCode {
    use std::process::ExitCode;
    let args = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("tiles: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(dirs) = Dirs::from_env(&crate::serve::env) else {
        eprintln!("tiles: HOME is not set");
        return ExitCode::FAILURE;
    };
    let forms = match crate::engine_forms() {
        Ok(forms) => forms,
        Err(e) => {
            eprintln!("tiles: {}", sanitize::line(&e, 400));
            return ExitCode::FAILURE;
        }
    };
    // What the installs reach, decided as `tiles install` decides it but
    // without reading the Omni key before the question.
    let (omni, reach) = here(sources);
    let plans = this_machine(&dirs, &reach, &forms, &Harness::ALL, &claude_tiles);
    let copies = crate::copies::here();
    let mut data = vec![omni];
    data.extend(sources.iter().filter_map(|s| s.found()));
    let mut report = report(plans, &terminal(&crate::serve::env), &copies, &data);
    add_cards(&mut report, &crate::serve::env);
    let mut answer = || {
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
        }
    };
    let apply = |_: Harness, steps: &[Step]| apply(steps, &install::run_program);
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let mut out = std::io::stdout().lock();
    match run(args, &report, tty, &mut out, &mut answer, &apply) {
        Ok(true) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests;
