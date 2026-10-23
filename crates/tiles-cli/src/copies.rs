//! The other `tiles` on `PATH` (ADR-035). `tiles setup` and `tiles doctor`
//! name each one with the version its `--version` prints within
//! [`PATIENCE`]. A copy ahead of the running `tiles` is the one a terminal
//! runs, so when it is older, or its version cannot be read, setup offers
//! to move it aside: renamed `tiles-<version>-old` beside itself, never
//! deleted, and only when the user owns it. Without a terminal to ask,
//! setup and doctor print the `mv` command instead. One the same or newer
//! is named so and left.

use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tiles_term::sanitize;

use crate::update::Version;

/// The most characters in one line: enough for two paths and the words
/// around them, so an `mv` command is never cut.
pub const LONGEST: usize = 4096;

/// The longest another copy's `--version` may take.
pub const PATIENCE: Duration = Duration::from_secs(5);

/// Where another copy is on `PATH`, against the running `tiles`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Before the running one; the first of these is what a terminal runs.
    Ahead,
    /// After the running one.
    Behind,
    /// On `PATH`, where the running one is not.
    Only,
}

/// Another `tiles` on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Other {
    /// Where it is, as `PATH` names it.
    pub path: PathBuf,
    /// The version its `--version` named, `None` when it named none.
    pub version: Option<String>,
    /// Its version against the running one's, `None` when either is not a
    /// release's `X.Y.Z`.
    pub against: Option<Ordering>,
    /// Where it is against the running one.
    pub place: Place,
    /// Whether the user running this owns it.
    pub yours: bool,
    /// The free name beside it it would be moved to, `None` when there is
    /// none.
    pub aside: Option<PathBuf>,
}

/// The running `tiles` and every other one on `PATH`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Copies {
    /// The running `tiles`.
    pub this: PathBuf,
    /// The running one's version.
    pub current: String,
    /// The others, in `PATH` order.
    pub others: Vec<Other>,
}

/// The version in `answer`, a `tiles --version` output such as `tiles 0.1.2
/// (intent schema 1)`: its second word, when the first is `tiles` and the
/// second is fit for a file name.
fn named(answer: &str) -> Option<String> {
    let line = answer.lines().find(|l| !l.trim().is_empty())?;
    let mut words = line.split_whitespace();
    if words.next()? != "tiles" {
        return None;
    }
    let v = words.next()?;
    let fits = v.len() <= 40
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-' | '_'));
    fits.then(|| v.to_string())
}

/// The first free `tiles-<version>-old` beside `path`, then `-2` to `-9`.
fn free_name(path: &Path, version: Option<&str>) -> Option<PathBuf> {
    let dir = path.parent()?;
    let stem = format!("tiles-{}-old", version.unwrap_or("unknown"));
    std::iter::once(stem.clone())
        .chain((2..10).map(|n| format!("{stem}-{n}")))
        .map(|name| dir.join(name))
        .find(|p| fs::symlink_metadata(p).is_err())
}

/// Each `tiles` in `path`, a `PATH` value, that is not `this`, which is told
/// apart by its file, so a symlink or hard link to `this` is `this`.
/// `current` is the running one's version, `uid` the user running this,
/// and `version` asks a copy its `--version`. A directory `PATH` names
/// relatively is skipped.
#[must_use]
pub fn find(
    path: &OsStr,
    this: &Path,
    current: &str,
    uid: u32,
    version: &dyn Fn(&Path) -> Option<String>,
) -> Copies {
    let release = |v: &str| Version::parse(v).filter(|v| !v.is_development());
    let running = release(current);
    let id = |p: &Path| fs::metadata(p).ok().map(|m| (m.dev(), m.ino()));
    let here = id(this);
    let mut seen = Vec::new();
    let mut on_path = false;
    let mut found = Vec::new();
    for dir in std::env::split_paths(path).filter(|d| d.is_absolute()) {
        let p = dir.join("tiles");
        let Some(file) = id(&p).filter(|_| p.is_file()) else {
            continue;
        };
        if seen.contains(&file) {
            continue;
        }
        seen.push(file);
        if Some(file) == here {
            on_path = true;
        } else {
            found.push((p, on_path));
        }
    }
    let others = found
        .into_iter()
        .map(|(p, behind)| {
            let version = version(&p).as_deref().and_then(named);
            Other {
                against: version
                    .as_deref()
                    .and_then(release)
                    .zip(running)
                    .map(|(v, r)| v.cmp(&r)),
                place: match (behind, on_path) {
                    (true, _) => Place::Behind,
                    (false, true) => Place::Ahead,
                    (false, false) => Place::Only,
                },
                yours: fs::symlink_metadata(&p).is_ok_and(|m| m.uid() == uid),
                aside: free_name(&p, version.as_deref()),
                version,
                path: p,
            }
        })
        .collect();
    Copies {
        this: this.to_path_buf(),
        current: current.to_string(),
        others,
    }
}

/// [`find`] on this machine, for the running `tiles`.
pub(crate) fn here() -> Copies {
    find(
        &std::env::var_os("PATH").unwrap_or_default(),
        &crate::this_tiles(),
        tiles_intent::TILES_VERSION,
        rustix::process::geteuid().as_raw(),
        &|p| crate::opencode::ask_with(p, &["--version"], PATIENCE),
    )
}

/// `p` as one shell word.
fn word(p: &Path) -> String {
    let s = p.display().to_string();
    let plain = !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ',' | ':' | '@')
        });
    if plain {
        s
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

impl Other {
    /// The command that moves it aside, `sudo` first when it is not the
    /// user's; `None` when no name beside it is free.
    #[must_use]
    pub fn mv(&self) -> Option<String> {
        let to = self.aside.as_deref()?;
        let sudo = if self.yours { "" } else { "sudo " };
        Some(format!("{sudo}mv -n {} {}", word(&self.path), word(to)))
    }

    /// Renames it to [`Other::aside`], unless that name was taken since.
    /// Returns the new path.
    ///
    /// # Errors
    ///
    /// When there is no free name, or the rename fails.
    pub fn move_aside(&self) -> Result<PathBuf, String> {
        let to = self
            .aside
            .clone()
            .ok_or_else(|| "every tiles-<version>-old name beside it is taken".to_string())?;
        if fs::symlink_metadata(&to).is_ok() {
            return Err(format!("{} exists now", to.display()));
        }
        fs::rename(&self.path, &to).map_err(|e| e.to_string())?;
        Ok(to)
    }

    /// Whether it is ahead of the running one and older, or of a version
    /// that cannot be read.
    #[must_use]
    pub fn stale(&self) -> bool {
        self.place == Place::Ahead
            && !matches!(self.against, Some(Ordering::Equal | Ordering::Greater))
    }

    /// Whether setup offers to move it: [`Other::stale`], the user's, and
    /// with a free name beside it.
    #[must_use]
    pub fn offered(&self) -> bool {
        self.stale() && self.yours && self.aside.is_some()
    }
}

impl Copies {
    /// One line per other copy, for setup's `Tiles found:` list. A stale
    /// copy that is not the user's carries the `sudo mv` that moves it.
    #[must_use]
    pub fn found(&self) -> Vec<String> {
        let clean = |s: &str| sanitize::line(s, LONGEST);
        let this = self.this.display();
        let current = &self.current;
        let mut first = true;
        self.others
            .iter()
            .map(|o| {
                let what = format!(
                    "tiles: {}, {}",
                    o.path.display(),
                    o.version
                        .as_deref()
                        .map_or_else(|| "which printed no version".into(), |v| format!("version {v}"))
                );
                let rank = match o.against {
                    Some(Ordering::Greater) => Some("newer than"),
                    Some(Ordering::Equal) => Some("the same version as"),
                    _ => None,
                };
                let line = match (o.place, rank) {
                    (Place::Ahead, None) if first => {
                        first = false;
                        format!(
                            "{what}, comes first on PATH, so a terminal runs it, not this one ({this}, version {current})"
                        )
                    }
                    (Place::Ahead, Some(rank)) if first => {
                        first = false;
                        format!(
                            "{what}, is {rank} this one ({this}, version {current}) and comes first on PATH, so a terminal runs it"
                        )
                    }
                    (Place::Ahead, None) => format!("{what}, is also ahead of this one on PATH"),
                    (Place::Ahead, Some(rank)) => {
                        format!("{what}, is {rank} this one and also ahead of it on PATH")
                    }
                    (Place::Behind, _) => format!("{what}, is later on PATH than this one"),
                    (Place::Only, _) => {
                        format!("{what}, is on PATH, and this one ({this}) is not")
                    }
                };
                match o.mv() {
                    Some(mv) if o.stale() && !o.yours => clean(&format!(
                        "{line}; it is not yours, so Tiles leaves it: {mv}"
                    )),
                    _ => clean(&line),
                }
            })
            .collect()
    }

    /// The copies setup offers to move aside.
    #[must_use]
    pub fn moves(&self) -> Vec<&Other> {
        self.others.iter().filter(|o| o.offered()).collect()
    }

    /// `tiles doctor`'s lines: one per copy ahead of this one, a stale one
    /// with the command that moves it aside.
    #[must_use]
    pub fn doctor(&self) -> Vec<String> {
        self.others
            .iter()
            .zip(self.found())
            .filter(|(o, _)| o.place == Place::Ahead)
            .map(|(o, line)| match o.mv() {
                _ if !o.stale() => line,
                Some(mv) if o.yours => {
                    sanitize::line(&format!("{line}; move it aside: {mv}"), LONGEST)
                }
                Some(_) => line,
                None => format!("{line}; move or remove it"),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    use super::{Copies, Place, find, named};

    fn dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tiles-copies-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// A `tiles` in `root/<bin>` whose `--version` prints `version`.
    fn tiles(root: &Path, bin: &str, version: &str) -> PathBuf {
        let d = root.join(bin);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("tiles");
        std::fs::write(&p, format!("#!/bin/sh\nprintf '%s' '{version}'\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn path(root: &Path, bins: &[&str]) -> std::ffi::OsString {
        std::env::join_paths(bins.iter().map(|b| root.join(b))).unwrap()
    }

    fn me(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().uid()
    }

    /// A copy's `--version`, with room for a test machine under load.
    fn ask(p: &Path) -> Option<String> {
        crate::opencode::ask_with(p, &["--version"], std::time::Duration::from_secs(30))
    }

    #[test]
    fn a_version_is_the_word_after_tiles_and_fit_for_a_file_name() {
        assert_eq!(
            named("tiles 0.1.2 (intent schema 1)\n").as_deref(),
            Some("0.1.2")
        );
        assert_eq!(
            named("\n tiles 0.2.0-rc.1\n").as_deref(),
            Some("0.2.0-rc.1")
        );
        for none in [
            "",
            "\n",
            "tiles",
            "other 0.1.2",
            "tiles ../x",
            "tiles \u{1b}[2J",
        ] {
            assert_eq!(named(none), None, "{none:?}");
        }
    }

    #[test]
    fn an_older_copy_ahead_is_offered_and_a_newer_one_behind_is_only_named() {
        let root = dir("ahead");
        let old = tiles(&root, "local", "tiles 0.1.2 (intent schema 1)\n");
        let this = tiles(&root, "usr", "tiles 0.1.9 (intent schema 1)\n");
        let newer = tiles(&root, "brew", "tiles 0.2.0 (intent schema 1)\n");
        let found = find(
            &path(&root, &["local", "usr", "brew"]),
            &this,
            "0.1.9",
            me(&this),
            &ask,
        );
        assert_eq!(found.others.len(), 2);
        let (ahead, behind) = (&found.others[0], &found.others[1]);
        assert_eq!(
            (ahead.path.as_path(), ahead.place),
            (old.as_path(), Place::Ahead)
        );
        assert_eq!(ahead.version.as_deref(), Some("0.1.2"));
        assert_eq!(ahead.aside, Some(root.join("local/tiles-0.1.2-old")));
        assert_eq!(
            (behind.path.as_path(), behind.place),
            (newer.as_path(), Place::Behind)
        );
        assert_eq!(found.moves(), [ahead]);
        let lines = found.found();
        assert_eq!(
            lines[0],
            format!(
                "tiles: {}, version 0.1.2, comes first on PATH, so a terminal runs it, not this one ({}, version 0.1.9)",
                old.display(),
                this.display(),
            )
        );
        assert_eq!(
            lines[1],
            format!(
                "tiles: {}, version 0.2.0, is later on PATH than this one",
                newer.display()
            )
        );
        let doctor = found.doctor();
        assert_eq!(doctor.len(), 1, "{doctor:?}");
        assert!(
            doctor[0].ends_with(&format!(
                "; move it aside: mv -n {} {}",
                old.display(),
                root.join("local/tiles-0.1.2-old").display()
            )),
            "{doctor:?}"
        );
        // Moving it renames it beside itself and keeps every byte.
        let before = std::fs::read(&old).unwrap();
        let to = ahead.move_aside().unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), before);
        assert!(!old.exists());
        assert_eq!(
            find(
                &path(&root, &["local", "usr", "brew"]),
                &this,
                "0.1.9",
                me(&this),
                &ask
            )
            .moves(),
            Vec::<&super::Other>::new()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_copy_of_another_user_is_named_with_sudo_and_never_offered() {
        let root = dir("theirs");
        let old = tiles(&root, "opt", "tiles 0.1.2\n");
        let this = tiles(&root, "usr", "tiles 0.1.9\n");
        let found = find(
            &path(&root, &["opt", "usr"]),
            &this,
            "0.1.9",
            me(&this) + 1,
            &ask,
        );
        assert!(!found.others[0].yours);
        assert_eq!(found.moves(), Vec::<&super::Other>::new());
        let mv = format!(
            "sudo mv -n {} {}",
            old.display(),
            root.join("opt/tiles-0.1.2-old").display()
        );
        assert!(
            found.found()[0].ends_with(&format!("; it is not yours, so Tiles leaves it: {mv}")),
            "{:?}",
            found.found()
        );
        assert_eq!(found.doctor(), found.found());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_broken_copy_is_named_without_a_version_and_moved_to_an_unknown_name() {
        let root = dir("broken");
        let silent = tiles(&root, "a", "");
        let this = tiles(&root, "usr", "tiles 0.1.9\n");
        // One that never answers is cut off at the patience, and one that
        // is a symlink to this one, or a second mention of a directory, is
        // not another copy.
        let stuck = root.join("b/tiles");
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(&stuck, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(root.join("c")).unwrap();
        std::os::unix::fs::symlink(&this, root.join("c/tiles")).unwrap();
        std::fs::write(root.join("a/tiles-unknown-old"), "taken").unwrap();
        let started = std::time::Instant::now();
        let found = find(
            &path(&root, &["a", "b", "c", "a", "usr"]),
            &this,
            "0.1.9",
            me(&this),
            &|p| crate::opencode::ask_with(p, &["--version"], super::PATIENCE),
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        let paths: Vec<&Path> = found.others.iter().map(|o| o.path.as_path()).collect();
        assert_eq!(paths, [silent.as_path(), stuck.as_path()]);
        assert!(found.others.iter().all(|o| o.version.is_none()));
        assert_eq!(
            found.others[0].aside,
            Some(root.join("a/tiles-unknown-old-2"))
        );
        assert!(found.found()[0].contains(", which printed no version, comes first on PATH"));
        assert!(found.found()[1].contains(", is also ahead of this one on PATH"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn with_this_one_off_path_nothing_is_offered_and_odd_paths_are_quoted() {
        let root = dir("off");
        let other = tiles(&root, "it's here", "tiles 0.1.2\n");
        let this = root.join("target/tiles");
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(&this, "x").unwrap();
        let found = find(
            &path(&root, &["it's here"]),
            &this,
            "0.1.9",
            me(&this),
            &ask,
        );
        assert_eq!(found.others[0].place, Place::Only);
        assert!(found.moves().is_empty() && found.doctor().is_empty());
        assert_eq!(
            found.others[0].mv().unwrap(),
            format!(
                "mv -n '{}' '{}'",
                other.display().to_string().replace('\'', r"'\''"),
                root.join("it's here/tiles-0.1.2-old")
                    .display()
                    .to_string()
                    .replace('\'', r"'\''")
            )
        );
        assert_eq!(Copies::default().found(), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_copy_ahead_the_same_or_newer_is_named_so_and_never_offered() {
        let root = dir("newer");
        let newer = tiles(&root, "local", "tiles 0.2.0 (intent schema 1)\n");
        let same = tiles(&root, "brew", "tiles 0.1.9\n");
        let this = tiles(&root, "usr", "tiles 0.1.9\n");
        let bins = ["local", "brew", "usr"];
        for uid in [me(&this), me(&this) + 1] {
            let found = find(&path(&root, &bins), &this, "0.1.9", uid, &ask);
            assert_eq!(found.others.len(), 2);
            assert!(
                found
                    .others
                    .iter()
                    .all(|o| o.place == Place::Ahead && !o.stale())
            );
            assert_eq!(found.moves(), Vec::<&super::Other>::new());
            let lines = found.found();
            assert_eq!(
                lines,
                [
                    format!(
                        "tiles: {}, version 0.2.0, is newer than this one ({}, version 0.1.9) and comes first on PATH, so a terminal runs it",
                        newer.display(),
                        this.display()
                    ),
                    format!(
                        "tiles: {}, version 0.1.9, is the same version as this one and also ahead of it on PATH",
                        same.display()
                    ),
                ]
            );
            assert_eq!(found.doctor(), lines);
        }
        // A development build compares with no release, so an older-looking
        // copy ahead of it is offered as one whose version cannot be read.
        let dev = find(&path(&root, &bins), &this, "0.0.0", me(&this), &ask);
        assert_eq!(dev.moves().len(), 2);
        assert!(
            dev.found()[0].contains(", comes first on PATH, so a terminal runs it, not this one (")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
