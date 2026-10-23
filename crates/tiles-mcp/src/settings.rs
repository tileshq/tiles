//! Tiles' local settings file (ADR-021): the theme the user picked with
//! `tiles theme <name>` or the `tiles_theme` tool, and a data source's own
//! settings under its key (`warehouse`, ADR-033), kept in one small JSON
//! file on this machine. Nothing in it leaves the machine.
//!
//! A card's theme is, first to last: the card's own `--theme` (`tiles
//! render`), `TILES_THEME` for a session, this file, then the design
//! tokens' default theme.

use std::fs;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use tiles_engine::Theme;

/// The environment variable that moves the settings directory.
pub const ENV_CONFIG_DIR: &str = "TILES_CONFIG_DIR";
/// Largest settings file read.
const MAX_BYTES: u64 = 64 * 1024;

/// Where the theme in force came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `TILES_THEME`, for this session only.
    Env,
    /// The settings file.
    Saved,
    /// Nothing chose one.
    Default,
}

/// The settings file: `$TILES_CONFIG_DIR/config.json`, else
/// `$XDG_CONFIG_HOME/tiles/config.json` or `~/.config/tiles/config.json`.
#[must_use]
pub fn path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = set(ENV_CONFIG_DIR).or_else(|| {
        set("XDG_CONFIG_HOME")
            .or_else(|| set("HOME").map(|h| h.join(".config")))
            .map(|c| c.join("tiles"))
    })?;
    Some(dir.join("config.json"))
}

fn read(path: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    match serde_json::from_slice(&bytes).ok()? {
        serde_json::Value::Object(o) => Some(o),
        _ => None,
    }
}

/// The theme the settings file names; `None` when it names none or cannot
/// be read.
#[must_use]
pub fn saved_theme(env: &dyn Fn(&str) -> Option<String>) -> Option<Theme> {
    let settings = read(&path(env)?)?;
    Theme::from_name(settings.get("theme")?.as_str()?)
}

/// The theme in force for a card that names none, and where it came from.
#[must_use]
pub fn theme(env: &dyn Fn(&str) -> Option<String>) -> (Theme, Source) {
    if let Some(t) = env("TILES_THEME").as_deref().and_then(Theme::from_name) {
        return (t, Source::Env);
    }
    saved_theme(env).map_or((Theme::default(), Source::Default), |t| (t, Source::Saved))
}

/// The whole settings file as JSON; `Null` when there is none or it cannot
/// be read, so a source reads its own key from it.
#[must_use]
pub fn all(env: &dyn Fn(&str) -> Option<String>) -> serde_json::Value {
    path(env)
        .and_then(|p| read(&p))
        .map_or(serde_json::Value::Null, serde_json::Value::Object)
}

/// Saves `theme` in the settings file, keeping its other keys, owner-only
/// and through a fresh temporary file so a planted symlink is replaced,
/// never followed.
///
/// # Errors
///
/// When there is no home to keep the file under, or it cannot be written.
pub fn save_theme(env: &dyn Fn(&str) -> Option<String>, theme: Theme) -> Result<PathBuf, String> {
    save(env, "theme", theme.name().into())
}

/// Saves `value` under `key` in the settings file, as [`save_theme`] does,
/// keeping the other keys; `Null` removes the key.
///
/// # Errors
///
/// When there is no home to keep the file under, or it cannot be written.
pub fn save(
    env: &dyn Fn(&str) -> Option<String>,
    key: &str,
    value: serde_json::Value,
) -> Result<PathBuf, String> {
    let path =
        path(env).ok_or_else(|| format!("no HOME to keep settings under; set {ENV_CONFIG_DIR}"))?;
    let show = || tiles_term::sanitize::line(&path.display().to_string(), 200);
    let mut settings = read(&path).unwrap_or_default();
    if value.is_null() {
        settings.remove(key);
    } else {
        settings.insert(key.into(), value);
    }
    if let Some(dir) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| format!("cannot create {}: {e}", show()))?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let mut text = serde_json::Value::Object(settings).to_string();
    text.push('\n');
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .and_then(|()| fs::rename(&tmp, &path));
    written.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot save {}: {e}", show())
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use tiles_engine::Theme;

    use super::{Source, all, path, save, save_theme, saved_theme, theme};

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tiles-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_file_follows_the_override_then_xdg_then_home() {
        let env = |k: &str| (k == "HOME").then(|| "/home/u".to_string());
        assert_eq!(
            path(&env).unwrap(),
            std::path::PathBuf::from("/home/u/.config/tiles/config.json")
        );
        let xdg = |k: &str| match k {
            "HOME" => Some("/home/u".to_string()),
            "XDG_CONFIG_HOME" => Some("/x".to_string()),
            _ => None,
        };
        assert_eq!(
            path(&xdg).unwrap(),
            std::path::PathBuf::from("/x/tiles/config.json")
        );
        let over = |k: &str| (k == "TILES_CONFIG_DIR").then(|| "/o".to_string());
        assert_eq!(
            path(&over).unwrap(),
            std::path::PathBuf::from("/o/config.json")
        );
        assert!(path(&|_| None).is_none());
    }

    #[test]
    fn the_default_until_a_theme_is_saved_and_the_session_variable_wins() {
        let dir = temp("order");
        let d = dir.display().to_string();
        let env = |k: &str| (k == "TILES_CONFIG_DIR").then(|| d.clone());
        assert_eq!(theme(&env), (Theme::default(), Source::Default));
        let saved = save_theme(&env, Theme::from_name("light").unwrap()).unwrap();
        assert_eq!(
            theme(&env),
            (Theme::from_name("light").unwrap(), Source::Saved)
        );
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&saved), 0o600);
        assert_eq!(mode(&dir), 0o700);
        let session = |k: &str| match k {
            "TILES_CONFIG_DIR" => Some(d.clone()),
            "TILES_THEME" => Some("dark".to_string()),
            _ => None,
        };
        assert_eq!(
            theme(&session),
            (Theme::from_name("dark").unwrap(), Source::Env)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_keys_survive_and_a_bad_file_is_ignored() {
        let dir = temp("keys");
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.display().to_string();
        let env = |k: &str| (k == "TILES_CONFIG_DIR").then(|| d.clone());
        std::fs::write(dir.join("config.json"), r#"{"theme": "neon", "other": 1}"#).unwrap();
        assert_eq!(saved_theme(&env), None, "an unknown theme is ignored");
        save_theme(&env, Theme::from_name("dark").unwrap()).unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
        assert_eq!(v, serde_json::json!({"theme": "dark", "other": 1}));
        save(
            &env,
            "warehouse",
            serde_json::json!({ "billing_project": "acme-dev" }),
        )
        .unwrap();
        assert_eq!(all(&env)["theme"], "dark");
        assert_eq!(all(&env)["warehouse"]["billing_project"], "acme-dev");
        save(&env, "warehouse", serde_json::Value::Null).unwrap();
        assert!(all(&env).get("warehouse").is_none());
        std::fs::write(dir.join("config.json"), b"not json").unwrap();
        assert_eq!(theme(&env).1, Source::Default);
        assert_eq!(all(&env), serde_json::Value::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_planted_symlink_is_replaced_not_followed() {
        let dir = temp("link");
        std::fs::create_dir_all(&dir).unwrap();
        let victim = dir.join("victim");
        std::fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(&victim, dir.join("config.json")).unwrap();
        let d = dir.display().to_string();
        let env = |k: &str| (k == "TILES_CONFIG_DIR").then(|| d.clone());
        save_theme(&env, Theme::from_name("light").unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert_eq!(saved_theme(&env), Some(Theme::from_name("light").unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
