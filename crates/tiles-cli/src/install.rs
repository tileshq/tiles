//! `tiles install <harness>`: the permanent add-on, for sessions started
//! without `tiles run`. Pi gets the extension in its extensions directory;
//! `OpenCode` gets a config file of Tiles' own beside the user's; Claude Code
//! and Codex get the MCP server through their own `mcp add` command, which
//! owns their config files. With Omni or a warehouse configured, Codex's
//! config also gets the one line that makes it ask before each Tiles tool
//! call, and with a warehouse the tool timeout that outlasts its query,
//! since `codex mcp add` has a flag for neither; [`reapply_approvals`]
//! gives an install made before either was configured the same settings. Outside
//! `tiles run` no wrapper draws, so an installed server answers with the text
//! route unless the harness draws images itself (Pi).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tiles_intent::schema::FormEntry;
use tiles_mcp::instructions::Draw;
use tiles_mcp::source::Reach;
use tiles_mcp::{THEME_TOOL, TOOL_NAME};

use crate::addons::{self, Spec};
use crate::harness::{self, Harness};

/// Where the harnesses Tiles installs into keep their config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// The user's home.
    pub home: PathBuf,
    /// The XDG config directory, `~/.config` by default.
    pub config_home: PathBuf,
    /// Codex's home, `~/.codex` by default.
    pub codex_home: PathBuf,
}

/// Where `tiles install pi` puts the Pi extension under `home`.
#[must_use]
pub fn pi_extension(home: &Path) -> PathBuf {
    home.join(".pi/agent/extensions/tiles")
}

impl Dirs {
    /// From `HOME`, `XDG_CONFIG_HOME` and `CODEX_HOME`; `None` without `HOME`.
    #[must_use]
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let home = PathBuf::from(env("HOME")?);
        Some(Self {
            config_home: env("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from),
            codex_home: env("CODEX_HOME").map_or_else(|| home.join(".codex"), PathBuf::from),
            home,
        })
    }

    pub(crate) fn opencode_file(&self) -> PathBuf {
        self.config_home.join("opencode").join(OPENCODE_FILE)
    }

    /// `OpenCode`'s `config.json` when it holds a `tiles` server, as
    /// `tiles install opencode` writes it.
    #[must_use]
    pub fn opencode_install(&self) -> Option<PathBuf> {
        let file = self.opencode_file();
        let config: serde_json::Value = serde_json::from_slice(&fs::read(&file).ok()?).ok()?;
        config.pointer("/mcp/tiles").is_some().then_some(file)
    }

    pub(crate) fn codex_file(&self) -> PathBuf {
        self.codex_home.join(CODEX_FILE)
    }
}

/// The global `OpenCode` config file Tiles owns. `OpenCode` merges
/// `config.json`, `opencode.json` and `opencode.jsonc` from its config
/// directory, so the user's own `opencode.json` is never edited.
pub const OPENCODE_FILE: &str = "config.json";

/// Refuses to overwrite an `OpenCode` `config.json` that is not Tiles' own: one
/// with keys beyond `mcp`, `permission`, `command` and `instructions`, a
/// server other than `tiles`, or a command other than `theme`.
///
/// # Errors
///
/// What the file holds and what to do instead.
pub fn opencode_file_is_ours(existing: Option<&[u8]>) -> Result<(), String> {
    let Some(bytes) = existing else { return Ok(()) };
    let ours = serde_json::from_slice::<serde_json::Value>(bytes).is_ok_and(|v| {
        v.as_object().is_some_and(|o| {
            o.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "mcp" | "permission" | "command" | "instructions"
                )
            }) && o
                .get("mcp")
                .and_then(|m| m.as_object())
                .is_none_or(|m| m.keys().all(|k| k == "tiles"))
                && o.get("command")
                    .and_then(|m| m.as_object())
                    .is_none_or(|m| m.keys().all(|k| k == "theme"))
        })
    });
    if ours {
        Ok(())
    } else {
        Err(format!(
            "OpenCode's {OPENCODE_FILE} holds settings of yours; add the tiles server to opencode.json by hand (tiles install --dry-run opencode shows it) or use tiles run opencode"
        ))
    }
}

/// Codex's config file, in `CODEX_HOME` (`~/.codex` by default).
pub const CODEX_FILE: &str = "config.toml";

/// The Codex server setting that decides when Codex asks before a tool call;
/// codex-cli 0.163.0 takes `auto`, `prompt`, `writes` or `approve`.
const CODEX_APPROVAL: &str = "default_tools_approval_mode";

fn codex_tiles_approval(config: &toml::Table) -> Option<&toml::Value> {
    config.get("mcp_servers")?.get("tiles")?.get(CODEX_APPROVAL)
}

/// Refuses an install that would drop a `default_tools_approval_mode` the
/// user set for the tiles server: `codex mcp add` writes the server's table
/// afresh. A `prompt` there is the one Tiles writes, so it may be replaced.
///
/// # Errors
///
/// What the file holds and what to do instead.
pub fn codex_setting_is_ours(existing: Option<&str>) -> Result<(), String> {
    let Some(config) = existing.and_then(|t| t.parse::<toml::Table>().ok()) else {
        return Ok(());
    };
    match codex_tiles_approval(&config) {
        Some(mode) if mode.as_str() != Some("prompt") => Err(format!(
            "Codex's {CODEX_FILE} sets {CODEX_APPROVAL} for the tiles server, which codex mcp add would drop; Tiles leaves it: run the command tiles install --dry-run codex shows yourself"
        )),
        _ => Ok(()),
    }
}

/// The Codex server setting for how long Codex waits for one tool call, in
/// seconds; codex-cli 0.163.0 reads it from the server's table.
const CODEX_TIMEOUT: &str = "tool_timeout_sec";

/// `config` with `default_tools_approval_mode = "prompt"` as the first key of
/// its `[mcp_servers.tiles]` table and every other byte kept. A config that
/// already says `prompt` comes back unchanged.
///
/// # Errors
///
/// When the file does not parse, has no `[mcp_servers.tiles]` header line,
/// sets another approval mode, or would mean anything else after the edit.
pub fn codex_prompt(config: &str) -> Result<String, String> {
    let by_hand = format!(
        "add {CODEX_APPROVAL} = \"prompt\" under [mcp_servers.tiles] in Codex's {CODEX_FILE} yourself"
    );
    let parsed = config
        .parse::<toml::Table>()
        .map_err(|_| format!("Codex's {CODEX_FILE} does not parse; {by_hand}"))?;
    match codex_tiles_approval(&parsed) {
        Some(mode) if mode.as_str() == Some("prompt") => return Ok(config.to_string()),
        Some(_) => {
            return Err(format!(
                "Codex's {CODEX_FILE} sets {CODEX_APPROVAL} for the tiles server; Tiles leaves it"
            ));
        }
        None => {}
    }
    codex_tiles_set(
        config,
        parsed,
        CODEX_APPROVAL,
        toml::Value::String("prompt".into()),
        &by_hand,
    )
}

/// The `tool_timeout_sec` Codex's config gives the tiles server, in whole
/// seconds.
fn codex_tiles_timeout(config: &toml::Table) -> Option<f64> {
    let value = config
        .get("mcp_servers")?
        .get("tiles")?
        .get(CODEX_TIMEOUT)?;
    #[allow(clippy::cast_precision_loss)] // A timeout in seconds.
    value
        .as_integer()
        .map(|n| n as f64)
        .or_else(|| value.as_float())
}

/// `config` with `tool_timeout_sec = secs` in its `[mcp_servers.tiles]`
/// table and every other byte kept: a shorter timeout's line replaced, or
/// a new first line under the header. A config that already waits `secs`
/// or longer comes back unchanged, so Tiles only ever lengthens the wait.
///
/// # Errors
///
/// When the file does not parse, has no `[mcp_servers.tiles]` header line,
/// or would mean anything else after the edit.
pub fn codex_timeout(config: &str, secs: u64) -> Result<String, String> {
    let by_hand = format!(
        "add {CODEX_TIMEOUT} = {secs} under [mcp_servers.tiles] in Codex's {CODEX_FILE} yourself"
    );
    let parsed = config
        .parse::<toml::Table>()
        .map_err(|_| format!("Codex's {CODEX_FILE} does not parse; {by_hand}"))?;
    #[allow(clippy::cast_precision_loss)] // A timeout in seconds.
    if codex_tiles_timeout(&parsed).is_some_and(|t| t >= secs as f64) {
        return Ok(config.to_string());
    }
    let value = i64::try_from(secs).map_err(|_| format!("{secs} s is too long for Codex"))?;
    codex_tiles_set(
        config,
        parsed,
        CODEX_TIMEOUT,
        toml::Value::Integer(value),
        &by_hand,
    )
}

/// `config`, whose parse is `want`, with `key = value` in its
/// `[mcp_servers.tiles]` table and every other byte kept: the key's own
/// line in that table replaced when it has one, else a new first line
/// under the header. The edit stands only if the file then parses to
/// `want` with the key set.
fn codex_tiles_set(
    config: &str,
    mut want: toml::Table,
    key: &str,
    value: toml::Value,
    by_hand: &str,
) -> Result<String, String> {
    let shown = match &value {
        toml::Value::String(text) => format!("\"{text}\""),
        other => other
            .as_integer()
            .map_or_else(String::new, |n| n.to_string()),
    };
    let line = format!("{key} = {shown}\n");
    let Some(tiles) = want
        .get_mut("mcp_servers")
        .and_then(|s| s.get_mut("tiles"))
        .and_then(toml::Value::as_table_mut)
    else {
        return Err(format!(
            "Codex's {CODEX_FILE} has no tiles server; {by_hand}"
        ));
    };
    let had = tiles.insert(key.into(), value).is_some();
    let mut end = 0;
    let header = config.split_inclusive('\n').find_map(|line| {
        end += line.len();
        (line.trim() == "[mcp_servers.tiles]").then_some(end)
    });
    let Some(at) = header else {
        return Err(format!(
            "Codex's {CODEX_FILE} has no [mcp_servers.tiles] line; {by_hand}"
        ));
    };
    let edited = if had {
        // The key's own line, before the table's end at the next header.
        let mut start = at;
        let old = config[at..]
            .split_inclusive('\n')
            .take_while(|l| !l.trim_start().starts_with('['))
            .find_map(|l| {
                let here = start;
                start += l.len();
                l.trim_start()
                    .strip_prefix(key)
                    .is_some_and(|rest| rest.trim_start().starts_with('='))
                    .then_some(here..here + l.len())
            });
        let Some(old) = old else {
            return Err(format!(
                "cannot change the line in Codex's {CODEX_FILE} safely; {by_hand}"
            ));
        };
        format!("{}{line}{}", &config[..old.start], &config[old.end..])
    } else {
        let newline = if config[..at].ends_with('\n') {
            ""
        } else {
            "\n"
        };
        format!("{}{newline}{line}{}", &config[..at], &config[at..])
    };
    if edited.parse::<toml::Table>().ok() == Some(want) {
        Ok(edited)
    } else {
        Err(format!(
            "cannot add the line to Codex's {CODEX_FILE} safely; {by_hand}"
        ))
    }
}

/// A Tiles-owned `OpenCode` config with every Tiles tool set to `ask`, for a
/// machine where Omni or a warehouse is now configured: `tiles_chart` at the
/// `allow` an install without either wrote, and a data tool of `tools` (the
/// configured sources' own, such as `tiles_omni_topics`) the file does not
/// list, become `ask`. Any other value is the user's and stays. `None` when
/// the file is not Tiles' own or already asks.
#[must_use]
pub fn opencode_ask(existing: &[u8], tools: &[&str]) -> Option<Vec<u8>> {
    opencode_file_is_ours(Some(existing)).ok()?;
    let mut config: serde_json::Value = serde_json::from_slice(existing).ok()?;
    let permission = config
        .as_object_mut()?
        .entry("permission")
        .or_insert_with(|| json!({}))
        .as_object_mut()?;
    let mut changed = false;
    for tool in [TOOL_NAME, THEME_TOOL].iter().chain(tools) {
        let key = addons::opencode_permission(tool);
        let preapproved = (*tool == TOOL_NAME || *tool == THEME_TOOL)
            && permission.get(&key).and_then(serde_json::Value::as_str) == Some("allow");
        if preapproved || !permission.contains_key(&key) {
            permission.insert(key, json!("ask"));
            changed = true;
        }
    }
    changed.then(|| config.to_string().into_bytes())
}

/// Whether Codex's config has a tiles server with no approval mode, so Codex
/// runs its tools as their hints say.
fn codex_needs_prompt(config: &str) -> bool {
    config.parse::<toml::Table>().is_ok_and(|c| {
        c.get("mcp_servers").and_then(|s| s.get("tiles")).is_some()
            && codex_tiles_approval(&c).is_none()
    })
}

/// Whether Codex's config has a tiles server that waits less than `secs`
/// for one tool call.
fn codex_needs_timeout(config: &str, secs: u64) -> bool {
    #[allow(clippy::cast_precision_loss)] // A timeout in seconds.
    config.parse::<toml::Table>().is_ok_and(|c| {
        c.get("mcp_servers").and_then(|s| s.get("tiles")).is_some()
            && !codex_tiles_timeout(&c).is_some_and(|t| t >= secs as f64)
    })
}

/// The installs under `dirs` that still run a Tiles tool without asking, for
/// a machine where Omni or a warehouse is configured, whose data tools are
/// `tools`, or whose Codex stops waiting sooner than `timeout`, the seconds
/// a configured source's chart call may take, each with the command that
/// fixes it.
#[must_use]
pub fn stale_installs(dirs: &Dirs, tools: &[&str], timeout: Option<u64>) -> Vec<String> {
    let mut out = Vec::new();
    if fs::read(dirs.opencode_file()).is_ok_and(|b| opencode_ask(&b, tools).is_some()) {
        out.push("OpenCode's Tiles install runs tiles_chart or a data tool without asking; run tiles install opencode".to_string());
    }
    if fs::read_to_string(dirs.codex_file()).is_ok_and(|c| codex_needs_prompt(&c)) {
        out.push(format!(
            "Codex's tiles server has no {CODEX_APPROVAL}, so Codex may run its tools without asking; run tiles install codex"
        ));
    }
    if let Some(secs) = timeout
        && let Ok(config) = fs::read_to_string(dirs.codex_file())
        && codex_needs_timeout(&config, secs)
    {
        let set = config
            .parse::<toml::Table>()
            .is_ok_and(|c| codex_tiles_timeout(&c).is_some());
        let has = if set {
            format!("a {CODEX_TIMEOUT} under {secs}")
        } else {
            format!("no {CODEX_TIMEOUT}")
        };
        out.push(format!(
            "Codex's tiles server has {has}, so Codex may stop waiting before a query ends; run tiles install codex"
        ));
    }
    out
}

/// Gives the installs Tiles made under `dirs` the approval settings an
/// install with Omni or a warehouse configured writes: every Tiles tool,
/// the data tools `tools` among them, `ask` in the `OpenCode` config Tiles
/// owns, `prompt` for Codex's tiles server, and, with `timeout`, the
/// seconds a configured source's chart call may take, a `tool_timeout_sec`
/// at least that long. A file Tiles does not own and a value the user set
/// stay as they are, but for a shorter timeout. Returns one line per
/// change or failure, naming only paths.
pub fn reapply_approvals(dirs: &Dirs, tools: &[&str], timeout: Option<u64>) -> Vec<String> {
    let show = |p: &Path| tiles_term::sanitize::line(&p.display().to_string(), 200);
    let mut out = Vec::new();
    let opencode = dirs.opencode_file();
    if let Some(edited) = fs::read(&opencode)
        .ok()
        .and_then(|b| opencode_ask(&b, tools))
    {
        out.push(match addons::write_file(&opencode, &edited) {
            Ok(()) => format!("set the Tiles tools to ask in {}", show(&opencode)),
            Err(e) => e,
        });
    }
    let codex = dirs.codex_file();
    if fs::read_to_string(&codex).is_ok_and(|c| codex_needs_prompt(&c)) {
        out.push(match edit_codex(&codex, &codex_prompt) {
            Ok(()) => format!(
                "set {CODEX_APPROVAL} = \"prompt\" for Codex's tiles server in {}",
                show(&codex)
            ),
            Err(e) => e,
        });
    }
    if let Some(secs) = timeout
        && fs::read_to_string(&codex).is_ok_and(|c| codex_needs_timeout(&c, secs))
    {
        out.push(match edit_codex(&codex, &|c| codex_timeout(c, secs)) {
            Ok(()) => format!(
                "set {CODEX_TIMEOUT} = {secs} for Codex's tiles server in {}",
                show(&codex)
            ),
            Err(e) => e,
        });
    }
    out
}

/// Applies `edit`, [`codex_prompt`] or [`codex_timeout`], to the file at
/// `path`, writing it only if it changes. A symlinked config is edited
/// where it points.
fn edit_codex(path: &Path, edit: &dyn Fn(&str) -> Result<String, String>) -> Result<(), String> {
    let show = || tiles_term::sanitize::line(&path.display().to_string(), 200);
    let path = fs::canonicalize(path).map_err(|e| format!("cannot read {}: {e}", show()))?;
    let config = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", show()))?;
    let edited = edit(&config)?;
    if edited == config {
        return Ok(());
    }
    addons::write_file(&path, edited.as_bytes())
}

/// One step of an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Write a file.
    Write(PathBuf, Vec<u8>),
    /// Run the harness's own command.
    Run(String, Vec<String>),
    /// Make Codex ask before each Tiles tool call, in this config file.
    CodexPrompt(PathBuf),
    /// Make Codex wait at least this many seconds for a Tiles tool call, in
    /// this config file.
    CodexTimeout(PathBuf, u64),
}

impl Step {
    /// The step in words, for `--dry-run` and the report.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Step::Write(path, _) => format!("write {}", path.display()),
            Step::Run(program, args) => format!("run {program} {}", args.join(" ")),
            Step::CodexPrompt(path) => format!(
                "set {CODEX_APPROVAL} = \"prompt\" under [mcp_servers.tiles] in {}",
                path.display()
            ),
            Step::CodexTimeout(path, secs) => format!(
                "set {CODEX_TIMEOUT} = {secs} under [mcp_servers.tiles] in {}",
                path.display()
            ),
        }
    }
}

/// Who draws for an installed harness.
#[must_use]
pub fn installed_draw(harness: Harness) -> Draw {
    match harness {
        Harness::Pi => Draw::Image,
        _ => Draw::Text,
    }
}

/// The steps that install `harness` for the user whose home is `home`, with
/// `config_home` the XDG config directory and `codex_home` Codex's.
#[must_use]
pub fn steps(
    harness: Harness,
    home: &Path,
    config_home: &Path,
    codex_home: &Path,
    spec: &Spec,
) -> Vec<Step> {
    let tiles = spec.tiles.display().to_string();
    let draw_env = format!("TILES_DRAW={}", spec.draw.name());
    match harness {
        Harness::Pi => {
            let dir = pi_extension(home);
            vec![
                Step::Write(
                    dir.join("index.ts"),
                    addons::SOURCES[0].1.as_bytes().to_vec(),
                ),
                Step::Write(
                    dir.join("tool.json"),
                    addons::pi_tool(spec).to_string().into_bytes(),
                ),
            ]
        }
        Harness::OpenCode => {
            let dir = config_home.join("opencode");
            let instructions = dir.join("tiles-instructions.md");
            let config =
                addons::opencode_config(&tiles, &spec.env, &spec.reach, &[], &instructions);
            vec![
                Step::Write(
                    instructions,
                    tiles_mcp::instructions::session(spec.draw, spec.companion, &spec.reach)
                        .into_bytes(),
                ),
                Step::Write(dir.join(OPENCODE_FILE), config.to_string().into_bytes()),
            ]
        }
        Harness::Claude => vec![Step::Run(
            "claude".into(),
            [
                "mcp", "add", "--scope", "user", "tiles", "-e", &draw_env, "--", &tiles, "mcp",
            ]
            .map(String::from)
            .to_vec(),
        )],
        Harness::Codex => {
            let mut steps = vec![Step::Run(
                "codex".into(),
                [
                    "mcp", "add", "tiles", "--env", &draw_env, "--", &tiles, "mcp",
                ]
                .map(String::from)
                .to_vec(),
            )];
            if spec.reach.fetches() {
                steps.push(Step::CodexPrompt(codex_home.join(CODEX_FILE)));
            }
            if let Some(timeout) = spec.reach.tool_timeout() {
                steps.push(Step::CodexTimeout(
                    codex_home.join(CODEX_FILE),
                    harness::wait_secs(timeout),
                ));
            }
            steps.push(Step::Write(
                codex_home.join("skills/tiles-theme/SKILL.md"),
                addons::CODEX_THEME_SKILL.as_bytes().to_vec(),
            ));
            steps
        }
    }
}

/// The add-on spec an install of `harness` writes: the `tiles` at `tiles`,
/// who draws for an installed harness, what a session can fetch from, and
/// the engine's forms.
#[must_use]
pub fn spec(harness: Harness, tiles: PathBuf, reach: Reach, forms: Vec<FormEntry>) -> Spec {
    let draw = installed_draw(harness);
    Spec {
        tiles,
        draw,
        companion: false,
        reach,
        env: vec![("TILES_DRAW".to_string(), draw.name().to_string())],
        forms,
    }
}

/// Refuses an install that would overwrite a file of the user's:
/// [`opencode_file_is_ours`] for `OpenCode`, [`codex_setting_is_ours`] for
/// Codex.
///
/// # Errors
///
/// What the file holds and what to do instead.
pub fn refusal(harness: Harness, dirs: &Dirs) -> Result<(), String> {
    match harness {
        Harness::OpenCode => opencode_file_is_ours(fs::read(dirs.opencode_file()).ok().as_deref()),
        Harness::Codex => {
            codex_setting_is_ours(fs::read_to_string(dirs.codex_file()).ok().as_deref())
        }
        Harness::Pi | Harness::Claude => Ok(()),
    }
}

/// Runs one harness command: program and arguments.
pub type RunCommand<'a> = dyn Fn(&str, &[String]) -> Result<(), String> + 'a;

/// Runs `program` with `args` on this terminal, as [`apply`]'s commands run.
///
/// # Errors
///
/// When it cannot start or exits unsuccessfully.
pub fn run_program(program: &str, args: &[String]) -> Result<(), String> {
    let status = std::process::Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

/// Carries out the steps, running commands through `run`.
///
/// # Errors
///
/// The first step that failed.
pub fn apply(steps: &[Step], run: &RunCommand<'_>) -> Result<(), String> {
    for step in steps {
        match step {
            Step::Write(path, bytes) => addons::write_file(path, bytes)?,
            Step::Run(program, args) => run(program, args)?,
            Step::CodexPrompt(path) => edit_codex(path, &codex_prompt)?,
            Step::CodexTimeout(path, secs) => edit_codex(path, &|c| codex_timeout(c, *secs))?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use tiles_mcp::instructions::Draw;

    use super::{Dirs, Step, apply, installed_draw, reapply_approvals, stale_installs, steps};
    use crate::addons::Spec;
    use crate::harness::Harness;

    fn spec(h: Harness) -> Spec {
        Spec {
            tiles: PathBuf::from("/opt/tiles"),
            draw: installed_draw(h),
            companion: false,
            reach: tiles_mcp::source::Reach::default(),
            env: vec![("TILES_DRAW".into(), installed_draw(h).name().into())],
            forms: vec![],
        }
    }

    #[test]
    fn pi_and_opencode_get_their_add_on_files() {
        let home = Path::new("/home/u");
        let s = steps(
            Harness::Pi,
            home,
            &home.join(".config"),
            home,
            &spec(Harness::Pi),
        );
        let paths: Vec<String> = s.iter().map(Step::describe).collect();
        assert_eq!(
            paths,
            [
                "write /home/u/.pi/agent/extensions/tiles/index.ts",
                "write /home/u/.pi/agent/extensions/tiles/tool.json"
            ]
        );
        let s = steps(
            Harness::OpenCode,
            home,
            Path::new("/xdg"),
            home,
            &spec(Harness::OpenCode),
        );
        assert_eq!(s[0].describe(), "write /xdg/opencode/tiles-instructions.md");
        assert_eq!(s[1].describe(), "write /xdg/opencode/config.json");
        let Step::Write(_, config) = &s[1] else {
            panic!()
        };
        let config: serde_json::Value = serde_json::from_slice(config).unwrap();
        assert_eq!(
            config["mcp"]["tiles"]["command"],
            serde_json::json!(["/opt/tiles", "mcp"])
        );
        assert_eq!(config["mcp"]["tiles"]["environment"]["TILES_DRAW"], "text");
        assert_eq!(
            config["instructions"][0],
            "/xdg/opencode/tiles-instructions.md"
        );
    }

    #[test]
    fn an_opencode_config_json_of_the_users_is_never_overwritten() {
        use super::opencode_file_is_ours;
        assert!(opencode_file_is_ours(None).is_ok());
        assert!(
            opencode_file_is_ours(Some(br#"{"mcp":{"tiles":{}},"instructions":["x"]}"#)).is_ok()
        );
        assert!(opencode_file_is_ours(Some(br#"{"theme":"x"}"#)).is_err());
        assert!(opencode_file_is_ours(Some(br#"{"mcp":{"github":{}}}"#)).is_err());
        assert!(opencode_file_is_ours(Some(b"// jsonc\n{}")).is_err());
    }

    #[test]
    fn claude_and_codex_use_their_own_mcp_add() {
        let home = Path::new("/home/u");
        assert_eq!(
            steps(Harness::Claude, home, home, home, &spec(Harness::Claude))[0].describe(),
            "run claude mcp add --scope user tiles -e TILES_DRAW=text -- /opt/tiles mcp"
        );
        let codex = steps(Harness::Codex, home, home, home, &spec(Harness::Codex));
        let described: Vec<String> = codex.iter().map(Step::describe).collect();
        assert_eq!(
            described,
            [
                "run codex mcp add tiles --env TILES_DRAW=text -- /opt/tiles mcp",
                "write /home/u/skills/tiles-theme/SKILL.md",
            ]
        );
        assert_eq!(installed_draw(Harness::Pi), Draw::Image);
    }

    #[test]
    fn codex_with_omni_is_set_to_ask_before_each_call() {
        let home = Path::new("/home/u");
        let mut s = spec(Harness::Codex);
        s.reach.omni = true;
        let codex = steps(Harness::Codex, home, home, Path::new("/codex"), &s);
        let described: Vec<String> = codex.iter().map(Step::describe).collect();
        assert_eq!(
            described,
            [
                "run codex mcp add tiles --env TILES_DRAW=text -- /opt/tiles mcp",
                "set default_tools_approval_mode = \"prompt\" under [mcp_servers.tiles] in /codex/config.toml",
                "write /codex/skills/tiles-theme/SKILL.md",
            ]
        );
    }

    #[test]
    fn a_configured_warehouse_makes_codex_prompt_and_opencode_ask() {
        let home = Path::new("/home/u");
        let mut s = spec(Harness::Codex);
        s.reach.offers.push(tiles_mcp::source::Offer {
            name: "warehouse",
            origin: "BigQuery",
            tools: vec![tiles_mcp::source::Tool {
                name: "tiles_warehouse_models",
                title: "t",
                description: "d",
                input: serde_json::json!({}),
            }],
            instruction: "Pass data.warehouse.".into(),
            chart_line: String::new(),
            tool_timeout: std::time::Duration::from_secs(300),
        });
        let codex = steps(Harness::Codex, home, home, Path::new("/codex"), &s);
        assert!(
            codex
                .iter()
                .any(|step| matches!(step, Step::CodexPrompt(_)))
        );
        assert!(
            codex.contains(&Step::CodexTimeout(
                Path::new("/codex/config.toml").into(),
                330
            )),
            "Codex waits out the warehouse's 300 s budget: {codex:?}"
        );
        assert_eq!(
            Step::CodexTimeout(Path::new("/codex/config.toml").into(), 330).describe(),
            "set tool_timeout_sec = 330 under [mcp_servers.tiles] in /codex/config.toml"
        );
        let opencode = steps(Harness::OpenCode, home, home, Path::new("/codex"), &s);
        let Step::Write(_, config) = &opencode[1] else {
            panic!("the config file: {opencode:?}");
        };
        let config: serde_json::Value = serde_json::from_slice(config).unwrap();
        for tool in [
            "tiles_tiles_chart",
            "tiles_tiles_theme",
            "tiles_tiles_warehouse_models",
        ] {
            assert_eq!(config["permission"][tool], "ask", "{tool}");
        }
        let Step::Write(_, instructions) = &opencode[0] else {
            panic!("the instruction file");
        };
        assert!(String::from_utf8_lossy(instructions).contains("Pass data.warehouse."));
    }

    /// What `codex mcp add tiles --env TILES_DRAW=text -- /opt/tiles mcp`
    /// (codex-cli 0.163.0) writes over a config of the user's.
    const CODEX_ADDED: &str = "# my config\nmodel = \"o3\"\n\n[mcp_servers.other]\ncommand = \"x\"\n\n[mcp_servers.tiles]\ncommand = \"/opt/tiles\"\nargs = [\"mcp\"]\n\n[mcp_servers.tiles.env]\nTILES_DRAW = \"text\"\n";

    #[test]
    fn the_codex_prompt_line_is_added_and_nothing_else_changes() {
        use super::codex_prompt;
        let line = "default_tools_approval_mode = \"prompt\"\n";
        let edited = codex_prompt(CODEX_ADDED).unwrap();
        assert_eq!(edited.replacen(line, "", 1), CODEX_ADDED);
        let config: toml::Table = edited.parse().unwrap();
        assert_eq!(
            config["mcp_servers"]["tiles"]["default_tools_approval_mode"].as_str(),
            Some("prompt")
        );
        assert_eq!(
            config["mcp_servers"]["other"]["command"].as_str(),
            Some("x")
        );
        assert_eq!(
            codex_prompt(&edited).unwrap(),
            edited,
            "a second run changes nothing"
        );
        let unterminated = "[mcp_servers.tiles]";
        let edited = codex_prompt(unterminated).unwrap();
        assert_eq!(edited, format!("[mcp_servers.tiles]\n{line}"));
        assert!(codex_prompt("model = \"o3\"\n").is_err(), "no tiles server");
        assert!(codex_prompt("mcp_servers = { tiles = { command = \"t\" } }\n").is_err());
        assert!(codex_prompt("[mcp_servers.tiles\n").is_err());
    }

    #[test]
    fn the_codex_timeout_line_is_added_or_lengthened_and_nothing_else_changes() {
        use super::{codex_prompt, codex_timeout};
        let line = "tool_timeout_sec = 330\n";
        let edited = codex_timeout(CODEX_ADDED, 330).unwrap();
        assert_eq!(edited.replacen(line, "", 1), CODEX_ADDED);
        let config: toml::Table = edited.parse().unwrap();
        assert_eq!(
            config["mcp_servers"]["tiles"]["tool_timeout_sec"].as_integer(),
            Some(330)
        );
        assert_eq!(
            codex_timeout(&edited, 330).unwrap(),
            edited,
            "a second run changes nothing"
        );
        let both = codex_prompt(&edited).unwrap();
        assert_eq!(codex_timeout(&both, 330).unwrap(), both);
        let short = both.replace(line, "tool_timeout_sec = 60 # mine\n");
        let raised = codex_timeout(&short, 330).unwrap();
        assert_eq!(raised, both, "a shorter wait is lengthened in place");
        let long = both.replace(line, "tool_timeout_sec = 900.5\n");
        assert_eq!(
            codex_timeout(&long, 330).unwrap(),
            long,
            "a longer wait stays"
        );
        assert!(
            codex_timeout("model = \"o3\"\n", 330).is_err(),
            "no tiles server"
        );
        let dotted = "[mcp_servers]\ntiles.command = \"t\"\ntiles.tool_timeout_sec = 60\n";
        assert!(
            codex_timeout(dotted, 330).is_err(),
            "no line to change safely"
        );
    }

    #[test]
    fn a_codex_approval_setting_of_the_users_is_never_touched() {
        use super::{codex_prompt, codex_setting_is_ours};
        let theirs = CODEX_ADDED.replace(
            "[mcp_servers.tiles]\n",
            "[mcp_servers.tiles]\ndefault_tools_approval_mode = \"approve\"\n",
        );
        assert!(codex_setting_is_ours(Some(&theirs)).is_err());
        assert!(codex_prompt(&theirs).is_err());
        let ours = codex_prompt(CODEX_ADDED).unwrap();
        assert!(codex_setting_is_ours(Some(&ours)).is_ok());
        assert!(codex_setting_is_ours(Some(CODEX_ADDED)).is_ok());
        assert!(codex_setting_is_ours(None).is_ok());
    }

    #[test]
    fn the_codex_prompt_step_edits_the_file_a_symlink_points_to() {
        use std::os::unix::fs::symlink;
        let dir = std::env::temp_dir().join(format!("tiles-codex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dotfiles")).unwrap();
        let target = dir.join("dotfiles/config.toml");
        std::fs::write(&target, CODEX_ADDED).unwrap();
        let link = dir.join("config.toml");
        symlink(&target, &link).unwrap();
        let run = |_: &str, _: &[String]| Ok(());
        apply(&[Step::CodexPrompt(link.clone())], &run).unwrap();
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let edited = std::fs::read_to_string(&target).unwrap();
        assert_eq!(edited, super::codex_prompt(CODEX_ADDED).unwrap());
        let missing = apply(&[Step::CodexPrompt(dir.join("none.toml"))], &run);
        assert!(missing.unwrap_err().starts_with("cannot read"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_writes_files_and_runs_commands_in_order() {
        let dir = std::env::temp_dir().join(format!("tiles-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ran = RefCell::new(Vec::new());
        let run = |p: &str, a: &[String]| {
            ran.borrow_mut().push(format!("{p} {}", a.join(" ")));
            Ok(())
        };
        let s = vec![
            Step::Write(dir.join("a/b.ts"), b"x".to_vec()),
            Step::Run("codex".into(), vec!["mcp".into()]),
        ];
        apply(&s, &run).unwrap();
        assert_eq!(std::fs::read(dir.join("a/b.ts")).unwrap(), b"x");
        assert_eq!(*ran.borrow(), ["codex mcp"]);
        let failing = |_: &str, _: &[String]| Err("codex: not found".to_string());
        assert_eq!(apply(&s[1..], &failing).unwrap_err(), "codex: not found");
        let _ = std::fs::remove_dir_all(&dir);
    }

    struct Keychain(RefCell<Vec<(String, String)>>);

    impl tiles_omni::SecretStore for Keychain {
        fn get(&self, account: &str) -> Result<Option<String>, tiles_omni::ConfigError> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|(a, _)| a == account)
                .map(|(_, v)| v.clone()))
        }
        fn put(&self, account: &str, value: &str) -> Result<(), tiles_omni::ConfigError> {
            self.0.borrow_mut().push((account.into(), value.into()));
            Ok(())
        }
    }

    fn fake_dirs(name: &str) -> Dirs {
        let root = std::env::temp_dir().join(format!("tiles-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Dirs {
            home: root.join("home"),
            config_home: root.join("xdg"),
            codex_home: root.join("codex"),
        }
    }

    /// Installs `OpenCode` and Codex with no Omni configured, `codex mcp add`
    /// faked by writing what it writes.
    fn install_without_omni(dirs: &Dirs) {
        let codex_file = dirs.codex_home.join("config.toml");
        let run = |program: &str, _: &[String]| {
            assert_eq!(program, "codex");
            crate::addons::write_file(&codex_file, CODEX_ADDED.as_bytes())
        };
        for h in [Harness::OpenCode, Harness::Codex] {
            let s = steps(h, &dirs.home, &dirs.config_home, &dirs.codex_home, &spec(h));
            apply(&s, &run).unwrap();
        }
    }

    fn opencode_permission(dirs: &Dirs) -> serde_json::Value {
        let bytes = std::fs::read(dirs.config_home.join("opencode/config.json")).unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["permission"].clone()
    }

    const KEY: &str = "omni_planted_key_7Yq2xL9vR4";
    const OMNI_TOOLS: &[&str] = &[tiles_mcp::OMNI_TOPICS_TOOL, tiles_mcp::OMNI_FIELDS_TOOL];

    #[test]
    fn set_key_after_an_install_without_omni_makes_every_tiles_tool_ask() {
        let dirs = fake_dirs("install-then-set-key");
        install_without_omni(&dirs);
        assert_eq!(
            opencode_permission(&dirs),
            serde_json::json!({ "tiles_tiles_chart": "allow", "tiles_tiles_theme": "allow" })
        );
        assert_eq!(stale_installs(&dirs, OMNI_TOOLS, None).len(), 2);
        let keychain = Keychain(RefCell::new(Vec::new()));
        let said = crate::omni::set_key(
            "https://acme.omniapp.co",
            &mut format!("{KEY}\n").as_bytes(),
            &keychain,
            Some(&dirs),
        )
        .unwrap();
        assert_eq!(said.len(), 3, "{said:?}");
        assert!(
            said[1].starts_with("set the Tiles tools to ask in "),
            "{said:?}"
        );
        assert!(
            said[2].starts_with(
                "set default_tools_approval_mode = \"prompt\" for Codex's tiles server in "
            ),
            "{said:?}"
        );
        assert!(said.iter().all(|l| !l.contains(KEY) && !l.contains("7Yq2")));
        assert_eq!(
            opencode_permission(&dirs),
            serde_json::json!({
                "tiles_tiles_chart": "ask",
                "tiles_tiles_theme": "ask",
                "tiles_tiles_omni_topics": "ask",
                "tiles_tiles_omni_fields": "ask",
            })
        );
        let codex = std::fs::read_to_string(dirs.codex_home.join("config.toml")).unwrap();
        assert_eq!(codex, super::codex_prompt(CODEX_ADDED).unwrap());
        assert_eq!(
            stale_installs(&dirs, OMNI_TOOLS, None),
            Vec::<String>::new()
        );
        assert!(
            reapply_approvals(&dirs, OMNI_TOOLS, None).is_empty(),
            "a second run changes nothing"
        );
        let _ = std::fs::remove_dir_all(dirs.home.parent().unwrap());
    }

    #[test]
    fn reapplying_approvals_keeps_the_users_own_values_and_files() {
        let dirs = fake_dirs("reapply-keeps-user-values");
        let opencode = dirs.config_home.join("opencode/config.json");
        let codex = dirs.codex_home.join("config.toml");
        let mine =
            br#"{"permission":{"tiles_tiles_chart":"deny","tiles_tiles_omni_topics":"allow"}}"#;
        crate::addons::write_file(&opencode, mine).unwrap();
        let theirs = CODEX_ADDED.replace(
            "[mcp_servers.tiles]\n",
            "[mcp_servers.tiles]\ndefault_tools_approval_mode = \"approve\"\n",
        );
        crate::addons::write_file(&codex, theirs.as_bytes()).unwrap();
        let changed = reapply_approvals(&dirs, OMNI_TOOLS, None);
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert_eq!(
            opencode_permission(&dirs),
            serde_json::json!({
                "tiles_tiles_chart": "deny",
                "tiles_tiles_theme": "ask",
                "tiles_tiles_omni_topics": "allow",
                "tiles_tiles_omni_fields": "ask",
            })
        );
        assert_eq!(std::fs::read_to_string(&codex).unwrap(), theirs);
        let not_ours = br#"{"theme":"x","permission":{"tiles_tiles_chart":"allow"}}"#;
        crate::addons::write_file(&opencode, not_ours).unwrap();
        assert_eq!(
            reapply_approvals(&dirs, OMNI_TOOLS, None),
            Vec::<String>::new()
        );
        assert_eq!(std::fs::read(&opencode).unwrap(), not_ours);
        assert_eq!(
            stale_installs(&dirs, OMNI_TOOLS, None),
            Vec::<String>::new()
        );
        let _ = std::fs::remove_dir_all(dirs.home.parent().unwrap());
    }
}
