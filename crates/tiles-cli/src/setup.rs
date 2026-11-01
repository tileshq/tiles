//! `tiles setup [--yes] [--dry-run] [--verbose]`: one command that sets Tiles
//! up on this Mac (ADR-035). It finds each harness Tiles supports on `PATH`,
//! the terminal, and the data logins it can see without reading or copying a
//! secret, lists what it would change, and asks once, no by default. Then it
//! prints one line per step (ADR-060), running the steps `tiles install` runs
//! for each harness not yet set up, and a data source it found but could not
//! finish names what finishes it. A harness whose install already matches
//! what `tiles install` would write is left as it is, so a second run changes
//! nothing. A Claude Code whose user-scope `tiles` server runs another `tiles`
//! has it removed and this one added, named before the question (ADR-039). A
//! server in a project's or a folder's config, one that sets a variable of the
//! user's, or one that lacks `mcp` and the draw variable, is reported with the
//! commands that replace it, and left as it is. Each other `tiles` on `PATH`
//! is named with its version, and an older one ahead of this one that the user
//! owns is offered to be moved aside after a question of its own, in a
//! terminal only ([`crate::copies`]). `--yes` answers for a script; `--dry-run`
//! prints the plan and changes nothing; `--verbose` adds the paths and each
//! change under its step. `tiles doctor` gets a hint from the same plan,
//! reading Claude Code's registration from `~/.claude.json` so it starts no
//! server.

use std::cmp::Ordering;
use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tiles_intent::schema::FormEntry;
use tiles_mcp::source::{Found, Reach, Source};
use tiles_omni::{BaseUrl, ENV_API_KEY, ENV_URL, KEYCHAIN_URL, SecretStore, keychain_key_account};
use tiles_term::sanitize;
use tiles_term::style::{Mark, Style};

use crate::addons::Spec;
use crate::copies::{Copies, Other, Place};
use crate::harness::Harness;
use crate::install::{self, Dirs, Step};
use crate::proved::title;

/// Usage of `tiles setup`.
pub const USAGE: &str = "tiles setup [--yes] [--dry-run] [--verbose]";

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
    /// `--verbose`: add the paths and each change under its step.
    pub verbose: bool,
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
            "--verbose" => out.verbose = true,
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
/// bytes, or for `OpenCode`'s config the same settings, Codex already asks
/// and waits long enough, or Codex already has the server.
fn current(step: &Step, spec: &Spec, i: &Inputs<'_>) -> bool {
    match step {
        Step::Write(path, bytes) if *path == i.dirs.opencode_file() => {
            fs::read(path).is_ok_and(|b| install::opencode_file_is_current(&b, bytes))
        }
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

/// One line of the step list: how the step stands, its label, its state, and
/// what `--verbose` adds under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    mark: Mark,
    label: String,
    state: String,
    detail: Vec<String>,
}

impl Row {
    fn new(mark: Mark, label: &str, state: impl Into<String>) -> Self {
        Self {
            mark,
            label: label.to_string(),
            state: state.into(),
            detail: Vec::new(),
        }
    }

    fn with(mut self, detail: impl IntoIterator<Item = String>) -> Self {
        self.detail.extend(detail);
        self
    }
}

/// How the list prints: its style, whether details show, and the label width.
#[derive(Clone, Copy)]
struct Printer {
    style: Style,
    verbose: bool,
    width: usize,
}

impl Printer {
    fn print(self, row: &Row, out: &mut dyn Write) -> std::io::Result<()> {
        let line = self
            .style
            .step(row.mark, &row.label, self.width, &row.state);
        writeln!(out, "{line}")?;
        if self.verbose {
            for detail in &row.detail {
                writeln!(out, "{}", self.style.detail(detail))?;
            }
        }
        Ok(())
    }
}

/// The label of the running `tiles`'s own step, the longest fixed one.
const ON_PATH: &str = "Tiles on PATH";

/// The label of each other `tiles`.
const OTHER: &str = "Other tiles";

/// Everything setup prints and does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The terminal's step.
    pub terminal: Row,
    /// The step saying cards will not draw inline here, on the text route.
    pub pictures: Option<Row>,
    /// Each harness with its plan.
    pub plans: Vec<(Harness, Plan)>,
    /// One step per data source.
    pub data: Vec<Row>,
    /// The harnesses it would set up, each with its steps.
    pub installs: Vec<(Harness, Vec<Step>)>,
    /// Whether any harness Tiles supports is on `PATH`.
    pub on_path: bool,
    /// Whether a harness on `PATH` is left for the user: refused, or set
    /// up with another `tiles`.
    pub left: bool,
    /// The running `tiles` and the others on `PATH`, some of which setup
    /// offers to move aside.
    pub copies: Copies,
}

/// The step for what a data source found: finished, left for the user with
/// what finishes it, or skipped.
fn data_row(found: &Found) -> Row {
    let (name, said) = found.line.split_once(": ").unwrap_or(("data", &found.line));
    let mut label = name.to_string();
    if let Some(first) = label.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    let row = if !found.next.is_empty() {
        Row::new(
            Mark::Attention,
            &label,
            format!(
                "{said}; to finish: {}; then tiles setup again",
                found.next.join("; ")
            ),
        )
    } else if said.starts_with("set up for ") {
        Row::new(Mark::Done, &label, said)
    } else {
        Row::new(Mark::Skipped, &label, format!("skipped: {said}"))
    };
    row.with([found.line.clone()])
}

/// The report from each harness's plan, the terminal's step, the other copies
/// of `tiles` on `PATH`, and what each data source found.
#[must_use]
pub fn report(
    plans: Vec<(Harness, Plan)>,
    terminal: Row,
    copies: &Copies,
    data: &[Found],
) -> Report {
    let installs = plans
        .iter()
        .filter_map(|(h, plan)| match plan {
            Plan::Install(_, steps) | Plan::Replace(_, _, steps) => Some((*h, steps.clone())),
            _ => None,
        })
        .collect();
    let on_path = plans.iter().any(|(_, p)| *p != Plan::Missing);
    let left = plans
        .iter()
        .any(|(_, p)| matches!(p, Plan::Refused(..) | Plan::Other(..)));
    Report {
        terminal,
        pictures: None,
        data: data.iter().map(data_row).collect(),
        plans,
        installs,
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

/// Where a harness with something to change stands.
#[derive(Clone, Copy)]
enum Change<'a> {
    /// A question is about to ask.
    Will,
    /// A dry run.
    Would,
    /// The question was not answered yes.
    Declined,
    /// It ran.
    Done,
    /// It ran and failed, with this error.
    Failed(&'a str),
}

/// The step for `harness` with `plan`, where `change` says how far a harness
/// with something to change has got.
fn harness_row(harness: Harness, plan: &Plan, change: Change<'_>) -> Row {
    let label = title(harness);
    let shown = |p: &Path| p.display().to_string();
    match plan {
        Plan::Missing => Row::new(Mark::Skipped, label, "skipped: not on PATH"),
        Plan::Current(path) => Row::new(Mark::Done, label, "already set up").with([shown(path)]),
        Plan::Refused(path, why) => {
            Row::new(Mark::Attention, label, format!("left as it is: {why}")).with([shown(path)])
        }
        Plan::Other(path, other) => {
            Row::new(Mark::Attention, label, format!("left as it is: {other}")).with([shown(path)])
        }
        Plan::Install(path, steps) => changing(label, None, change)
            .with([shown(path)])
            .with(steps.iter().map(Step::describe)),
        Plan::Replace(path, other, steps) => changing(label, Some(other), change)
            .with([shown(path)])
            .with(steps.iter().map(Step::describe)),
    }
}

/// The step for a harness with something to change; `replacing` says what its
/// old `tiles` server is, when it has one.
fn changing(label: &str, replacing: Option<&String>, change: Change<'_>) -> Row {
    match (change, replacing) {
        (Change::Will, None) => Row::new(Mark::Pending, label, "will configure"),
        (Change::Will, Some(other)) => Row::new(
            Mark::Pending,
            label,
            format!("{other}; Tiles will replace it"),
        ),
        (Change::Would, None) => Row::new(Mark::Pending, label, "would configure"),
        (Change::Would, Some(other)) => Row::new(
            Mark::Pending,
            label,
            format!("{other}; Tiles would replace it, or {CLAUDE_FIX}"),
        ),
        (Change::Declined, None) => Row::new(Mark::Skipped, label, "skipped: not confirmed"),
        (Change::Declined, Some(other)) => Row::new(
            Mark::Attention,
            label,
            format!("not confirmed; {other}: {CLAUDE_FIX}"),
        ),
        (Change::Done, None) => Row::new(Mark::Done, label, "configured"),
        (Change::Done, Some(other)) => {
            Row::new(Mark::Done, label, format!("configured; it was {other}"))
        }
        (Change::Failed(e), _) => {
            let again = if e.ends_with("tiles setup") || e.ends_with("tiles setup again") {
                ""
            } else {
                "; then tiles setup again"
            };
            Row::new(Mark::Failed, label, format!("failed: {e}{again}"))
        }
    }
}

/// The step for the running `tiles`: `already` when this machine has nothing
/// left to do.
fn binary_row(copies: &Copies, already: bool) -> Row {
    let row = if copies.on_path {
        let state = if already {
            "already set up"
        } else {
            "installed"
        };
        Row::new(Mark::Done, ON_PATH, state)
    } else {
        let dir = copies.this.parent().unwrap_or(&copies.this).display();
        Row::new(
            Mark::Attention,
            ON_PATH,
            format!("not on PATH: add {dir} to PATH, then open a new terminal tab"),
        )
    };
    row.with([format!(
        "{}, version {}",
        copies.this.display(),
        copies.current
    )])
}

/// How a copy offered a move came out.
enum Moved {
    /// It stays where it is.
    Stays,
    /// It was moved here.
    To(PathBuf),
    /// The move failed with this error.
    Failed(String),
}

/// `<path>, version <v>`, for the copy `o`.
fn copy_named(o: &Other) -> String {
    let path = o.path.display();
    o.version.as_deref().map_or_else(
        || format!("{path}, which printed no version"),
        |v| format!("{path}, version {v}"),
    )
}

/// The step for another `tiles`; `first` says it is the one a terminal runs.
fn copy_row(o: &Other, first: bool, moved: &Moved) -> Row {
    let path = o.path.display();
    let mv = o.mv();
    match moved {
        Moved::To(to) => {
            return Row::new(
                Mark::Done,
                OTHER,
                format!(
                    "moved {path} aside, to {}; open a new terminal tab to run this one",
                    to.display()
                ),
            );
        }
        Moved::Failed(e) => {
            return Row::new(
                Mark::Failed,
                OTHER,
                format!(
                    "could not move {path}: {e}; run: {}",
                    mv.unwrap_or_default()
                ),
            );
        }
        Moved::Stays => {}
    }
    let what = copy_named(o);
    let ahead = if first {
        "comes first on PATH, so a terminal runs it"
    } else {
        "is also ahead of this one on PATH"
    };
    let mut row = match (o.place, o.against) {
        (Place::Ahead, Some(Ordering::Greater)) => Row::new(
            Mark::Attention,
            OTHER,
            format!("{what}, is newer than this one and {ahead}"),
        ),
        (Place::Ahead, Some(Ordering::Equal)) => Row::new(
            Mark::Attention,
            OTHER,
            format!("{what}, is the same version as this one and {ahead}"),
        ),
        (Place::Ahead, _) => Row::new(Mark::Attention, OTHER, format!("{what}, {ahead}")),
        (Place::Behind, _) => Row::new(
            Mark::Skipped,
            OTHER,
            format!("{what}, is later on PATH, so a terminal runs this one"),
        ),
        (Place::Only, _) => Row::new(
            Mark::Attention,
            OTHER,
            format!("{what}, is on PATH, and this one is not"),
        ),
    };
    if let Some(mv) = mv.filter(|_| o.stale()) {
        row.state = format!("{}; run: {mv}, then open a new terminal tab", row.state);
    }
    row
}

/// Asks `question`, no by default; a reply with no terminal to echo it ends
/// its own line. Returns whether the reply is a yes.
fn ask(
    question: &str,
    tty: bool,
    out: &mut dyn Write,
    answer: &mut dyn FnMut() -> Option<String>,
) -> std::io::Result<bool> {
    write!(out, "{question} [y/N] ")?;
    out.flush()?;
    let reply = answer();
    if reply.is_none() || !tty {
        writeln!(out)?;
    }
    Ok(reply.is_some_and(|r| matches!(r.trim().to_ascii_lowercase().as_str(), "y" | "yes")))
}

/// The widest label in the list.
fn label_width(report: &Report) -> usize {
    [ON_PATH, OTHER, report.terminal.label.as_str()]
        .into_iter()
        .chain(report.pictures.iter().map(|r| r.label.as_str()))
        .chain(report.plans.iter().map(|(h, _)| title(*h)))
        .chain(report.data.iter().map(|r| r.label.as_str()))
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
}

/// Prints a step per other copy, moving aside those in `to_move`. Returns whether
/// every move succeeded and whether a stale copy stays ahead of this one.
fn copy_rows(
    copies: &Copies,
    to_move: &[PathBuf],
    printer: Printer,
    out: &mut dyn Write,
) -> std::io::Result<(bool, bool)> {
    let mut ok = true;
    let mut stale_left = false;
    let mut first = true;
    for (o, line) in copies.others.iter().zip(copies.found()) {
        let outcome = if to_move.contains(&o.path) {
            match o.move_aside() {
                Ok(to) => Moved::To(to),
                Err(e) => {
                    ok = false;
                    Moved::Failed(e)
                }
            }
        } else {
            Moved::Stays
        };
        stale_left |= o.stale() && !matches!(outcome, Moved::To(_));
        let leads = first && o.place == Place::Ahead;
        first &= !leads;
        printer.print(&copy_row(o, leads, &outcome).with([line]), out)?;
    }
    Ok((ok, stale_left))
}

/// Prints the header, lists what would change and asks unless `args` answers,
/// then prints one step per line, moving copies aside and installing through
/// `apply` as it reaches them, and one closing line. With `tty` saying a person
/// can answer, it asks once per copy of `tiles` it offers to move aside, listed
/// with the installs; without one, with `--yes` or `--dry-run`, that copy's
/// step names the `mv` command. `answer` reads the user's reply; `None` is no
/// reply. `style` says whether the steps carry marks. Returns whether every
/// install and move it ran succeeded.
pub fn run(
    args: Args,
    report: &Report,
    tty: bool,
    style: Style,
    out: &mut dyn Write,
    answer: &mut dyn FnMut() -> Option<String>,
    apply: &Apply<'_>,
) -> std::io::Result<bool> {
    let printer = Printer {
        style,
        verbose: args.verbose,
        width: label_width(report),
    };
    writeln!(out, "Tiles {} setup", tiles_intent::TILES_VERSION)?;
    let moves = report.copies.moves();
    let asked: &[&Other] = if tty && !args.yes && !args.dry_run {
        &moves
    } else {
        &[]
    };
    let question = !args.yes && !args.dry_run && !report.installs.is_empty();
    let asking = question || !asked.is_empty();
    if asking {
        for (h, plan) in &report.plans {
            if matches!(plan, Plan::Install(..) | Plan::Replace(..)) {
                printer.print(&harness_row(*h, plan, Change::Will), out)?;
            }
        }
        for m in asked {
            printer.print(&Row::new(Mark::Pending, OTHER, moving(m)), out)?;
        }
        if !args.verbose {
            writeln!(out, "tiles setup --dry-run --verbose lists each change.")?;
        }
    }
    let go = if question {
        let names: Vec<&str> = report.installs.iter().map(|(h, _)| title(*h)).collect();
        let question = format!("Set up Tiles for {}?", listed(&names));
        ask(&question, tty, out, answer)?
    } else {
        args.yes
    };
    let mut to_move = Vec::new();
    for m in asked {
        let clean = |p: &Path| sanitize::line(&p.display().to_string(), crate::copies::LONGEST);
        let question = format!(
            "Move {} aside, to {}?",
            clean(&m.path),
            clean(m.aside.as_deref().unwrap_or(&m.path))
        );
        if ask(&question, tty, out, answer)? {
            to_move.push(m.path.clone());
        }
    }
    if asking {
        writeln!(out)?;
    }

    let mut ok = true;
    let already = report.installs.is_empty()
        && moves.is_empty()
        && report
            .plans
            .iter()
            .any(|(_, p)| matches!(p, Plan::Current(_)));
    printer.print(&binary_row(&report.copies, already), out)?;
    let (copies_ok, stale_left) = copy_rows(&report.copies, &to_move, printer, out)?;
    ok &= copies_ok;
    printer.print(&report.terminal, out)?;
    if let Some(row) = &report.pictures {
        printer.print(row, out)?;
    }
    for (h, plan) in &report.plans {
        let row = match plan {
            Plan::Install(_, steps) | Plan::Replace(_, _, steps) if go && !args.dry_run => {
                match apply(*h, steps) {
                    Ok(()) => harness_row(*h, plan, Change::Done),
                    Err(e) => {
                        ok = false;
                        harness_row(*h, plan, Change::Failed(&e))
                    }
                }
            }
            _ if args.dry_run => harness_row(*h, plan, Change::Would),
            _ => harness_row(*h, plan, Change::Declined),
        };
        printer.print(&row, out)?;
    }
    for row in &report.data {
        printer.print(row, out)?;
    }
    let blocked = if !report.copies.on_path {
        Some(ON_PATH)
    } else if stale_left {
        Some(OTHER)
    } else {
        None
    };
    let last = closing(args, report, go, ok, !asked.is_empty(), blocked);
    writeln!(out, "{last}")?;
    Ok(ok)
}

/// The closing line: what to try next, or why not yet. `go` says the harnesses
/// were to change, `ok` that nothing failed, `move_asked` that a move was asked
/// about, and `blocked` names the step to see first, while `tiles` in a terminal
/// would not start this one.
fn closing(
    args: Args,
    report: &Report,
    go: bool,
    ok: bool,
    move_asked: bool,
    blocked: Option<&str>,
) -> String {
    let first = blocked.map_or_else(String::new, |row| format!("see the {row} line, then "));
    if !ok {
        "Some steps failed; each failed line says what to run.".into()
    } else if args.dry_run {
        "Dry run: nothing changed.".into()
    } else if !report.installs.is_empty() && !go {
        let what = if move_asked { "No harness" } else { "Nothing" };
        format!(
            "{what} changed. To set it up, {first}run tiles setup again, or tiles install <harness> for one harness."
        )
    } else if !report.on_path {
        format!(
            "No harness Tiles supports is on PATH; {first}install Claude Code, Codex, OpenCode or Pi, then run tiles setup again."
        )
    } else if report.left && report.installs.is_empty() {
        let what = if move_asked {
            "Tiles changes no harness"
        } else {
            "Nothing for Tiles to change"
        };
        format!("{what}; see the lines above.")
    } else {
        let set_up = report
            .plans
            .iter()
            .find(|(_, p)| matches!(p, Plan::Current(_) | Plan::Install(..) | Plan::Replace(..)));
        let next = if blocked.is_some() {
            format!("Not yet: {first}")
        } else {
            "Next: ".to_string()
        };
        set_up.map_or_else(
            || format!("{next}run tiles check."),
            |(h, _)| {
                format!(
                    "{next}run tiles check, then start a session with tiles run {}.",
                    h.name()
                )
            },
        )
    }
}

/// The state of a copy setup is about to ask to move aside.
fn moving(m: &Other) -> String {
    let name = m
        .aside
        .as_deref()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "{}, is ahead of this one on PATH; Tiles will move it aside, to {name} beside it",
        copy_named(m)
    )
}

/// The terminal's step: its name, and that setup changes nothing there.
fn terminal(env: &dyn Fn(&str) -> Option<String>) -> Row {
    let label = "Terminal";
    match tiles_term::detect::detect(env, None) {
        Ok(d) if d.terminal == tiles_term::detect::Terminal::Unknown => Row::new(
            Mark::Done,
            label,
            "nothing to set up; tiles check tests how cards draw here",
        ),
        Ok(d) => Row::new(
            Mark::Done,
            label,
            format!("{}: nothing to set up", d.terminal.name()),
        )
        .with(["tiles doctor shows how cards draw here".to_string()]),
        Err(e) => Row::new(Mark::Skipped, label, format!("skipped: {e}")),
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

/// Adds the step for the [`cards`] line, when there is one; the whole line
/// goes under it for `--verbose`.
fn add_cards(report: &mut Report, env: &dyn Fn(&str) -> Option<String>) {
    report.pictures = cards(env).map(|line| {
        Row::new(
            Mark::Attention,
            "Pictures",
            "cards will not draw inline here; tiles doctor says how they open",
        )
        .with([line])
    });
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
    let mut report = report(plans, terminal(&crate::serve::env), &copies, &data);
    add_cards(&mut report, &crate::serve::env);
    let mut answer = || {
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
        }
    };
    // With --verbose a harness's own output shows; otherwise it would break
    // up the step list.
    let run_step: &install::RunCommand<'_> = if args.verbose {
        &install::run_program
    } else {
        &install::run_quiet
    };
    let apply = |_: Harness, steps: &[Step]| apply(steps, run_step);
    let stdout = std::io::stdout();
    let tty = std::io::stdin().is_terminal() && stdout.is_terminal();
    let style = Style::new(stdout.is_terminal(), std::env::var_os("NO_COLOR").is_some());
    let mut out = stdout.lock();
    match run(args, &report, tty, style, &mut out, &mut answer, &apply) {
        Ok(true) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests;
