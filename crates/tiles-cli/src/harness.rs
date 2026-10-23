//! How `tiles run <harness>` launches each harness with Tiles loaded, through
//! the harness's own launch options, and whether the wrapper draws (ADR-004,
//! ADR-011). Pure: the plan is built from paths, the detected terminal and the
//! user's own arguments, so every harness's command line is tested.
//!
//! | Harness | Loaded through | Who draws the card |
//! |---|---|---|
//! | Pi | `-e <add-on>`: an extension that registers `tiles_chart` and its renderer | Pi itself on the placeholder route (kitty, Ghostty, iTerm2); the wrapper elsewhere |
//! | Claude Code | `--mcp-config`, `--append-system-prompt`, `--allowedTools` (see [`pre_approved`]), `--settings` (see [`claude_settings`]), and `--plugin-dir <mod>` in kitty and Ghostty | the mod's `Image` in kitty and Ghostty; the wrapper elsewhere |
//! | Codex | `-c mcp_servers.tiles.*` (tools set to prompt as [`pre_approved`] says), `-c developer_instructions` | the wrapper |
//! | `OpenCode` | `OPENCODE_CONFIG_CONTENT` with the MCP server, its tool permissions, the cache denied (see [`opencode_deny`]) and the instruction file, and `--standalone` for a launcher with a background service ([`crate::opencode`]) | the wrapper |
//!
//! On a terminal with no images every harness gets the text route: the
//! summary and the saved PNG's path, with no wrapper.
//!
//! Claude Code and `OpenCode` are told to keep the model's file and shell
//! tools out of the card cache, where every card's picture sits (ADR-028).
//! `tiles run` passes Pi and Codex no such rule.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::json;
use tiles_mcp::instructions::{self, Draw};
use tiles_mcp::source::Reach;
use tiles_mcp::{THEME_TOOL, TOOL_NAME};
use tiles_term::detect::{Detected, Route, Terminal};

use crate::motion::Motion;
use crate::wrap::Launch;

/// A harness `tiles run` knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    /// Pi.
    Pi,
    /// `OpenCode`.
    OpenCode,
    /// Claude Code.
    Claude,
    /// Codex.
    Codex,
}

impl Harness {
    /// Every harness.
    pub const ALL: [Harness; 4] = [
        Harness::Pi,
        Harness::OpenCode,
        Harness::Claude,
        Harness::Codex,
    ];

    /// The name on the command line, which is also its program.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Harness::Pi => "pi",
            Harness::OpenCode => "opencode",
            Harness::Claude => "claude",
            Harness::Codex => "codex",
        }
    }

    /// The harness a name means.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Harness> {
        Harness::ALL.into_iter().find(|h| h.name() == name)
    }

    /// The panel the harness keeps right of its conversation, as the
    /// terminal width above which it shows and the columns it takes.
    /// `OpenCode` 1.19.3 opens its session sidebar, 42 columns, in a terminal
    /// wider than 120 (its TUI's session route: the sidebar is `auto` while
    /// `width>120`, and the messages are `width-(sidebar?42:0)-4` wide).
    #[must_use]
    pub fn side_panel(self) -> Option<(u16, u16)> {
        match self {
            Harness::OpenCode => Some((120, 42)),
            Harness::Pi | Harness::Claude | Harness::Codex => None,
        }
    }
}

/// Who draws, and whether the wrapper runs.
#[must_use]
pub fn draw_for(harness: Harness, detected: &Detected, force_wrapper: bool) -> (Draw, bool) {
    if detected.route == Route::Text {
        return (Draw::Text, false);
    }
    if force_wrapper {
        return (Draw::Block, true);
    }
    match harness {
        // Pi's own image sits in its tool-result panel; ADR-011
        // proved it on the placeholder route only.
        Harness::Pi if detected.route == Route::KittyPlaceholders => (Draw::Image, false),
        Harness::Claude
            if detected.multiplexer.is_none()
                && matches!(detected.terminal, Terminal::Kitty | Terminal::Ghostty) =>
        {
            (Draw::Image, false)
        }
        _ => (Draw::Block, true),
    }
}

/// Whether the wrapper places a card early and plays its entrance as a
/// kitty animation: kitty itself, on the placeholder route, with no
/// multiplexer in between, where both were proven.
#[must_use]
pub fn places_early(detected: &Detected) -> bool {
    detected.terminal == Terminal::Kitty
        && detected.multiplexer.is_none()
        && detected.route == Route::KittyPlaceholders
}

/// Whether the wrapper plays a card's entrance: where it places cards early
/// ([`places_early`]), and on the pre-cut route (Warp, `WezTerm`, VS Code,
/// herdr), a frame placed per 50 ms tick (ADR-030).
#[must_use]
pub fn animates(detected: &Detected) -> bool {
    places_early(detected) || detected.route == Route::KittyPrecut
}

/// The Tiles tools the harness may run without asking the user. With Omni
/// or a warehouse configured, `tiles_chart` can fetch rows as well as draw
/// rows the model typed, and a harness cannot tell the two apart by the
/// tool's name, so no tool is pre-approved unless the user passed
/// `--allow-<name>` for every source the session reaches (`allowed`, such as
/// `omni` and `warehouse`): a model steered by text it read cannot pull
/// company data without a prompt. `tiles_theme` only writes the local
/// settings file and goes with `tiles_chart`.
#[must_use]
pub fn pre_approved(reach: &Reach, allowed: &[String]) -> Vec<&'static str> {
    if !reach.fetches() {
        return vec![TOOL_NAME, THEME_TOOL];
    }
    if !reach.all_allowed(allowed) {
        return Vec::new();
    }
    let mut tools = vec![TOOL_NAME, THEME_TOOL];
    tools.extend(reach.data_tools());
    tools
}

/// The seconds Codex waits for a Tiles tool: 120, or longer when a
/// configured source's query may run longer, plus a margin for drawing.
#[must_use]
pub fn tool_timeout_secs(reach: &Reach) -> u64 {
    reach.tool_timeout().map_or(120, wait_secs)
}

/// The seconds Codex waits for a Tiles tool call that may take `timeout`:
/// that, plus a margin for drawing, and never under 120.
#[must_use]
pub fn wait_secs(timeout: std::time::Duration) -> u64 {
    timeout.as_secs().saturating_add(30).max(120)
}

/// Everything a plan is built from.
#[derive(Debug, Clone)]
pub struct Setup {
    /// The `tiles` binary.
    pub tiles: PathBuf,
    /// Where the add-ons were written.
    pub addons: PathBuf,
    /// The card cache, which the model's own tools are kept out of.
    pub cache: PathBuf,
    /// The user's home, for the cache's home-relative spellings.
    pub home: Option<PathBuf>,
    /// The terminal and route.
    pub detected: Detected,
    /// What the session can fetch from: Omni and each configured source.
    pub reach: Reach,
    /// The sources the user passed `--allow-<name>` for (`omni` for
    /// `--allow-omni`): their tools and charts run without asking.
    pub allow: Vec<String>,
    /// The user's own Claude Code settings, from a `--settings
    /// <file-or-json>` among their arguments, read by the caller
    /// ([`settings_value`]); Tiles' rules are merged into them
    /// ([`claude_settings`]).
    pub settings: Option<serde_json::Value>,
    /// The geometry and cache variables the MCP server reads.
    pub look_env: Vec<(String, String)>,
    /// Use the wrapper even where the harness draws.
    pub force_wrapper: bool,
    /// This run's token: the server writes it into each card's geometry, so
    /// the wrapper draws and sends ahead only the cards its own session made.
    pub session: u64,
    /// The `OpenCode` command line on `PATH`, when `tiles run opencode` read
    /// its `--help`.
    pub opencode: Option<crate::opencode::Cli>,
    /// How cards enter ([`crate::motion::setting`]).
    pub motion: Motion,
    /// Whether `tiles run` keeps the companion window
    /// ([`crate::companion::wanted`]).
    pub companion: bool,
}

/// How one harness is launched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The command.
    pub launch: Launch,
    /// Whether the wrapper runs it.
    pub wrap: bool,
    /// Who draws.
    pub draw: Draw,
    /// Files the launch reads, written under the add-ons before it starts.
    pub files: Vec<(PathBuf, String)>,
}

/// The variables the MCP server needs: the draw mode and the geometry.
#[must_use]
pub fn server_env(draw: Draw, look_env: &[(String, String)]) -> Vec<(String, String)> {
    let mut env = vec![("TILES_DRAW".to_string(), draw.name().to_string())];
    env.extend(look_env.iter().cloned());
    env
}

/// [`server_env`], with the session's token wherever the wrapper draws and
/// on the text route, where the companion window shows only the session's
/// own cards (ADR-046), `TILES_COMPANION` where that window is on, so the
/// chart result says the card is already showing in it, and where the
/// wrapper plays the entrance with motion not off,
/// one card's frame count (`TILES_FRAMES`, which makes the server draw them)
/// and `TILES_MOTION` under reduce motion.
fn plan_env(setup: &Setup, draw: Draw, wrap: bool) -> Vec<(String, String)> {
    let mut env = server_env(draw, &setup.look_env);
    if wrap || draw == Draw::Text {
        env.push((
            "TILES_SESSION".to_string(),
            format!("{:016x}", setup.session),
        ));
    }
    if setup.companion && draw == Draw::Text {
        env.push((tiles_mcp::COMPANION_ENV.to_string(), "1".to_string()));
    }
    if wrap
        && animates(&setup.detected)
        && let Some((frames, _)) = setup.motion.frames()
    {
        env.push(("TILES_FRAMES".to_string(), frames.to_string()));
        if setup.motion == Motion::Reduce {
            env.push((
                "TILES_MOTION".to_string(),
                Motion::Reduce.name().to_string(),
            ));
        }
    }
    env
}

/// `s` as a TOML basic string, for Codex's `-c key=value`.
#[must_use]
pub fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn path(p: &Path) -> String {
    p.display().to_string()
}

/// Where `tiles run claude` writes [`claude_settings`].
pub const CLAUDE_SETTINGS: &str = "claude-settings.json";

/// The settings `tiles run claude` passes with `--settings`: the user's own
/// `base`, when their arguments carried `--settings <file-or-json>`
/// ([`take_settings`], [`settings_value`]), with the rules that keep the
/// model out of the card cache `cache` added to its `permissions.deny`, none
/// twice. Claude Code 2.1.303 takes one `--settings`, checks deny rules
/// before allow rules from any settings file and applies them in every
/// permission mode, bypass included. `Read(//<path>/**)` is an absolute
/// path; Claude Code applies Read rules to Grep, Glob and the shell commands
/// it knows read files (`cat`, `head`, `sed`). The `Bash` rules deny any
/// command whose text names the cache, which a command that spells the path
/// another way slips past. Tiles writes no `sandbox` entry: the cache in the
/// sandbox's `denyRead` would be lost to every shell command of the session,
/// `tiles open` and `tiles cache clear` included.
///
/// # Errors
///
/// When `base` is not an object, or its `permissions` or `permissions.deny`
/// is not the shape Claude Code's settings give it.
pub fn claude_settings(
    cache: &Path,
    home: Option<&Path>,
    base: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let dir = path(cache);
    let dir = dir.trim_end_matches('/');
    let mut rules = vec![format!("Read(/{dir}/**)"), format!("Bash(*{dir}*)")];
    if let Some(home) = home.and_then(|h| home_relative(dir, h)) {
        rules.push(format!("Bash(*~/{home}*)"));
        rules.push(format!("Bash(*$HOME/{home}*)"));
    }
    let mut settings = base.cloned().unwrap_or_else(|| json!({}));
    let deny = settings
        .as_object_mut()
        .ok_or("the settings are not a JSON object")?
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("permissions is not an object")?
        .entry("deny")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("permissions.deny is not a list")?;
    for rule in rules {
        if !deny.iter().any(|r| r == &rule) {
            deny.push(json!(rule));
        }
    }
    Ok(settings)
}

/// A `--settings <file-or-json>` among the user's own Claude Code arguments
/// `args`, taken out: the arguments without it and the last value, whose
/// settings [`claude_settings`] merges Tiles' rules into. `--settings=<value>`
/// counts too.
///
/// # Errors
///
/// When `--settings` ends the arguments with no value.
pub fn take_settings(args: &[String]) -> Result<(Vec<String>, Option<String>), String> {
    let mut rest = Vec::with_capacity(args.len());
    let mut value = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--settings" {
            value = Some(it.next().ok_or("--settings needs a file or JSON")?.clone());
        } else if let Some(v) = a.strip_prefix("--settings=") {
            value = Some(v.to_string());
        } else {
            rest.push(a.clone());
        }
    }
    Ok((rest, value))
}

/// The settings a `--settings <file-or-json>` value `spec` means, as Claude
/// Code reads it: JSON when it starts with `{`, else the file at that path,
/// read with `read`.
///
/// # Errors
///
/// When the file cannot be read or the text is not JSON; the line names
/// the value.
pub fn settings_value(
    spec: &str,
    read: &dyn Fn(&Path) -> std::io::Result<String>,
) -> Result<serde_json::Value, String> {
    let name = || tiles_term::sanitize::line(spec, 200);
    let text = if spec.trim_start().starts_with('{') {
        spec.to_string()
    } else {
        read(Path::new(spec)).map_err(|e| format!("cannot read --settings {}: {e}", name()))?
    };
    serde_json::from_str(&text).map_err(|e| format!("cannot parse --settings {}: {e}", name()))
}

/// `dir` under the user's home `home`, as the home-relative rest.
fn home_relative(dir: &str, home: &Path) -> Option<String> {
    let home = path(home);
    let rest = dir
        .strip_prefix(home.trim_end_matches('/'))?
        .strip_prefix('/')?;
    (!rest.is_empty()).then(|| rest.to_string())
}

/// The `OpenCode` permission that keeps the model out of the card cache
/// `cache`. `OpenCode` 1.19.3 asks `external_directory` with the pattern
/// `<dir>/*` before its read, grep and glob tools touch a path outside the
/// project, and before a shell command whose arguments name one
/// (`tool/external-directory.ts`, `tool/shell.ts`); a pattern's `*` matches
/// across `/`, so one rule covers the cache and everything under it.
#[must_use]
pub fn opencode_deny(cache: &Path) -> serde_json::Value {
    let dir = path(cache);
    json!({ format!("{}/*", dir.trim_end_matches('/')): "deny" })
}

/// Claude Code's arguments, and the settings file they name with its
/// contents `settings`. The caller's own `--settings` is taken out, since
/// Claude Code takes one and the file carries its settings.
fn claude_args(
    setup: &Setup,
    env: &[(String, String)],
    draw: Draw,
    instruction: String,
    user_args: &[String],
    settings: String,
) -> Result<(Vec<String>, (PathBuf, String)), String> {
    // The caller's arguments go first: --mcp-config and --allowedTools
    // take lists, so a prompt after them would be read as one more.
    let (mut args, _) = take_settings(user_args)?;
    let config = json!({ "mcpServers": { "tiles": {
        "type": "stdio",
        "command": path(&setup.tiles),
        "args": ["mcp"],
        "env": env.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>(),
    } } });
    let allowed: Vec<String> = pre_approved(&setup.reach, &setup.allow)
        .iter()
        .map(|t| format!("mcp__tiles__{t}"))
        .collect();
    args.extend([
        "--mcp-config".into(),
        config.to_string(),
        "--append-system-prompt".into(),
        instruction,
    ]);
    if !allowed.is_empty() {
        args.extend(["--allowedTools".into(), allowed.join(",")]);
    }
    let file = setup.addons.join(CLAUDE_SETTINGS);
    args.extend(["--settings".into(), path(&file)]);
    if draw == Draw::Image {
        args.extend(["--plugin-dir".into(), path(&setup.addons.join("claude"))]);
    }
    Ok((args, (file, settings)))
}

/// `OpenCode`'s arguments: the user's own, after `--standalone` when the
/// launcher's TUI would otherwise attach to its background service, which
/// never sees the session's `OPENCODE_CONFIG_CONTENT` (ADR-029).
fn opencode_args(
    cli: Option<crate::opencode::Cli>,
    user_args: &[String],
) -> Result<Vec<String>, String> {
    use crate::opencode::{Cli, STANDALONE};
    let mut args = Vec::with_capacity(user_args.len() + 1);
    if cli == Some(Cli::Launcher) {
        if user_args
            .iter()
            .any(|a| a == "--server" || a.starts_with("--server="))
        {
            return Err(format!(
                "OpenCode's --server attaches to a server that never loads this session's tiles server; drop it, and tiles run opencode starts a private one with {STANDALONE}"
            ));
        }
        if !user_args.iter().any(|a| a == STANDALONE) {
            args.push(STANDALONE.to_string());
        }
    }
    args.extend(user_args.iter().cloned());
    Ok(args)
}

/// Codex's `-c` arguments: the tiles server with its environment, a tool
/// timeout that outlasts a warehouse query, the instruction, and `prompt`
/// approvals when `prompt` says no tool is pre-approved.
fn codex_args(
    setup: &Setup,
    env: &[(String, String)],
    tiles: &str,
    instruction: &str,
    prompt: bool,
) -> Vec<String> {
    let mut args = Vec::new();
    let env_table: Vec<String> = env
        .iter()
        .map(|(k, v)| format!("{k}={}", toml_string(v)))
        .collect();
    for (key, value) in [
        ("mcp_servers.tiles.command", toml_string(tiles)),
        ("mcp_servers.tiles.args", "[\"mcp\"]".to_string()),
        (
            "mcp_servers.tiles.env",
            format!("{{{}}}", env_table.join(",")),
        ),
        (
            "mcp_servers.tiles.tool_timeout_sec",
            tool_timeout_secs(&setup.reach).to_string(),
        ),
        ("developer_instructions", toml_string(instruction)),
    ] {
        args.extend(["-c".to_string(), format!("{key}={value}")]);
    }
    if prompt {
        // Codex runs a read-only tool without asking unless the
        // server's tools are set to prompt (codex-cli 0.163.0).
        args.extend([
            "-c".to_string(),
            "mcp_servers.tiles.default_tools_approval_mode=\"prompt\"".to_string(),
        ]);
    }
    args
}

/// The variables that tell Pi's extension not to ask before a call that
/// fetches from a source the user allowed: `TILES_ALLOW_OMNI`, and
/// `TILES_ALLOW_<NAME>` for each configured source in `allowed`.
fn pi_allowed(reach: &Reach, allowed: &[String]) -> Vec<String> {
    let named = |n: &str| allowed.iter().any(|a| a == n);
    let mut out = Vec::new();
    if reach.omni && named("omni") {
        out.push("TILES_ALLOW_OMNI".to_string());
    }
    for offer in reach.offers.iter().filter(|o| named(o.name)) {
        out.push(format!("TILES_ALLOW_{}", offer.name.to_ascii_uppercase()));
    }
    out
}

/// The plan for `tiles run <harness> <user args>`.
///
/// # Errors
///
/// When Claude Code's arguments carry a `--settings` with no value, or
/// `setup.settings` cannot take Tiles' rules ([`claude_settings`]), or the
/// `OpenCode` launcher's arguments name a `--server` ([`opencode_args`]).
pub fn plan(harness: Harness, setup: &Setup, user_args: &[String]) -> Result<Plan, String> {
    let (draw, wrap) = draw_for(harness, &setup.detected, setup.force_wrapper);
    let env = plan_env(setup, draw, wrap);
    let instruction = instructions::session(draw, setup.companion, &setup.reach);
    let tiles = path(&setup.tiles);
    let tools = pre_approved(&setup.reach, &setup.allow);
    let mut files = Vec::new();
    let mut launch = Launch {
        program: harness.name().to_string(),
        args: Vec::new(),
        env: env.clone(),
    };
    // Tells an installed add-on that `tiles run` already loaded Tiles.
    launch.env.push(("TILES_RUN".into(), "1".into()));
    match harness {
        Harness::Pi => {
            launch
                .args
                .extend(["-e".to_string(), path(&setup.addons.join("pi/index.ts"))]);
            launch.args.extend(user_args.iter().cloned());
            launch.env.push(("TILES_BIN".into(), tiles));
            launch
                .env
                .push(("TILES_ADDONS".into(), format!("{}/", path(&setup.addons))));
            // Pi has no tool approvals; the extension asks before a call
            // that fetches from Omni or a warehouse unless these say not to
            // (ADR-028, ADR-034).
            for name in pi_allowed(&setup.reach, &setup.allow) {
                launch.env.push((name, "1".into()));
            }
            if setup.detected.multiplexer.is_some() && draw == Draw::Image {
                // herdr passes kitty graphics through but hides the terminal
                // Pi would detect; only a forced placeholder route gets here.
                launch
                    .env
                    .push(("PI_IMAGE_PROTOCOL".into(), "kitty".into()));
            }
        }
        Harness::Claude => {
            let settings =
                claude_settings(&setup.cache, setup.home.as_deref(), setup.settings.as_ref())
                    .map_err(|e| {
                        let (_, spec) = take_settings(user_args).unwrap_or_default();
                        format!(
                            "cannot use --settings {}: {e}",
                            tiles_term::sanitize::line(spec.as_deref().unwrap_or_default(), 200)
                        )
                    })?;
            let (args, file) = claude_args(
                setup,
                &env,
                draw,
                instruction,
                user_args,
                settings.to_string(),
            )?;
            launch.args = args;
            files.push(file);
        }
        Harness::Codex => {
            launch.args = codex_args(setup, &env, &tiles, &instruction, tools.is_empty());
            launch.args.extend(user_args.iter().cloned());
        }
        Harness::OpenCode => {
            let mut config = crate::addons::opencode_config(
                &tiles,
                &env,
                &setup.reach,
                &setup.allow,
                &setup.addons.join("opencode/instructions.md"),
            );
            config["permission"]["external_directory"] = opencode_deny(&setup.cache);
            launch
                .env
                .push(("OPENCODE_CONFIG_CONTENT".into(), config.to_string()));
            launch.args = opencode_args(setup.opencode, user_args)?;
        }
    }
    Ok(Plan {
        launch,
        wrap,
        draw,
        files,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tiles_mcp::instructions::Draw;
    use tiles_term::detect::{Detected, Multiplexer, Route, Terminal};

    use tiles_mcp::source::{Offer, Reach, Tool};

    use super::{
        Harness, Plan, Setup, claude_settings, draw_for, opencode_deny, pre_approved,
        settings_value, take_settings, toml_string,
    };
    use crate::motion::Motion;
    use crate::wrap::paint::conversation_cols;

    fn plan(harness: Harness, setup: &Setup, user_args: &[String]) -> Plan {
        super::plan(harness, setup, user_args).unwrap()
    }

    #[test]
    fn opencodes_sidebar_takes_42_columns_above_120() {
        let panel = Harness::OpenCode.side_panel();
        assert_eq!(conversation_cols(panel, 182), 140);
        assert_eq!(conversation_cols(panel, 121), 79);
        assert_eq!(conversation_cols(panel, 120), 120, "no sidebar at 120");
        assert_eq!(conversation_cols(Harness::Claude.side_panel(), 182), 182);
    }

    fn detected(terminal: Terminal, multiplexer: Option<Multiplexer>, route: Route) -> Detected {
        Detected {
            terminal,
            multiplexer,
            route,
            reason: String::new(),
        }
    }

    fn setup(d: Detected) -> Setup {
        Setup {
            tiles: PathBuf::from("/opt/tiles"),
            addons: PathBuf::from("/c/addons"),
            cache: PathBuf::from("/c"),
            home: None,
            detected: d,
            reach: Reach::default(),
            allow: Vec::new(),
            settings: None,
            look_env: vec![("TILES_COLS".into(), "100".into())],
            force_wrapper: false,
            session: 0xabc,
            opencode: None,
            motion: Motion::On,
            companion: false,
        }
    }

    fn warp() -> Detected {
        detected(Terminal::Warp, None, Route::KittyPrecut)
    }

    #[test]
    fn who_draws_follows_adr_004() {
        let kitty = detected(Terminal::Kitty, None, Route::KittyPlaceholders);
        assert_eq!(
            draw_for(Harness::Claude, &kitty, false),
            (Draw::Image, false)
        );
        assert_eq!(
            draw_for(Harness::Claude, &warp(), false),
            (Draw::Block, true)
        );
        assert_eq!(draw_for(Harness::Pi, &kitty, false), (Draw::Image, false));
        let ghostty = detected(Terminal::Ghostty, None, Route::KittyPlaceholders);
        assert_eq!(draw_for(Harness::Pi, &ghostty, false), (Draw::Image, false));
        assert_eq!(draw_for(Harness::Pi, &kitty, true), (Draw::Block, true));
        assert_eq!(
            draw_for(Harness::Pi, &warp(), false),
            (Draw::Block, true),
            "on the pre-cut route the wrapper draws Pi's card in the reply"
        );
        let wezterm = detected(Terminal::Wezterm, None, Route::KittyPrecut);
        assert_eq!(draw_for(Harness::Pi, &wezterm, false), (Draw::Block, true));
        let foot = detected(Terminal::Foot, None, Route::Sixel);
        assert_eq!(draw_for(Harness::Pi, &foot, false), (Draw::Block, true));
        assert_eq!(draw_for(Harness::Codex, &kitty, false), (Draw::Block, true));
        assert_eq!(
            draw_for(Harness::OpenCode, &warp(), false),
            (Draw::Block, true)
        );
        let plain = detected(Terminal::AppleTerminal, None, Route::Text);
        for h in Harness::ALL {
            assert_eq!(draw_for(h, &plain, true), (Draw::Text, false));
        }
        let herdr_kitty = detected(
            Terminal::Kitty,
            Some(Multiplexer::Herdr),
            Route::KittyPrecut,
        );
        assert_eq!(
            draw_for(Harness::Claude, &herdr_kitty, false),
            (Draw::Block, true)
        );
    }

    #[test]
    fn claude_gets_its_own_launch_options() {
        let p = plan(Harness::Claude, &setup(warp()), &["fix the bug".into()]);
        let a = &p.launch.args;
        assert_eq!(p.launch.program, "claude");
        assert_eq!(a[0], "fix the bug", "the caller's arguments come first");
        assert_eq!(a[1], "--mcp-config");
        let config: serde_json::Value = serde_json::from_str(&a[2]).unwrap();
        assert_eq!(config["mcpServers"]["tiles"]["command"], "/opt/tiles");
        assert_eq!(config["mcpServers"]["tiles"]["args"][0], "mcp");
        assert_eq!(config["mcpServers"]["tiles"]["env"]["TILES_DRAW"], "block");
        assert_eq!(a[3], "--append-system-prompt");
        assert!(a[4].contains("inside a ```tiles code fence"));
        assert_eq!(
            a[5..],
            [
                "--allowedTools".to_string(),
                "mcp__tiles__tiles_chart,mcp__tiles__tiles_theme".into(),
                "--settings".into(),
                "/c/addons/claude-settings.json".into(),
            ]
        );
        assert_eq!(
            p.files,
            [(
                PathBuf::from("/c/addons/claude-settings.json"),
                claude_settings(&PathBuf::from("/c"), None, None)
                    .unwrap()
                    .to_string()
            )]
        );
        let kitty = plan(
            Harness::Claude,
            &setup(detected(Terminal::Kitty, None, Route::KittyPlaceholders)),
            &[],
        );
        assert_eq!(
            kitty.launch.args[kitty.launch.args.len() - 2..],
            ["--plugin-dir".to_string(), "/c/addons/claude".into()]
        );
        assert!(!kitty.wrap);
    }

    #[test]
    fn codex_gets_toml_overrides() {
        let p = plan(
            Harness::Codex,
            &setup(warp()),
            &["--model".into(), "o4".into()],
        );
        let a = &p.launch.args;
        assert_eq!(
            a[..4],
            [
                "-c".to_string(),
                "mcp_servers.tiles.command=\"/opt/tiles\"".into(),
                "-c".into(),
                "mcp_servers.tiles.args=[\"mcp\"]".into()
            ]
        );
        assert_eq!(
            a[5],
            "mcp_servers.tiles.env={TILES_DRAW=\"block\",TILES_COLS=\"100\",TILES_SESSION=\"0000000000000abc\",TILES_FRAMES=\"10\"}"
        );
        assert!(a[9].starts_with("developer_instructions=\"This terminal session runs with Tiles"));
        assert_eq!(a[a.len() - 2..], ["--model".to_string(), "o4".into()]);
        assert!(p.wrap);
    }

    #[test]
    fn opencode_gets_inline_config() {
        let mut s = setup(warp());
        s.reach.omni = true;
        let p = plan(Harness::OpenCode, &s, &[]);
        let (_, config) = p
            .launch
            .env
            .iter()
            .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(
            config["mcp"]["tiles"]["command"],
            serde_json::json!(["/opt/tiles", "mcp"])
        );
        assert_eq!(config["mcp"]["tiles"]["type"], "local");
        for tool in [
            "tiles_tiles_chart",
            "tiles_tiles_omni_topics",
            "tiles_tiles_omni_fields",
        ] {
            assert_eq!(config["permission"][tool], "ask", "{tool}");
        }
        s.allow.push("omni".into());
        let p = plan(Harness::OpenCode, &s, &[]);
        let (_, config) = p
            .launch
            .env
            .iter()
            .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(config["permission"]["tiles_tiles_omni_topics"], "allow");
        assert_eq!(config["permission"]["tiles_tiles_chart"], "allow");
        assert_eq!(
            config["instructions"][0],
            "/c/addons/opencode/instructions.md"
        );
        assert!(p.launch.env.contains(&("TILES_RUN".into(), "1".into())));
    }

    // A launcher build of OpenCode, 2026-10-19: its TUI attached to a
    // background service that had no tiles server or yesterday's, so the
    // model drew no card or one the session's wrapper would not draw.
    #[test]
    fn the_opencode_launcher_gets_a_private_server() {
        use crate::opencode::Cli;
        let mut s = setup(warp());
        let args = |s: &Setup, user: &[&str]| {
            super::plan(
                Harness::OpenCode,
                s,
                &user.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )
            .map(|p| p.launch.args)
        };
        assert_eq!(args(&s, &["-c"]).unwrap(), ["-c"], "not read: as before");
        s.opencode = Some(Cli::Public);
        assert_eq!(
            args(&s, &["--prompt", "hi"]).unwrap(),
            ["--prompt", "hi"],
            "sst/opencode starts its own server"
        );
        assert_eq!(
            args(&s, &["--server", "http://x"]).unwrap(),
            ["--server", "http://x"],
            "sst/opencode has no --server; its own error says so"
        );
        s.opencode = Some(Cli::Launcher);
        assert_eq!(args(&s, &[]).unwrap(), ["--standalone"]);
        assert_eq!(
            args(&s, &["-c", "/work"]).unwrap(),
            ["--standalone", "-c", "/work"]
        );
        assert_eq!(
            args(&s, &["--standalone", "-c"]).unwrap(),
            ["--standalone", "-c"],
            "never twice"
        );
        for server in [
            &["--server", "http://127.0.0.1:4096"][..],
            &["--server=http://x"],
        ] {
            let e = args(&s, server).unwrap_err();
            assert!(e.starts_with("OpenCode's --server attaches"), "{e}");
        }
        let p = plan(Harness::OpenCode, &s, &[]);
        assert!(
            p.launch
                .env
                .iter()
                .any(|(k, _)| k == "OPENCODE_CONFIG_CONTENT"),
            "the private server reads the session's config"
        );
    }

    #[test]
    fn the_server_hears_of_the_companion_window_only_from_tiles_run() {
        let var = |p: &super::Plan| {
            p.launch
                .env
                .iter()
                .find(|(k, _)| k == "TILES_COMPANION")
                .map(|(_, v)| v.clone())
        };
        let plain = detected(Terminal::AppleTerminal, None, Route::Text);
        let mut on = setup(plain.clone());
        on.companion = true;
        for h in Harness::ALL {
            let p = plan(h, &on, &[]);
            assert_eq!(var(&p).as_deref(), Some("1"), "{h:?}");
            assert_eq!(
                var(&plan(h, &setup(plain.clone()), &[])),
                None,
                "{h:?}: window off"
            );
        }
        let p = plan(Harness::Claude, &on, &[]);
        assert_eq!(p.launch.args[0], "--mcp-config");
        let config: serde_json::Value = serde_json::from_str(&p.launch.args[1]).unwrap();
        assert_eq!(
            config["mcpServers"]["tiles"]["env"]["TILES_COMPANION"], "1",
            "Claude Code's MCP server gets it in --mcp-config"
        );
        let mut warp = setup(warp());
        warp.companion = true;
        assert_eq!(
            var(&plan(Harness::Claude, &warp, &[])),
            None,
            "only the text route"
        );
    }

    #[test]
    fn the_session_instruction_names_the_companion_window_only_when_it_is_on() {
        let plain = detected(Terminal::AppleTerminal, None, Route::Text);
        for companion in [false, true] {
            let mut s = setup(plain.clone());
            s.companion = companion;
            let instruction = tiles_mcp::instructions::session(Draw::Text, companion, &s.reach);
            assert_eq!(
                instruction.contains("the Tiles window beside it shows each chart"),
                companion,
                "{instruction}"
            );
            let claude = plan(Harness::Claude, &s, &[]);
            assert_eq!(claude.launch.args[2], "--append-system-prompt");
            assert_eq!(claude.launch.args[3], instruction, "companion {companion}");
            let codex = plan(Harness::Codex, &s, &[]);
            assert!(
                codex.launch.args.contains(&format!(
                    "developer_instructions={}",
                    toml_string(&instruction)
                )),
                "companion {companion}: {:?}",
                codex.launch.args
            );
        }
    }

    #[test]
    fn the_server_draws_an_entrance_only_where_the_wrapper_plays_it() {
        let var = |p: &super::Plan, name: &str| {
            p.launch
                .env
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        let frames = |p: &super::Plan| var(p, "TILES_FRAMES");
        let kitty = detected(Terminal::Kitty, None, Route::KittyPlaceholders);
        let p = plan(Harness::OpenCode, &setup(kitty.clone()), &[]);
        assert_eq!(frames(&p).as_deref(), Some("10"), "450 ms, 50 ms apart");
        let (_, config) = p
            .launch
            .env
            .iter()
            .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(config["mcp"]["tiles"]["environment"]["TILES_FRAMES"], "10");
        assert_eq!(
            config["mcp"]["tiles"]["environment"]["TILES_SESSION"],
            "0000000000000abc"
        );
        assert_eq!(
            frames(&plan(Harness::Codex, &setup(kitty.clone()), &[])).as_deref(),
            Some("10")
        );
        let mut forced = setup(kitty.clone());
        forced.force_wrapper = true;
        assert_eq!(
            frames(&plan(Harness::Claude, &forced, &[])).as_deref(),
            Some("10")
        );
        // Claude Code's own mod draws in kitty, Pi draws itself, and
        // Ghostty and herdr's placeholder route get no entrance.
        assert_eq!(
            frames(&plan(Harness::Claude, &setup(kitty.clone()), &[])),
            None
        );
        assert_eq!(frames(&plan(Harness::Pi, &setup(kitty), &[])), None);
        let ghostty = detected(Terminal::Ghostty, None, Route::KittyPlaceholders);
        assert_eq!(frames(&plan(Harness::OpenCode, &setup(ghostty), &[])), None);
        let herdr = detected(
            Terminal::Kitty,
            Some(Multiplexer::Herdr),
            Route::KittyPlaceholders,
        );
        assert_eq!(frames(&plan(Harness::OpenCode, &setup(herdr), &[])), None);
        // The pre-cut route plays it too, in every harness the wrapper runs.
        for h in [
            Harness::Claude,
            Harness::Pi,
            Harness::OpenCode,
            Harness::Codex,
        ] {
            let p = plan(h, &setup(warp()), &[]);
            assert!(
                p.launch
                    .env
                    .contains(&("TILES_SESSION".into(), "0000000000000abc".into())),
                "{h:?}: the wrapper draws only its own session's cards on every route"
            );
            assert_eq!(frames(&p).as_deref(), Some("10"), "{h:?} in Warp");
            assert_eq!(var(&p, "TILES_MOTION"), None);
        }
        // On the text route no wrapper runs, but the companion window shows
        // only the session's own cards, so every harness passes the token
        // and draws no entrance.
        let plain = detected(Terminal::AppleTerminal, None, Route::Text);
        for h in Harness::ALL {
            let p = plan(h, &setup(plain.clone()), &[]);
            assert!(!p.wrap);
            assert_eq!(
                var(&p, "TILES_SESSION").as_deref(),
                Some("0000000000000abc"),
                "{h:?}"
            );
            assert_eq!(frames(&p), None, "{h:?}");
        }
        // Reduce motion draws 4 frames of a fade; off draws none.
        let mut reduce = setup(warp());
        reduce.motion = Motion::Reduce;
        let p = plan(Harness::Claude, &reduce, &[]);
        assert_eq!(frames(&p).as_deref(), Some("4"));
        assert_eq!(var(&p, "TILES_MOTION").as_deref(), Some("reduce"));
        let mut off = setup(warp());
        off.motion = Motion::Off;
        let p = plan(Harness::Claude, &off, &[]);
        assert_eq!((frames(&p), var(&p, "TILES_MOTION")), (None, None));
    }

    #[test]
    fn pi_loads_the_extension_and_draws_itself_on_the_placeholder_route() {
        let kitty = detected(Terminal::Kitty, None, Route::KittyPlaceholders);
        let p = plan(Harness::Pi, &setup(kitty), &["-c".into()]);
        assert_eq!(p.launch.args, ["-e", "/c/addons/pi/index.ts", "-c"]);
        assert!(
            p.launch
                .env
                .contains(&("TILES_BIN".into(), "/opt/tiles".into()))
        );
        assert!(!p.wrap);
        let warp = plan(Harness::Pi, &setup(warp()), &[]);
        assert_eq!(warp.launch.args, ["-e", "/c/addons/pi/index.ts"]);
        assert!(warp.wrap);
        assert_eq!(warp.draw, Draw::Block);
        assert!(
            warp.launch
                .env
                .contains(&("TILES_DRAW".into(), "block".into()))
        );
        let herdr = plan(
            Harness::Pi,
            &setup(detected(
                Terminal::Unknown,
                Some(Multiplexer::Herdr),
                Route::KittyPlaceholders,
            )),
            &[],
        );
        assert!(
            herdr
                .launch
                .env
                .contains(&("PI_IMAGE_PROTOCOL".into(), "kitty".into()))
        );
    }

    #[test]
    fn omni_tools_and_omni_charts_are_pre_approved_only_on_request() {
        let none = Vec::<String>::new();
        let omni = vec!["omni".to_string()];
        assert_eq!(
            pre_approved(&Reach::omni(false), &none),
            ["tiles_chart", "tiles_theme"]
        );
        assert_eq!(pre_approved(&Reach::omni(true), &none), Vec::<&str>::new());
        assert_eq!(
            pre_approved(&Reach::omni(true), &omni),
            [
                "tiles_chart",
                "tiles_theme",
                "tiles_omni_topics",
                "tiles_omni_fields"
            ]
        );
        let mut s = setup(warp());
        s.reach.omni = true;
        let allowed = |p: &super::Plan| p.launch.env.iter().any(|(k, _)| k == "TILES_ALLOW_OMNI");
        assert!(!allowed(&plan(Harness::Pi, &s, &[])), "Pi's extension asks");
        let p = plan(Harness::Claude, &s, &[]);
        assert!(
            !p.launch.args.iter().any(|a| a == "--allowedTools"),
            "{:?}",
            p.launch.args
        );
        let codex = plan(Harness::Codex, &s, &[]);
        assert!(
            codex
                .launch
                .args
                .contains(&"mcp_servers.tiles.default_tools_approval_mode=\"prompt\"".to_string())
        );
        s.allow.push("omni".into());
        assert!(allowed(&plan(Harness::Pi, &s, &[])));
        let codex = plan(Harness::Codex, &s, &[]);
        assert!(
            !codex
                .launch
                .args
                .iter()
                .any(|a| a.contains("default_tools_approval_mode"))
        );
        let p = plan(Harness::Claude, &s, &[]);
        let at = p
            .launch
            .args
            .iter()
            .position(|a| a == "--allowedTools")
            .unwrap();
        assert_eq!(
            p.launch.args[at + 1],
            "mcp__tiles__tiles_chart,mcp__tiles__tiles_theme,mcp__tiles__tiles_omni_topics,mcp__tiles__tiles_omni_fields"
        );
    }

    fn warehouse() -> Offer {
        let tool = |name| Tool {
            name,
            title: "t",
            description: "d",
            input: serde_json::json!({}),
        };
        Offer {
            name: "warehouse",
            origin: "BigQuery",
            tools: vec![
                tool("tiles_warehouse_models"),
                tool("tiles_warehouse_columns"),
            ],
            instruction: "Pass data.warehouse.".into(),
            chart_line: "For warehouse data, pass data.warehouse.".into(),
            tool_timeout: std::time::Duration::from_secs(300),
        }
    }

    #[test]
    fn warehouse_tools_and_charts_are_pre_approved_only_on_request() {
        let reach = Reach {
            omni: false,
            offers: vec![warehouse()],
        };
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(pre_approved(&reach, &[]), Vec::<&str>::new());
        assert_eq!(
            pre_approved(&reach, &words(&["omni"])),
            Vec::<&str>::new(),
            "--allow-omni does not cover the warehouse"
        );
        assert_eq!(
            pre_approved(&reach, &words(&["warehouse"])),
            [
                "tiles_chart",
                "tiles_theme",
                "tiles_warehouse_models",
                "tiles_warehouse_columns"
            ]
        );
        let both = Reach {
            omni: true,
            ..reach.clone()
        };
        assert_eq!(
            pre_approved(&both, &words(&["warehouse"])),
            Vec::<&str>::new()
        );
        assert_eq!(pre_approved(&both, &words(&["omni", "warehouse"])).len(), 6);

        let mut s = setup(warp());
        s.reach = reach;
        let pi_allowed = |p: &super::Plan| {
            p.launch
                .env
                .iter()
                .any(|(k, v)| k == "TILES_ALLOW_WAREHOUSE" && v == "1")
        };
        assert!(
            !pi_allowed(&plan(Harness::Pi, &s, &[])),
            "Pi's extension asks"
        );
        let claude = plan(Harness::Claude, &s, &[]);
        assert!(!claude.launch.args.iter().any(|a| a == "--allowedTools"));
        let instruction = claude
            .launch
            .args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .map(|i| claude.launch.args[i + 1].clone())
            .unwrap();
        assert!(
            instruction.contains("Pass data.warehouse."),
            "{instruction}"
        );
        let codex = plan(Harness::Codex, &s, &[]);
        let args = &codex.launch.args;
        assert!(
            args.contains(&"mcp_servers.tiles.default_tools_approval_mode=\"prompt\"".to_string())
        );
        assert!(
            args.contains(&"mcp_servers.tiles.tool_timeout_sec=330".to_string()),
            "Codex waits out the warehouse's time budget: {args:?}"
        );
        assert!(
            plan(Harness::Codex, &setup(warp()), &[])
                .launch
                .args
                .contains(&"mcp_servers.tiles.tool_timeout_sec=120".to_string())
        );
        let opencode = plan(Harness::OpenCode, &s, &[]);
        let (_, config) = opencode
            .launch
            .env
            .iter()
            .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        for tool in [
            "tiles_tiles_chart",
            "tiles_tiles_theme",
            "tiles_tiles_warehouse_models",
            "tiles_tiles_warehouse_columns",
        ] {
            assert_eq!(config["permission"][tool], "ask", "{tool}");
        }
        s.allow.push("warehouse".into());
        assert!(pi_allowed(&plan(Harness::Pi, &s, &[])));
        let claude = plan(Harness::Claude, &s, &[]);
        let at = claude
            .launch
            .args
            .iter()
            .position(|a| a == "--allowedTools")
            .unwrap();
        assert_eq!(
            claude.launch.args[at + 1],
            "mcp__tiles__tiles_chart,mcp__tiles__tiles_theme,mcp__tiles__tiles_warehouse_models,mcp__tiles__tiles_warehouse_columns"
        );
    }

    // The security review's S1: a model with a file reader or a shell read
    // the card pictures beside their calls in the cache.
    #[test]
    fn claude_code_and_opencode_keep_the_model_out_of_the_cache() {
        let cache = PathBuf::from("/Users/u/Library/Caches/tiles");
        let settings = claude_settings(&cache, Some(&PathBuf::from("/Users/u/")), None).unwrap();
        assert_eq!(
            settings,
            serde_json::json!({ "permissions": { "deny": [
                "Read(//Users/u/Library/Caches/tiles/**)",
                "Bash(*/Users/u/Library/Caches/tiles*)",
                "Bash(*~/Library/Caches/tiles*)",
                "Bash(*$HOME/Library/Caches/tiles*)",
            ] } }),
            "the Read and Bash denies, and nothing the user did not choose"
        );
        let elsewhere = claude_settings(
            &PathBuf::from("/x/tiles/"),
            Some(&PathBuf::from("/Users/u")),
            None,
        )
        .unwrap();
        assert_eq!(
            elsewhere["permissions"]["deny"],
            serde_json::json!(["Read(//x/tiles/**)", "Bash(*/x/tiles*)"])
        );
        assert_eq!(
            opencode_deny(&cache),
            serde_json::json!({ "/Users/u/Library/Caches/tiles/*": "deny" })
        );
        for omni in [false, true] {
            let mut s = setup(warp());
            s.reach.omni = omni;
            if omni {
                s.allow.push("omni".into());
            }
            let p = plan(Harness::OpenCode, &s, &[]);
            let (_, config) = p
                .launch
                .env
                .iter()
                .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
                .unwrap();
            let config: serde_json::Value = serde_json::from_str(config).unwrap();
            assert_eq!(
                config["permission"]["external_directory"],
                serde_json::json!({ "/c/*": "deny" }),
                "--allow-omni never lifts it"
            );
            let claude = plan(Harness::Claude, &s, &[]);
            assert!(claude.launch.args.iter().any(|a| a == "--settings"));
        }
        for h in [Harness::Pi, Harness::Codex, Harness::OpenCode] {
            assert!(plan(h, &setup(warp()), &[]).files.is_empty(), "{h:?}");
        }
    }

    // Claude Code takes one --settings: the user's own is merged into the
    // file Tiles writes and passed once.
    #[test]
    fn a_users_own_settings_are_merged_and_passed_once() {
        let args: Vec<String> = ["-p", "hi", "--settings", "~/mine.json", "--model", "opus"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            take_settings(&args).unwrap(),
            (
                ["-p", "hi", "--model", "opus"].map(String::from).to_vec(),
                Some("~/mine.json".to_string())
            )
        );
        let joined: Vec<String> = ["--settings={}", "-c"].map(String::from).to_vec();
        assert_eq!(
            take_settings(&joined).unwrap(),
            (vec!["-c".to_string()], Some("{}".to_string()))
        );
        assert_eq!(take_settings(&[]).unwrap(), (Vec::new(), None));
        assert!(
            take_settings(&["--settings".to_string()])
                .unwrap_err()
                .contains("needs a file or JSON")
        );

        let mine = r#"{"permissions":{"deny":["Read(//c/**)","Read(//secret/**)"],"allow":["Bash(ls *)"]},"model":"opus"}"#;
        let inline = settings_value(mine, &|_| panic!("inline JSON reads no file")).unwrap();
        let read = |p: &std::path::Path| {
            if p == std::path::Path::new("/u/mine.json") {
                Ok(mine.to_string())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no such file",
                ))
            }
        };
        assert_eq!(settings_value("/u/mine.json", &read).unwrap(), inline);
        let missing = settings_value("/u/none.json", &read).unwrap_err();
        assert!(
            missing.starts_with("cannot read --settings /u/none.json: ") && !missing.contains('\n'),
            "{missing}"
        );
        let broken = settings_value("{oops", &read).unwrap_err();
        assert!(
            broken.starts_with("cannot parse --settings {oops: "),
            "{broken}"
        );

        let merged = claude_settings(&PathBuf::from("/c"), None, Some(&inline)).unwrap();
        assert_eq!(
            merged,
            serde_json::json!({
                "permissions": {
                    "deny": ["Read(//c/**)", "Read(//secret/**)", "Bash(*/c*)"],
                    "allow": ["Bash(ls *)"],
                },
                "model": "opus",
            }),
            "the user's rules first, Tiles' after, none twice"
        );
        let bare =
            claude_settings(&PathBuf::from("/c"), None, Some(&serde_json::json!({}))).unwrap();
        assert_eq!(
            bare,
            claude_settings(&PathBuf::from("/c"), None, None).unwrap()
        );
        for (wrong, said) in [
            (serde_json::json!([]), "not a JSON object"),
            (
                serde_json::json!({ "permissions": 1 }),
                "permissions is not an object",
            ),
            (
                serde_json::json!({ "permissions": { "deny": "x" } }),
                "permissions.deny is not a list",
            ),
        ] {
            let e = claude_settings(&PathBuf::from("/c"), None, Some(&wrong)).unwrap_err();
            assert!(e.contains(said), "{e}");
        }

        let mut s = setup(warp());
        s.settings = Some(inline);
        let p = plan(Harness::Claude, &s, &args);
        let a = &p.launch.args;
        assert_eq!(a[..4], ["-p", "hi", "--model", "opus"]);
        assert_eq!(a.iter().filter(|x| *x == "--settings").count(), 1, "{a:?}");
        let at = a.iter().position(|x| x == "--settings").unwrap();
        assert_eq!(a[at + 1], "/c/addons/claude-settings.json");
        assert!(!a.iter().any(|x| x == "~/mine.json"));
        let (file, text) = &p.files[0];
        assert_eq!(file, &PathBuf::from("/c/addons/claude-settings.json"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(text).unwrap(),
            merged
        );

        s.settings = Some(serde_json::json!({ "permissions": { "deny": "x" } }));
        let e = super::plan(Harness::Claude, &s, &args).unwrap_err();
        assert_eq!(
            e,
            "cannot use --settings ~/mine.json: permissions.deny is not a list"
        );
        let e =
            super::plan(Harness::Claude, &setup(warp()), &["--settings".to_string()]).unwrap_err();
        assert!(e.contains("needs a file or JSON"), "{e}");
    }

    #[test]
    fn toml_strings_escape_quotes_and_controls() {
        assert_eq!(
            toml_string("a \"b\" \\ c\n\u{1b}"),
            "\"a \\\"b\\\" \\\\ c\\n\\u001B\""
        );
    }
}
