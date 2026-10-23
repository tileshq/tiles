//! The per-user card cache (ADR-005, ADR-009): owner-only, refused when another
//! user owns it, and written through a temporary file renamed into place so a
//! planted symlink is never followed and a reader never sees half a PNG. Rows
//! fetched from Omni never reach it: the process that fetched them holds them
//! in memory for its own cards (ADR-027).

use std::collections::VecDeque;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{
    DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tiles_intent::source::Table;

/// The environment variable that moves the cache.
pub const ENV_CACHE_DIR: &str = "TILES_CACHE_DIR";
/// Most cards kept; older ones are removed as new ones are written.
pub const KEEP_CARDS: usize = 300;

/// The cache directory Tiles uses by default: `$TILES_CACHE_DIR`, else
/// `~/Library/Caches/tiles` on macOS, else `$XDG_CACHE_HOME/tiles` or
/// `~/.cache/tiles`.
#[must_use]
pub fn default_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = set(ENV_CACHE_DIR) {
        return Some(dir);
    }
    let home = set("HOME")?;
    if cfg!(target_os = "macos") {
        return Some(home.join("Library/Caches/tiles"));
    }
    Some(set("XDG_CACHE_HOME").map_or_else(|| home.join(".cache/tiles"), |x| x.join("tiles")))
}

/// Largest geometry or call file read back: the intent's payload limit
/// with room to spare, since a call file written before ADR-027 also held
/// the rows fetched for it.
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

fn read_json(path: &Path) -> Option<serde_json::Value> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_JSON_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// What a file in the cards directory is to [`Cache::clear`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CardFile {
    /// `<id>.png`.
    Card,
    /// `<id>.json`, `<id>.call.json`, `<id>-anim-NN.png`, or a write's
    /// leftover `.<id>.<pid>-<n>.tmp`.
    Beside,
}

fn card_file(name: &str) -> Option<CardFile> {
    if let Some(tmp) = name.strip_prefix('.') {
        let (id, rest) = tmp.split_at_checked(6)?;
        let write = rest
            .strip_suffix(".tmp")
            .is_some_and(|r| r.starts_with('.'));
        return (tiles_term::block::is_card_id(id) && write).then_some(CardFile::Beside);
    }
    let (id, rest) = name.split_at_checked(6)?;
    if !tiles_term::block::is_card_id(id) {
        return None;
    }
    let frame = rest
        .strip_prefix("-anim-")
        .and_then(|r| r.strip_suffix(".png"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    match rest {
        ".png" => Some(CardFile::Card),
        ".json" | ".call.json" => Some(CardFile::Beside),
        _ => frame.then_some(CardFile::Beside),
    }
}

/// The rows fetched from Omni for one card, in call order; `None` for a card
/// whose rows the model sent.
pub type Rows = Vec<Option<Table>>;

/// An opened, checked cache directory, and the rows this process fetched
/// from Omni for the cards it drew, which stay in memory (ADR-027).
#[derive(Clone)]
pub struct Cache {
    cards: PathBuf,
    held: Arc<Mutex<VecDeque<(String, Rows)>>>,
}

// Written by hand so a debug print never shows a held row.
impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = self.held.lock().map_or(0, |h| h.len());
        f.debug_struct("Cache")
            .field("cards", &self.cards)
            .field("held", &held)
            .finish()
    }
}

fn show(p: &Path) -> String {
    tiles_term::sanitize::line(&p.display().to_string(), 200)
}

/// Creates `dir` owner-only if needed, and refuses it when it is a symlink,
/// not a directory, or owned by another user.
fn checked_dir(dir: &Path) -> Result<(), String> {
    static PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| format!("cannot create the cache {}: {e}", show(dir)))?;
    let meta = fs::symlink_metadata(dir)
        .map_err(|e| format!("cannot read the cache {}: {e}", show(dir)))?;
    if !meta.file_type().is_dir() {
        return Err(format!("the cache {} is not a plain directory", show(dir)));
    }
    // A file this process creates carries this process's user id, so it names
    // the owner the directory must have without asking the OS.
    let n = PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = dir.join(format!(".owner-{}-{n}", std::process::id()));
    let _ = fs::remove_file(&probe);
    let mine = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&probe)
        .and_then(|f| f.metadata())
        .map(|m| m.uid());
    let _ = fs::remove_file(&probe);
    let mine = mine.map_err(|e| format!("cannot write in the cache {}: {e}", show(dir)))?;
    if meta.uid() != mine {
        return Err(format!(
            "the cache {} belongs to another user; remove it or set {ENV_CACHE_DIR}",
            show(dir)
        ));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("cannot make the cache {} private: {e}", show(dir)))?;
    }
    Ok(())
}

impl Cache {
    /// Opens the cache under `dir`, creating `dir/cards` owner-only.
    ///
    /// # Errors
    ///
    /// When either directory cannot be created, is not a plain directory, or
    /// belongs to another user.
    pub fn open(dir: &Path) -> Result<Self, String> {
        checked_dir(dir)?;
        let cards = dir.join("cards");
        checked_dir(&cards)?;
        Ok(Self {
            cards,
            held: Arc::default(),
        })
    }

    /// Where card `id`'s PNG lives.
    #[must_use]
    pub fn card_path(&self, id: &str) -> PathBuf {
        self.cards.join(format!("{id}.png"))
    }

    /// Writes card `id`'s PNG and removes the oldest cards past [`KEEP_CARDS`].
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn write_card(&self, id: &str, png: &[u8]) -> Result<PathBuf, String> {
        let path = self.card_path(id);
        self.write_file(&path, id, png)?;
        self.prune();
        Ok(path)
    }

    /// Writes a new card `id`'s PNG and removes the oldest cards past
    /// [`KEEP_CARDS`]; `None`, writing nothing, when anything already sits
    /// at that card's path. The PNG is linked into place, which never
    /// replaces a file, so a fresh card can never take an older card's id
    /// and repaint that card's blocks in scrollback.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn add_card(&self, id: &str, png: &[u8]) -> Result<Option<PathBuf>, String> {
        let path = self.card_path(id);
        let tmp = self.write_tmp(&path, id, png)?;
        let linked = fs::hard_link(&tmp, &path);
        let _ = fs::remove_file(&tmp);
        match linked {
            Ok(()) => {
                self.prune();
                Ok(Some(path))
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
            Err(e) => Err(format!("cannot save the card {}: {e}", show(&path))),
        }
    }

    /// Where frame `n` (from 0) of card `id`'s entrance lives.
    #[must_use]
    pub fn frame_path(&self, id: &str, n: usize) -> PathBuf {
        self.cards.join(format!("{id}-anim-{n:02}.png"))
    }

    /// Writes card `id`'s entrance frames, in order.
    ///
    /// # Errors
    ///
    /// When a file cannot be written.
    pub fn write_frames(&self, id: &str, frames: &[Vec<u8>]) -> Result<(), String> {
        for (n, png) in frames.iter().enumerate() {
            self.write_file(&self.frame_path(id, n), id, png)?;
        }
        Ok(())
    }

    /// Where card `id`'s geometry lives: `{"cols", "rows", "title", "frames",
    /// "frame_ms", "session", "theme"}`, which `tiles run` reads to place the
    /// card over its block and play its entrance, and reads again when the
    /// card is drawn in a new theme.
    #[must_use]
    pub fn meta_path(&self, id: &str) -> PathBuf {
        self.cards.join(format!("{id}.json"))
    }

    /// Writes card `id`'s geometry.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn write_meta(&self, id: &str, meta: &serde_json::Value) -> Result<(), String> {
        self.write_file(&self.meta_path(id), id, meta.to_string().as_bytes())
    }

    /// Card `id`'s geometry, when it can be read.
    #[must_use]
    pub fn read_meta(&self, id: &str) -> Option<serde_json::Value> {
        read_json(&self.meta_path(id))
    }

    /// Where card `id`'s call lives: the tool's arguments, which of its
    /// cards came from Omni and the geometry it was drawn at, so it can be
    /// drawn again in another theme ([`crate::card::redraw`]). It never holds
    /// a row fetched from Omni or a value from one (ADR-027).
    #[must_use]
    pub fn call_path(&self, id: &str) -> PathBuf {
        self.cards.join(format!("{id}.call.json"))
    }

    /// Writes card `id`'s call.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn write_call(&self, id: &str, call: &serde_json::Value) -> Result<(), String> {
        self.write_file(&self.call_path(id), id, call.to_string().as_bytes())
    }

    /// Card `id`'s call, when it was saved and can be read.
    #[must_use]
    pub fn read_call(&self, id: &str) -> Option<serde_json::Value> {
        read_json(&self.call_path(id))
    }

    /// Keeps the rows fetched from Omni for card `id` in this process's
    /// memory for as long as the card stays in the cache.
    pub fn hold(&self, id: &str, tables: Rows) {
        let Ok(mut held) = self.held.lock() else {
            return;
        };
        held.retain(|(h, _)| h != id);
        held.push_back((id.to_string(), tables));
        while held.len() > KEEP_CARDS {
            held.pop_front();
        }
    }

    /// The rows this process fetched from Omni for card `id`, when it drew
    /// the card and still holds them.
    #[must_use]
    pub fn held(&self, id: &str) -> Option<Rows> {
        let held = self.held.lock().ok()?;
        held.iter().find(|(h, _)| h == id).map(|(_, t)| t.clone())
    }

    /// Whether this process still holds the rows it fetched from Omni for
    /// card `id`, without copying them.
    #[must_use]
    pub fn holds(&self, id: &str) -> bool {
        self.held
            .lock()
            .is_ok_and(|held| held.iter().any(|(h, _)| h == id))
    }

    fn release(&self, id: &str) {
        if let Ok(mut held) = self.held.lock() {
            held.retain(|(h, _)| h != id);
        }
    }

    /// Removes card `id`'s entrance frames.
    pub fn remove_frames(&self, id: &str) {
        for n in 0.. {
            if fs::remove_file(self.frame_path(id, n)).is_err() {
                break;
            }
        }
    }

    /// The cards the `tiles run` session `session` made that can be drawn
    /// again, oldest first.
    #[must_use]
    pub fn session_cards(&self, session: u64) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.cards) else {
            return Vec::new();
        };
        let want = format!("{session:016x}");
        let mut cards: Vec<(std::time::SystemTime, String)> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let id = name.strip_suffix(".call.json")?;
                if !tiles_term::block::is_card_id(id) {
                    return None;
                }
                let meta = self.read_meta(id)?;
                (meta.get("session").and_then(serde_json::Value::as_str) == Some(want.as_str()))
                    .then_some(())?;
                Some((e.metadata().ok()?.modified().ok()?, id.to_string()))
            })
            .collect();
        cards.sort();
        cards.into_iter().map(|(_, id)| id).collect()
    }

    /// Removes every card with its geometry, call and entrance frames, and
    /// any temporary file a write left behind, and returns how many cards
    /// went. The cards directory stays, and so does `run/`, where a live
    /// `tiles run` keeps its add-ons.
    ///
    /// # Errors
    ///
    /// When the cards directory cannot be listed or a file cannot be removed.
    pub fn clear(&self) -> Result<usize, String> {
        let entries = fs::read_dir(&self.cards)
            .map_err(|e| format!("cannot list the cards in {}: {e}", show(&self.cards)))?;
        let mut cards = 0;
        for entry in entries.filter_map(Result::ok) {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Some(kind) = card_file(&name) else {
                continue;
            };
            let path = entry.path();
            fs::remove_file(&path).map_err(|e| format!("cannot remove {}: {e}", show(&path)))?;
            if kind == CardFile::Card {
                cards += 1;
            }
        }
        if let Ok(mut held) = self.held.lock() {
            held.clear();
        }
        Ok(cards)
    }

    fn write_file(&self, path: &Path, id: &str, bytes: &[u8]) -> Result<(), String> {
        let tmp = self.write_tmp(path, id, bytes)?;
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("cannot save the card {}: {e}", show(path)));
        }
        Ok(())
    }

    /// `bytes` in a fresh owner-only temporary file beside the cards, for
    /// renaming or linking to `path`.
    fn write_tmp(&self, path: &Path, id: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        static TMPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = TMPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = self
            .cards
            .join(format!(".{id}.{}-{n}.tmp", std::process::id()));
        let _ = fs::remove_file(&tmp);
        let written = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()));
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(format!("cannot save the card {}: {e}", show(path)));
        }
        Ok(tmp)
    }

    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.cards) else {
            return;
        };
        // A card is `<id>.png`; its entrance frames go with it.
        let card = |p: &Path| {
            p.extension().is_some_and(|x| x == "png")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(tiles_term::block::is_card_id)
        };
        let mut cards: Vec<(std::time::SystemTime, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter(|e| card(&e.path()))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if cards.len() <= KEEP_CARDS {
            return;
        }
        cards.sort();
        for (_, path) in &cards[..cards.len() - KEEP_CARDS] {
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(path.with_extension("json"));
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let _ = fs::remove_file(self.call_path(id));
            self.remove_frames(id);
            self.release(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::{Cache, KEEP_CARDS, default_dir};

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tiles-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_cache_is_owner_only_and_cards_are_private() {
        let dir = temp("perm");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cache = Cache::open(&dir).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("cards")), 0o700);
        let path = cache.write_card("abc123", b"png").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"png");
        assert_eq!(mode(&path), 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_symlinked_cache_is_refused() {
        let dir = temp("link");
        let real = temp("link-real");
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &dir).unwrap();
        let e = Cache::open(&dir).unwrap_err();
        assert!(e.contains("not a plain directory"), "{e}");
        let _ = std::fs::remove_file(&dir);
        let _ = std::fs::remove_dir_all(&real);
    }

    #[test]
    fn a_planted_symlink_at_the_card_path_is_replaced_not_followed() {
        let dir = temp("plant");
        let cache = Cache::open(&dir).unwrap();
        let victim = temp("victim");
        std::fs::write(&victim, b"keep").unwrap();
        std::os::unix::fs::symlink(&victim, cache.card_path("abc123")).unwrap();
        cache.write_card("abc123", b"png").unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&victim);
    }

    // The security review's F9: a new card written under a taken id
    // replaced the older card, and every block naming it repainted.
    #[test]
    fn a_new_card_never_takes_an_existing_id() {
        let dir = temp("add");
        let cache = Cache::open(&dir).unwrap();
        let path = cache.add_card("abc123", b"first").unwrap().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(cache.add_card("abc123", b"second").unwrap(), None);
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        let victim = temp("add-victim");
        std::os::unix::fs::symlink(&victim, cache.card_path("def456")).unwrap();
        assert_eq!(
            cache.add_card("def456", b"png").unwrap(),
            None,
            "a planted link is a taken id"
        );
        assert!(!victim.exists(), "and is not followed");
        let left: Vec<_> = std::fs::read_dir(dir.join("cards"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "no temporary file is left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_cards_are_pruned() {
        let dir = temp("prune");
        let cache = Cache::open(&dir).unwrap();
        for i in 0..KEEP_CARDS + 3 {
            cache.write_card(&format!("{i:06x}"), b"p").unwrap();
        }
        let count = std::fs::read_dir(dir.join("cards")).unwrap().count();
        assert_eq!(count, KEEP_CARDS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frames_sit_beside_their_card_and_go_with_it() {
        let dir = temp("frames");
        let cache = Cache::open(&dir).unwrap();
        cache.write_card("000000", b"p").unwrap();
        cache
            .write_frames("000000", &[b"f0".to_vec(), b"f1".to_vec()])
            .unwrap();
        assert_eq!(
            std::fs::read(dir.join("cards/000000-anim-01.png")).unwrap(),
            b"f1"
        );
        // Frames are not cards: KEEP_CARDS more cards still keep the first
        // until one more arrives.
        for i in 1..KEEP_CARDS {
            cache.write_card(&format!("{i:06x}"), b"p").unwrap();
        }
        assert!(cache.card_path("000000").exists());
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.write_card("ffffff", b"p").unwrap();
        assert!(!cache.card_path("000000").exists());
        assert!(!cache.frame_path("000000", 0).exists());
        assert!(!cache.frame_path("000000", 1).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_removes_every_card_file_and_nothing_else() {
        let dir = temp("clear");
        let cache = Cache::open(&dir).unwrap();
        for id in ["000001", "000002"] {
            cache.write_card(id, b"p").unwrap();
            cache.write_meta(id, &serde_json::json!({})).unwrap();
            cache.write_call(id, &serde_json::json!({})).unwrap();
        }
        cache.write_frames("000001", &[b"f0".to_vec()]).unwrap();
        cache.hold("000001", vec![None]);
        assert!(cache.holds("000001") && !cache.holds("000002"));
        let cards = dir.join("cards");
        std::fs::write(cards.join(".000002.123-0.tmp"), b"half").unwrap();
        std::fs::write(cards.join("notes.txt"), b"mine").unwrap();
        std::fs::write(cards.join("0000zz.png"), b"not a card id").unwrap();
        std::fs::create_dir_all(dir.join("run/42")).unwrap();
        assert_eq!(cache.clear().unwrap(), 2);
        let mut left: Vec<String> = std::fs::read_dir(&cards)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["0000zz.png", "notes.txt"]);
        assert!(dir.join("run/42").is_dir(), "a live session's add-ons stay");
        assert!(!cache.holds("000001") && cache.held("000001").is_none());
        assert_eq!(cache.clear().unwrap(), 0, "an empty cache clears to zero");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_dir_follows_the_override_then_home() {
        let env = |k: &str| match k {
            "HOME" => Some("/home/u".to_string()),
            _ => None,
        };
        let dir = default_dir(&env).unwrap();
        assert!(dir.starts_with("/home/u"));
        let over = |k: &str| (k == "TILES_CACHE_DIR").then(|| "/x/y".to_string());
        assert_eq!(
            default_dir(&over).unwrap(),
            std::path::PathBuf::from("/x/y")
        );
        assert!(default_dir(&|_| None).is_none());
    }
}
