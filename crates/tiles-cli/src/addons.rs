//! The harness add-ons, embedded in the binary and written out where a launch
//! or an install needs them, each beside a `tool.json` this binary generates
//! (the tool's schema, description and the session instruction), so an add-on
//! never drifts from the binary that serves it.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use serde_json::json;
use tiles_intent::schema::FormEntry;
use tiles_mcp::instructions::{self, Draw};
use tiles_mcp::source::Reach;

/// The add-on sources, by path under `adapters/`.
pub const SOURCES: [(&str, &str); 7] = [
    ("pi/index.ts", include_str!("../../../adapters/pi/index.ts")),
    (
        "claude/.claude-plugin/plugin.json",
        include_str!("../../../adapters/claude/.claude-plugin/plugin.json"),
    ),
    (
        "claude/hooks/hooks.json",
        include_str!("../../../adapters/claude/hooks/hooks.json"),
    ),
    (
        "claude/hooks/register.tsx",
        include_str!("../../../adapters/claude/hooks/register.tsx"),
    ),
    (
        "claude/hooks/card.ts",
        include_str!("../../../adapters/claude/hooks/card.ts"),
    ),
    (
        "claude/commands/theme.md",
        include_str!("../../../adapters/claude/commands/theme.md"),
    ),
    (
        "codex/skills/tiles-theme/SKILL.md",
        include_str!("../../../adapters/codex/skills/tiles-theme/SKILL.md"),
    ),
];

/// The Codex skill that stands in for `/theme`: codex-cli 0.163.0 has its
/// own `/theme` (syntax highlighting) and no custom prompts, and offers a
/// skill as `$tiles-theme` and in `/skills` (ADR-021).
pub const CODEX_THEME_SKILL: &str = SOURCES[6].1;

/// What the generated files say.
#[derive(Debug, Clone)]
pub struct Spec {
    /// The `tiles` binary the add-ons run.
    pub tiles: PathBuf,
    /// Who draws.
    pub draw: Draw,
    /// Whether `tiles run`'s companion window shows the cards
    /// ([`crate::companion::wanted`]).
    pub companion: bool,
    /// What the session can fetch from: Omni and each configured source.
    pub reach: Reach,
    /// Variables the add-ons pass to `tiles`.
    pub env: Vec<(String, String)>,
    /// The engine's forms.
    pub forms: Vec<FormEntry>,
}

/// Writes `bytes` to `path` owner-only through a fresh temporary file, so a
/// planted symlink is replaced, never followed.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let show = || tiles_term::sanitize::line(&path.display().to_string(), 200);
    if let Some(dir) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| format!("cannot create {}: {e}", show()))?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(bytes))
        .and_then(|()| fs::rename(&tmp, path));
    written.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot write {}: {e}", show())
    })
}

fn env_map(env: &[(String, String)]) -> serde_json::Map<String, serde_json::Value> {
    env.iter().map(|(k, v)| (k.clone(), json!(v))).collect()
}

/// The Pi extension's `tool.json`: `tiles_chart`, and each configured
/// source's tools, which the extension asks before it runs.
#[must_use]
pub fn pi_tool(spec: &Spec) -> serde_json::Value {
    let tool = tiles_mcp::server::chart_tool(&spec.forms, &spec.reach);
    let tools: Vec<serde_json::Value> = spec
        .reach
        .offers
        .iter()
        .flat_map(|o| &o.tools)
        .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.input }))
        .collect();
    json!({
        "name": tool["name"],
        "description": tool["description"],
        "inputSchema": tool["inputSchema"],
        "instructions": instructions::session(spec.draw, spec.companion, &spec.reach),
        "tiles": spec.tiles.display().to_string(),
        "env": env_map(&spec.env),
        "tools": tools,
    })
}

/// The `OpenCode` permission key for the Tiles server's tool `tool`.
#[must_use]
pub fn opencode_permission(tool: &str) -> String {
    format!("tiles_{tool}")
}

/// The `OpenCode` `/theme` command: `OpenCode` 1.19.3 has `/themes` for its
/// own colours and no `/theme`, and takes commands under the config's
/// `command` key with `$ARGUMENTS` for what the user typed after it.
#[must_use]
pub fn opencode_theme_command() -> serde_json::Value {
    json!({
        "description": format!(
            "List Tiles' card themes, or switch to one ({})",
            tiles_engine::Theme::all()
                .iter()
                .map(|t| t.name())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "template": format!(
            "Call the {} tool from the tiles MCP server. If \"$ARGUMENTS\" names a theme, pass it \
             as name; if it is empty, call the tool with no arguments, show me the themes it lists, \
             and ask which one I want, then call it again with my pick. Tell me what its result says.",
            tiles_mcp::THEME_TOOL
        ),
    })
}

/// The `OpenCode` config layer: the MCP server, its tools' permissions, the
/// `/theme` command and the instruction file. A tool is `allow` where
/// [`crate::harness::pre_approved`] says so and `ask` otherwise, since
/// `OpenCode` runs an MCP tool without asking unless told to. `OpenCode` 1.19.3 merges config files and
/// `OPENCODE_CONFIG_CONTENT` over the user's own config key by key
/// (`config/config.ts`, `mergeDeep`), so their other servers stay. Its
/// plugins can neither draw a result nor add an MCP server there, so this
/// layer is the whole `OpenCode` add-on.
#[must_use]
pub fn opencode_config(
    tiles: &str,
    env: &[(String, String)],
    reach: &Reach,
    allow: &[String],
    instructions_file: &Path,
) -> serde_json::Value {
    let allowed = crate::harness::pre_approved(reach, allow);
    let mut tools = vec![tiles_mcp::TOOL_NAME, tiles_mcp::THEME_TOOL];
    tools.extend(reach.data_tools());
    let permission = |t: &&str| json!(if allowed.contains(t) { "allow" } else { "ask" });
    json!({
        "mcp": { "tiles": {
            "type": "local",
            "command": [tiles, "mcp"],
            "environment": env_map(env),
            "enabled": true,
        } },
        "permission": tools.iter().map(|t| (opencode_permission(t), permission(t))).collect::<serde_json::Map<_, _>>(),
        "command": { "theme": opencode_theme_command() },
        "instructions": [instructions_file.display().to_string()],
    })
}

/// Writes every add-on under `dir`: `pi/`, `claude/`, and the instruction as
/// `opencode/instructions.md` for `OpenCode`'s `instructions` key.
pub fn write_all(dir: &Path, spec: &Spec) -> Result<(), String> {
    for (path, source) in SOURCES {
        write_file(&dir.join(path), source.as_bytes())?;
    }
    write_file(
        &dir.join("pi/tool.json"),
        pi_tool(spec).to_string().as_bytes(),
    )?;
    write_file(
        &dir.join("opencode/instructions.md"),
        instructions::session(spec.draw, spec.companion, &spec.reach).as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    use tiles_intent::schema::{FormEntry, RoleEntry};
    use tiles_mcp::instructions::Draw;

    use super::{Spec, pi_tool, write_all, write_file};

    fn spec() -> Spec {
        Spec {
            tiles: PathBuf::from("/opt/tiles"),
            draw: Draw::Image,
            companion: false,
            reach: tiles_mcp::source::Reach::default(),
            env: vec![("TILES_DRAW".into(), "image".into())],
            forms: vec![FormEntry {
                name: "bar".into(),
                summary: "Rank".into(),
                roles: vec![RoleEntry {
                    name: "x".into(),
                    required: true,
                    doc: "d".into(),
                }],
            }],
        }
    }

    #[test]
    fn the_pi_tool_carries_the_generated_schema_and_instruction() {
        let tool = pi_tool(&spec());
        assert_eq!(tool["name"], "tiles_chart");
        assert_eq!(
            tool["inputSchema"]["properties"]["form"]["enum"],
            serde_json::json!(["bar"])
        );
        assert!(
            tool["instructions"]
                .as_str()
                .unwrap()
                .contains("tiles_chart")
        );
        assert_eq!(tool["tiles"], "/opt/tiles");
        assert_eq!(tool["env"]["TILES_DRAW"], "image");
    }

    #[test]
    fn the_add_on_instructions_name_the_companion_window_only_when_it_is_on() {
        let dir = std::env::temp_dir().join(format!("tiles-addons-window-{}", std::process::id()));
        for companion in [false, true] {
            let spec = Spec {
                draw: Draw::Text,
                companion,
                ..spec()
            };
            let _ = std::fs::remove_dir_all(&dir);
            write_all(&dir, &spec).unwrap();
            let pi: serde_json::Value =
                serde_json::from_slice(&std::fs::read(dir.join("pi/tool.json")).unwrap()).unwrap();
            let opencode = std::fs::read_to_string(dir.join("opencode/instructions.md")).unwrap();
            for text in [pi["instructions"].as_str().unwrap(), opencode.as_str()] {
                assert_eq!(
                    text.contains("the Tiles window beside it shows each chart"),
                    companion,
                    "{text}"
                );
                assert_eq!(
                    text.contains(
                        "give the PNG path from the result, so they can open or share it"
                    ),
                    !companion,
                    "{text}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_add_on_is_written_owner_only() {
        let dir = std::env::temp_dir().join(format!("tiles-addons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_all(&dir, &spec()).unwrap();
        for f in [
            "pi/index.ts",
            "pi/tool.json",
            "opencode/instructions.md",
            "claude/hooks/register.tsx",
            "claude/.claude-plugin/plugin.json",
        ] {
            let meta = std::fs::metadata(dir.join(f)).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o600, "{f}");
        }
        assert_eq!(
            std::fs::metadata(dir.join("pi"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_planted_symlink_is_replaced_not_followed() {
        let dir = std::env::temp_dir().join(format!("tiles-addons-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let victim = dir.join("victim");
        std::fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(&victim, dir.join("tool.json")).unwrap();
        write_file(&dir.join("tool.json"), b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert_eq!(
            std::fs::read_to_string(dir.join("tool.json")).unwrap(),
            "new"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
