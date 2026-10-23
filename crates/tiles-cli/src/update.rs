//! `tiles update [--check] [--allow-older] [vX.Y.Z]` and the launch notice
//! (ADR-032).
//!
//! `tiles update` fetches a release from the releases location: with
//! `gh release download` when `gh` is on `PATH` and signed in, else from an
//! archive downloaded by hand and named by `TILES_ARCHIVE` and `TILES_SUMS`. It checks the archive against the
//! release's `SHA256SUMS`, removes macOS's quarantine flag from a by-hand
//! archive and from the unpacked binary, writes the binary beside the
//! installed one, runs its `--version`, and renames it over. Nothing updates
//! without the command.
//!
//! The notice: at most once a day, `tiles run` and `tiles doctor` send one
//! anonymous `GET` of the releases location's latest-release metadata to
//! `api.github.com` over [`tiles_net::Client`], and print one line when a
//! newer release exists. `TILES_UPDATE_CHECK=off` turns it off. This file is
//! the only shipped code that names a GitHub host
//! (`scripts/check-network-owners.py`).

use std::fmt;
use std::fs;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tiles_term::sanitize;

/// Where releases live until the open-core launch moves them; a public
/// launch changes this constant and the test that names it.
pub const RELEASES_REPO: &str = "tileshq/tiles";
/// Names another releases location, as `<owner>/<repo>`.
pub const ENV_REPO: &str = "TILES_RELEASES_REPO";
/// `off`, `0`, `false` or `no`, in any case, turns the daily check off.
pub const ENV_CHECK: &str = "TILES_UPDATE_CHECK";
/// Moves the state directory that holds the check's timestamp.
pub const ENV_STATE_DIR: &str = "TILES_STATE_DIR";
/// Usage of `tiles update`.
pub const USAGE: &str = "tiles update [--check] [--allow-older] [vX.Y.Z]";
/// The longest the daily check may take.
pub const CHECK_DEADLINE: Duration = Duration::from_secs(2);
/// Seconds between two checks.
pub const CHECK_EVERY: u64 = 24 * 60 * 60;

/// Most bytes read of the latest-release metadata.
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
/// Most bytes read of the state file.
const MAX_STATE_BYTES: u64 = 4 * 1024;
/// Most bytes read of `SHA256SUMS`.
const MAX_SUMS_BYTES: u64 = 64 * 1024;
/// Most bytes read of a release archive or its binary.
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
/// The extended attribute a browser download carries on macOS.
const QUARANTINE: &str = "com.apple.quarantine";
/// The state file, in the state directory.
const STATE_FILE: &str = "update-check.json";
/// How many names a temporary file or directory tries.
const NAMES: u32 = 8;

/// Temporary names made by this process, numbered.
static MADE: AtomicU32 = AtomicU32::new(0);

/// A release version, `X.Y.Z`; its text is digits and dots only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u32, u32, u32);

impl Version {
    /// `vX.Y.Z` or `X.Y.Z`, each part at most nine digits; anything else is
    /// `None`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let parts: Vec<&str> = text.split('.').collect();
        let [a, b, c] = parts.as_slice() else {
            return None;
        };
        let part = |p: &str| {
            (!p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
                .then(|| p.parse().ok())
                .flatten()
        };
        Some(Self(part(a)?, part(b)?, part(c)?))
    }

    /// Whether this is the version of a build made outside the release job.
    pub(crate) fn is_development(self) -> bool {
        self == Self(0, 0, 0)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The releases location: [`RELEASES_REPO`], or [`ENV_REPO`] when set.
///
/// # Errors
///
/// When [`ENV_REPO`] is not `<owner>/<repo>` in letters, digits, `-`, `_`
/// and `.`.
pub fn repo(env: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let Some(value) = env(ENV_REPO).filter(|v| !v.is_empty()) else {
        return Ok(RELEASES_REPO.to_string());
    };
    let part = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p != "."
            && p != ".."
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if value
        .split_once('/')
        .is_some_and(|(owner, name)| part(owner) && part(name))
    {
        Ok(value)
    } else {
        Err(format!(
            "{ENV_REPO}=\"{}\" is not <owner>/<repo>",
            sanitize::line(&value, 80)
        ))
    }
}

/// The release build for this Mac.
///
/// # Errors
///
/// On a platform Tiles ships no release for.
pub fn target() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        (os, arch) => Err(format!(
            "no release build for {os} {arch}; Tiles ships for macOS on Apple Silicon and Intel"
        )),
    }
}

fn archive_name(version: Version, target: &str) -> String {
    format!("tiles-v{version}-{target}.tar.gz")
}

/// The version in an archive's file name, `tiles-vX.Y.Z-<target>.tar.gz`.
fn archive_version(file_name: &str, target: &str) -> Option<Version> {
    let rest = file_name.strip_prefix("tiles-")?.strip_suffix(".tar.gz")?;
    Version::parse(rest.strip_suffix(target)?.strip_suffix('-')?)
}

/// `SHA256SUMS`' sum for `name`, from lines of `<64 hex digits>  <name>`.
fn expected_sum(sums: &str, name: &str) -> Result<[u8; 32], String> {
    let mut found: Option<[u8; 32]> = None;
    for line in sums.lines() {
        let Some((hash, rest)) = line.split_once(' ') else {
            continue;
        };
        let file = rest
            .strip_prefix(' ')
            .or_else(|| rest.strip_prefix('*'))
            .unwrap_or(rest);
        if file != name {
            continue;
        }
        let sum = hex32(hash).ok_or_else(|| format!("SHA256SUMS' line for {name} is not a sum"))?;
        if found.replace(sum).is_some_and(|before| before != sum) {
            return Err(format!("SHA256SUMS lists {name} with two sums"));
        }
    }
    found.ok_or_else(|| format!("SHA256SUMS has no line for {name}"))
}

fn hex32(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let digit = |b: u8| char::from(b).to_digit(16);
    let mut out = [0; 32];
    for (i, pair) in bytes.chunks(2).enumerate() {
        out[i] = u8::try_from(digit(pair[0])? * 16 + digit(pair[1])?).ok()?;
    }
    Some(out)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut out = [0; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

/// Removes macOS's quarantine flag from `path`, not following a symlink;
/// a file without one is left as it is. Elsewhere there is no flag.
///
/// # Errors
///
/// When the flag is there and cannot be removed.
pub fn strip_quarantine(path: &Path) -> Result<(), String> {
    remove_quarantine(path).map_err(|e| {
        format!(
            "cannot remove the {QUARANTINE} flag from {}: {e}",
            sanitize::line(&path.display().to_string(), 200)
        )
    })
}

#[cfg(target_os = "macos")]
fn remove_quarantine(path: &Path) -> Result<(), rustix::io::Errno> {
    match rustix::fs::lremovexattr(path, QUARANTINE) {
        Ok(()) | Err(rustix::io::Errno::NOATTR) => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps)] // macOS's can fail.
fn remove_quarantine(_: &Path) -> Result<(), rustix::io::Errno> {
    Ok(())
}

/// A parsed `tiles update`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Args {
    /// `--check`: report and change nothing.
    pub check: bool,
    /// `--allow-older`: install a release older than this one.
    pub allow_older: bool,
    /// The release to install; the latest when `None`.
    pub tag: Option<Version>,
}

/// Parses the arguments after `update`.
///
/// # Errors
///
/// An unknown option, a tag that is not `vX.Y.Z`, or `--check` with a tag.
pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    for a in args {
        match a.as_str() {
            "--check" => out.check = true,
            "--allow-older" => out.allow_older = true,
            tag => {
                let version = Version::parse(tag).filter(|_| tag.starts_with('v'));
                let Some(version) = version.filter(|_| out.tag.is_none()) else {
                    return Err(format!(
                        "\"{}\" is not a release tag such as v0.1.4; usage: {USAGE}",
                        sanitize::line(tag, 40)
                    ));
                };
                out.tag = Some(version);
            }
        }
    }
    if out.check && out.tag.is_some() {
        return Err(format!(
            "--check reports the latest release, so it takes no tag; usage: {USAGE}"
        ));
    }
    Ok(out)
}

/// What `tiles update` works with.
pub struct Context<'a> {
    /// One environment variable.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// `gh` on `PATH`.
    pub gh: Option<PathBuf>,
    /// This binary's version.
    pub current: &'a str,
    /// The release build for this Mac.
    pub target: &'a str,
    /// The `tiles` to replace: `$TILES_BIN_DIR/tiles`, else
    /// `~/.local/bin/tiles`, where `scripts/install.sh` puts it.
    pub dest: PathBuf,
    /// The binary running now, to say when it is not `dest`.
    pub running: Option<PathBuf>,
    /// Where the download is unpacked.
    pub work: PathBuf,
    /// The latest release by the anonymous metadata `GET`, for `--check`
    /// without `gh`.
    pub latest: &'a dyn Fn(&str) -> Result<Version, String>,
}

/// The two files of a release downloaded by hand.
struct ByHand {
    archive: PathBuf,
    sums: PathBuf,
}

fn by_hand(env: &dyn Fn(&str) -> Option<String>) -> Result<Option<ByHand>, String> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    match (set("TILES_ARCHIVE"), set("TILES_SUMS")) {
        (None, None) => Ok(None),
        (Some(archive), Some(sums)) => Ok(Some(ByHand { archive, sums })),
        (Some(_), None) => Err("set TILES_SUMS to the release's SHA256SUMS file too".into()),
        (None, Some(_)) => {
            Err("set TILES_ARCHIVE to the downloaded tiles-<tag>-<target>.tar.gz too".into())
        }
    }
}

fn gh_signed_in(gh: &Path) -> bool {
    Command::new(gh)
        .args(["auth", "status"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `gh args`' standard output, or its last line of complaint.
fn gh_run(gh: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(gh)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run gh: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let last = said
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    Err(format!(
        "gh {} failed: {}",
        args.iter().take(2).copied().collect::<Vec<_>>().join(" "),
        sanitize::line(last, 200)
    ))
}

fn latest_by_gh(gh: &Path, repo: &str) -> Result<Version, String> {
    let tag = gh_run(
        gh,
        &[
            "release", "view", "--repo", repo, "--json", "tagName", "--jq", ".tagName",
        ],
    )?;
    Version::parse(tag.trim()).ok_or_else(|| {
        format!(
            "the latest release of {repo} has the tag \"{}\", not vX.Y.Z",
            sanitize::line(tag.trim(), 40)
        )
    })
}

/// How to download a release by hand, for a machine without `gh` signed in.
fn steps(repo: &str, target: &str, tag: Option<Version>, why: &str) -> String {
    let (page, file) = match tag {
        Some(t) => (
            format!("https://github.com/{repo}/releases/tag/v{t}"),
            archive_name(t, target),
        ),
        None => (
            format!("https://github.com/{repo}/releases/latest"),
            format!("tiles-<tag>-{target}.tar.gz"),
        ),
    };
    let mut out = vec![
        format!("{why}, so tiles update cannot download the release itself. Download it by hand:"),
        format!(
            "  1. open {page} in a browser signed in to GitHub as an account that can read {repo}"
        ),
        format!("  2. download {file} and SHA256SUMS"),
        format!(
            "  3. run TILES_ARCHIVE=~/Downloads/{file} TILES_SUMS=~/Downloads/SHA256SUMS tiles update"
        ),
    ];
    if tag.is_none() {
        out.push("     with <tag> the release's tag, such as v0.1.5".into());
    }
    out.push("tiles update checks the archive against SHA256SUMS and removes the quarantine flag the browser adds. Or install gh, run gh auth login, then tiles update".into());
    out.join("\n")
}

/// A directory of this process's own, removed when dropped.
struct WorkDir(PathBuf);

impl WorkDir {
    fn new(parent: &Path) -> Result<Self, String> {
        for _ in 0..NAMES {
            let dir = parent.join(format!(
                "tiles-update-{}-{}",
                std::process::id(),
                MADE.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => return Ok(Self(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("cannot make a directory to unpack in: {e}")),
            }
        }
        Err("cannot make a directory to unpack in".into())
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The new binary written beside the one it replaces, removed when dropped
/// unless renamed over it.
struct Beside(Option<PathBuf>);

impl Beside {
    fn write(dest: &Path, bytes: &[u8]) -> Result<Self, String> {
        let dir = dest
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let shown = |p: &Path| sanitize::line(&p.display().to_string(), 200);
        fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", shown(dir)))?;
        for _ in 0..NAMES {
            let path = dir.join(format!(
                ".tiles.update.{}.{}",
                std::process::id(),
                MADE.fetch_add(1, Ordering::Relaxed)
            ));
            let file = fs::File::options()
                .write(true)
                .create_new(true)
                .mode(0o755)
                .open(&path);
            match file {
                Ok(mut f) => {
                    let beside = Self(Some(path));
                    f.write_all(bytes)
                        .and_then(|()| f.sync_all())
                        .map_err(|e| format!("cannot write beside {}: {e}", shown(dest)))?;
                    return Ok(beside);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("cannot write beside {}: {e}", shown(dest))),
            }
        }
        Err(format!("cannot write beside {}", shown(dest)))
    }

    fn path(&self) -> &Path {
        self.0.as_deref().unwrap_or(Path::new(""))
    }

    /// Renames the new binary over `dest` in one step.
    fn rename_over(mut self, dest: &Path) -> Result<(), String> {
        let path = self.0.take().unwrap_or_default();
        fs::rename(&path, dest).map_err(|e| {
            let _ = fs::remove_file(&path);
            format!(
                "cannot replace {}: {e}",
                sanitize::line(&dest.display().to_string(), 200)
            )
        })
    }
}

impl Drop for Beside {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(cap + 1).read_to_end(&mut bytes))
        .map_err(|e| {
            format!(
                "cannot read {}: {e}",
                sanitize::line(&path.display().to_string(), 200)
            )
        })?;
    if bytes.len() as u64 > cap {
        return Err(format!(
            "{} is larger than a release file",
            sanitize::line(&path.display().to_string(), 200)
        ));
    }
    Ok(bytes)
}

/// Whether to install `new` over `current`: `Ok(false)` when it is the
/// same release.
fn worth_installing(
    current: Option<Version>,
    new: Version,
    allow_older: bool,
) -> Result<bool, String> {
    match current {
        Some(c) if c == new => Ok(false),
        Some(c) if new < c && !allow_older => Err(format!(
            "v{new} is older than this tiles, {c}; pass --allow-older to install it"
        )),
        _ => Ok(true),
    }
}

/// What `--check` says of `latest` against this binary's version, for a
/// `tiles` the package installed when `packaged`.
fn check_line(current: Option<Version>, latest: Version, repo: &str, packaged: bool) -> String {
    match current {
        Some(c) if c == latest => format!("tiles {c} is the latest release of {repo}"),
        Some(c) if c > latest => {
            format!("tiles {c} is newer than the latest release of {repo}, v{latest}")
        }
        _ => available(latest, packaged),
    }
}

/// That `latest` is out, and how to install it: the next package for a
/// `tiles` the package installed, else `tiles update`.
fn available(latest: Version, packaged: bool) -> String {
    if packaged {
        format!(
            "Tiles v{latest} is available; download tiles-v{latest}-macos.pkg from its release and open it"
        )
    } else {
        format!("Tiles v{latest} is available; run tiles update")
    }
}

/// Where a release comes from.
enum Source<'a> {
    /// Files downloaded by hand.
    ByHand(ByHand),
    /// `gh`, signed in.
    Gh(&'a Path),
    /// Neither, and why `gh` cannot be used.
    Neither(String),
}

impl<'a> Source<'a> {
    fn find(cx: &'a Context<'_>, repo: &str) -> Result<Self, String> {
        if let Some(hand) = by_hand(cx.env)? {
            return Ok(Self::ByHand(hand));
        }
        match cx.gh.as_deref() {
            Some(gh) if gh_signed_in(gh) => Ok(Self::Gh(gh)),
            Some(_) => Ok(Self::Neither(format!(
                "gh is not signed in as an account that can read {repo}"
            ))),
            None => Ok(Self::Neither("gh is not on PATH".into())),
        }
    }

    /// The latest release, for `--check`; without `gh`, by the anonymous
    /// metadata `GET`.
    fn latest(&self, cx: &Context<'_>, repo: &str) -> Result<Version, String> {
        match self {
            Self::ByHand(hand) => version_of_archive(&hand.archive, cx.target),
            Self::Gh(gh) => latest_by_gh(gh, repo),
            Self::Neither(why) => (cx.latest)(repo).map_err(|e| {
                format!("cannot read the latest release of {repo}: {e}; {why}, and GitHub answers an anonymous request only for a public repository")
            }),
        }
    }

    /// The release to install: the one named, the archive's, or the latest.
    fn release(
        &self,
        tag: Option<Version>,
        cx: &Context<'_>,
        repo: &str,
    ) -> Result<Version, String> {
        match self {
            Self::ByHand(hand) => {
                let v = version_of_archive(&hand.archive, cx.target)?;
                match tag {
                    Some(t) if t != v => {
                        Err(format!("TILES_ARCHIVE holds v{v}, not the v{t} named"))
                    }
                    _ => Ok(v),
                }
            }
            Self::Gh(gh) => tag.map_or_else(|| latest_by_gh(gh, repo), Ok),
            Self::Neither(why) => Err(steps(repo, cx.target, tag, why)),
        }
    }

    /// The archive and `SHA256SUMS` of `version`, as bytes; a by-hand
    /// download loses its quarantine flag first.
    fn fetch(
        &self,
        work: &Path,
        repo: &str,
        version: Version,
        name: &str,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let (archive, sums) = match self {
            Self::ByHand(hand) => {
                strip_quarantine(&hand.archive)?;
                strip_quarantine(&hand.sums)?;
                (hand.archive.clone(), hand.sums.clone())
            }
            Self::Gh(gh) => {
                let dir = work.join("download");
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&dir)
                    .map_err(|e| format!("cannot make a directory to download into: {e}"))?;
                let tag = format!("v{version}");
                let into = dir.display().to_string();
                gh_run(
                    gh,
                    &[
                        "release",
                        "download",
                        &tag,
                        "--repo",
                        repo,
                        "--dir",
                        &into,
                        "--pattern",
                        name,
                        "--pattern",
                        "SHA256SUMS",
                    ],
                )?;
                (dir.join(name), dir.join("SHA256SUMS"))
            }
            Self::Neither(_) => return Err("no release to fetch".into()),
        };
        Ok((
            read_capped(&archive, MAX_ARCHIVE_BYTES)?,
            read_capped(&sums, MAX_SUMS_BYTES)?,
        ))
    }
}

/// Runs `tiles update` and returns the lines it prints.
///
/// # Errors
///
/// Why nothing was replaced, on one line or, for a download by hand, the
/// steps on several.
pub fn update(args: &Args, cx: &Context<'_>) -> Result<Vec<String>, String> {
    let repo = repo(cx.env)?;
    let current = Version::parse(cx.current);
    let source = Source::find(cx, &repo)?;
    if args.check {
        let packaged = packaged(
            bin_dir_set(cx.env),
            cx.running.as_deref(),
            Path::new(PACKAGE_BIN),
        );
        return Ok(vec![check_line(
            current,
            source.latest(cx, &repo)?,
            &repo,
            packaged,
        )]);
    }
    let new = source.release(args.tag, cx, &repo)?;
    if !worth_installing(current, new, args.allow_older)? {
        return Ok(vec![format!(
            "tiles {new} is already release v{new}; nothing to update"
        )]);
    }
    let work = WorkDir::new(&cx.work)?;
    let name = archive_name(new, cx.target);
    let (archive, sums) = source.fetch(&work.0, &repo, new, &name)?;
    if sha256(&archive) != expected_sum(&String::from_utf8_lossy(&sums), &name)? {
        return Err(format!(
            "{name} does not match SHA256SUMS; nothing was replaced"
        ));
    }
    let binary = unpack(&work.0, &archive, &name)?;
    strip_quarantine(&binary)?;
    let beside = Beside::write(&cx.dest, &read_capped(&binary, MAX_ARCHIVE_BYTES)?)?;
    let says = crate::opencode::ask(beside.path(), "--version")
        .and_then(|a| crate::opencode::version_of(&a))
        .and_then(|v| Version::parse(&v));
    if says != Some(new) {
        return Err(format!(
            "the new binary's --version does not say {new}; nothing was replaced"
        ));
    }
    beside.rename_over(&cx.dest)?;
    Ok(updated(cx, current, new))
}

/// The lines after an update: the old and new versions, and where the
/// installed `tiles` and `PATH` disagree.
fn updated(cx: &Context<'_>, current: Option<Version>, new: Version) -> Vec<String> {
    let shown = |p: &Path| sanitize::line(&p.display().to_string(), 200);
    let mut out = vec![format!(
        "updated tiles from {} to {new} at {}",
        current.map_or_else(|| sanitize::line(cx.current, 40), |c| c.to_string()),
        shown(&cx.dest)
    )];
    let dir = cx.dest.parent().unwrap_or(Path::new("."));
    let on_path = (cx.env)("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == dir));
    if !on_path {
        out.push(format!(
            "add {0} to PATH, for example: echo 'export PATH=\"{0}:$PATH\"' >> ~/.zshrc",
            shown(dir)
        ));
    }
    let same = |a: &Path, b: &Path| match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    if let Some(running) = cx.running.as_deref().filter(|r| !same(r, &cx.dest)) {
        out.push(format!(
            "this tiles runs from {}, which still holds {}",
            shown(running),
            sanitize::line(cx.current, 40)
        ));
    }
    out
}

fn version_of_archive(archive: &Path, target: &str) -> Result<Version, String> {
    let file = archive
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or_default();
    archive_version(file, target).ok_or_else(|| {
        format!(
            "TILES_ARCHIVE names {}, not a tiles-vX.Y.Z-{target}.tar.gz",
            sanitize::line(file, 80)
        )
    })
}

/// Unpacks the archive's `tiles` into `work` and returns its path.
fn unpack(work: &Path, archive: &[u8], name: &str) -> Result<PathBuf, String> {
    let verified = work.join("verified.tar.gz");
    fs::File::options()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&verified)
        .and_then(|mut f| f.write_all(archive))
        .map_err(|e| format!("cannot write the archive to unpack: {e}"))?;
    let out = work.join("unpacked");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&out)
        .map_err(|e| format!("cannot make a directory to unpack in: {e}"))?;
    let member = format!("{}/tiles", name.trim_end_matches(".tar.gz"));
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&verified)
        .arg("-C")
        .arg(&out)
        .arg(&member)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("cannot run tar: {e}"))?;
    let binary = out.join(&member);
    let is_file = fs::symlink_metadata(&binary).is_ok_and(|m| m.is_file());
    if !status.success() || !is_file {
        return Err(format!("{name} holds no {member}; nothing was replaced"));
    }
    Ok(binary)
}

/// Where the macOS package installs `tiles` (ADR-035).
pub const PACKAGE_BIN: &str = "/usr/local/bin/tiles";

/// Whether `TILES_BIN_DIR` names the folder `tiles update` writes.
fn bin_dir_set(env: &dyn Fn(&str) -> Option<String>) -> bool {
    env("TILES_BIN_DIR").is_some_and(|d| !d.is_empty())
}

/// Whether `running` is the `tiles` the macOS package installed at
/// `package`, which `tiles update` leaves to the next package unless
/// `TILES_BIN_DIR` is set.
#[must_use]
pub fn packaged(bin_dir_set: bool, running: Option<&Path>, package: &Path) -> bool {
    let canonical = |p: &Path| fs::canonicalize(p).ok();
    !bin_dir_set
        && running
            .and_then(canonical)
            .is_some_and(|r| Some(r) == canonical(package))
}

/// [`packaged`] for the `tiles` running now.
#[must_use]
pub fn packaged_here(env: &dyn Fn(&str) -> Option<String>) -> bool {
    packaged(
        bin_dir_set(env),
        std::env::current_exe().ok().as_deref(),
        Path::new(PACKAGE_BIN),
    )
}

/// Why `tiles update` leaves a `tiles` the macOS package installed at
/// `package`: it would write a second copy to `~/.local/bin`, and replacing
/// the package's own needs an administrator. `None` for `--check`, with
/// `TILES_BIN_DIR` set, or for any other `tiles`.
#[must_use]
pub fn from_package(
    args: &Args,
    bin_dir_set: bool,
    running: Option<&Path>,
    package: &Path,
) -> Option<String> {
    (!args.check && packaged(bin_dir_set, running, package)).then(|| {
        format!(
            "this tiles came from the Tiles package in {}; to update it, download tiles-<tag>-macos.pkg from the latest release and open it. tiles update --check says whether one is newer",
            package.display()
        )
    })
}

/// Runs `tiles update` with this machine's `PATH`, home and binary.
pub fn main(args: &[String]) -> ExitCode {
    let args = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("tiles: {}", sanitize::line(&e, 400));
            return ExitCode::from(2);
        }
    };
    let env = |k: &str| std::env::var(k).ok();
    let running = std::env::current_exe().ok();
    if let Some(why) = from_package(
        &args,
        bin_dir_set(&env),
        running.as_deref(),
        Path::new(PACKAGE_BIN),
    ) {
        eprintln!("tiles: {why}");
        return ExitCode::FAILURE;
    }
    let target = match target() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tiles: {e}");
            return ExitCode::FAILURE;
        }
    };
    let bin_dir = env("TILES_BIN_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".local/bin")));
    let Some(bin_dir) = bin_dir else {
        eprintln!("tiles: HOME is not set; set TILES_BIN_DIR to the folder that holds tiles");
        return ExitCode::FAILURE;
    };
    let gh = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join("gh"))
            .find(|p| p.is_file())
    });
    let cx = Context {
        env: &env,
        gh,
        current: tiles_intent::TILES_VERSION,
        target,
        dest: bin_dir.join("tiles"),
        running: std::env::current_exe().ok(),
        work: std::env::temp_dir(),
        latest: &fetch_latest,
    };
    match update(&args, &cx) {
        Ok(lines) => {
            for l in lines {
                println!("{}", sanitize::line(&l, 600));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            for (i, l) in e.lines().enumerate() {
                let prefix = if i == 0 { "tiles: " } else { "" };
                let indent = &l[..l.len() - l.trim_start_matches(' ').len()];
                eprintln!("{prefix}{indent}{}", sanitize::line(l, 600));
            }
            ExitCode::FAILURE
        }
    }
}

/// The check's memory: when it last ran and the newest release it saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    /// Seconds since the epoch of the last check.
    pub checked: Option<u64>,
    /// The newest release seen.
    pub latest: Option<Version>,
}

/// The state file: `$TILES_STATE_DIR/update-check.json`, else
/// `$XDG_STATE_HOME/tiles/` or `~/.local/state/tiles/`. Never in the card
/// cache, which `tiles cache clear` empties.
#[must_use]
pub fn state_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = set(ENV_STATE_DIR)
        .or_else(|| set("XDG_STATE_HOME").map(|d| d.join("tiles")))
        .or_else(|| set("HOME").map(|h| h.join(".local/state/tiles")))?;
    Some(dir.join(STATE_FILE))
}

/// The state in `path`; empty when it is missing or unreadable.
#[must_use]
pub fn read_state(path: &Path) -> State {
    let Ok(bytes) = read_capped(path, MAX_STATE_BYTES) else {
        return State::default();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return State::default();
    };
    State {
        checked: json.get("checked").and_then(serde_json::Value::as_u64),
        latest: json
            .get("latest")
            .and_then(serde_json::Value::as_str)
            .and_then(Version::parse),
    }
}

/// Writes `state` owner-only through a fresh temporary file, renamed over,
/// so a planted symlink is replaced, never followed.
fn write_state(path: &Path, state: &State) -> Result<(), String> {
    let dir = path.parent().ok_or("no state directory")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| e.to_string())?;
    let json = serde_json::json!({
        "checked": state.checked,
        "latest": state.latest.map(|v| v.to_string()),
    });
    let tmp = dir.join(format!(
        ".{STATE_FILE}.{}.{}",
        std::process::id(),
        MADE.fetch_add(1, Ordering::Relaxed)
    ));
    let written = fs::File::options()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(json.to_string().as_bytes()))
        .and_then(|()| fs::rename(&tmp, path));
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    Ok(())
}

/// Whether the daily check is on: [`ENV_CHECK`] is unset or anything but
/// `off`, `0`, `false` or `no`, in any case.
#[must_use]
pub fn enabled(env: &dyn Fn(&str) -> Option<String>) -> bool {
    env(ENV_CHECK).is_none_or(|v| {
        !["off", "0", "false", "no"]
            .iter()
            .any(|off| v.trim().eq_ignore_ascii_case(off))
    })
}

/// Whether a day has passed since the last check, or the clock went back.
#[must_use]
pub fn due(state: &State, now: u64) -> bool {
    state
        .checked
        .is_none_or(|t| now < t || now - t >= CHECK_EVERY)
}

/// The URL of the releases location's latest-release metadata.
#[must_use]
pub fn metadata_url(repo: &str) -> String {
    format!("https://api.github.com/repos/{repo}/releases/latest")
}

/// The release version in latest-release metadata: its `tag_name`.
#[must_use]
pub fn latest_from_metadata(body: &[u8]) -> Option<Version> {
    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    Version::parse(json.get("tag_name")?.as_str()?)
}

/// The latest release of `repo`, by one anonymous `GET` of its metadata
/// with `tiles/<version>` as the user agent and no other identifying
/// header, within [`CHECK_DEADLINE`].
///
/// # Errors
///
/// What failed, in a few words.
pub fn fetch_latest(repo: &str) -> Result<Version, String> {
    let client = tiles_net::Client::new(&format!("tiles/{}", tiles_intent::TILES_VERSION), false);
    let url = metadata_url(repo);
    let reply = client
        .send(
            &tiles_net::Request {
                endpoint: (),
                url: &url,
                headers: &[],
                body: None,
                cap: MAX_METADATA_BYTES,
                poll_status: None,
            },
            Instant::now() + CHECK_DEADLINE,
        )
        .map_err(|f| match f {
            tiles_net::Failure::Status { status, .. } => format!("GitHub answered {status}"),
            tiles_net::Failure::Transport { kind, .. } => format!("the request failed ({kind:?})"),
            tiles_net::Failure::Redirect { .. } => "GitHub answered with a redirect".into(),
            tiles_net::Failure::Deadline { .. } => "GitHub did not answer in time".into(),
        })?;
    latest_from_metadata(&reply.body).ok_or_else(|| "the metadata names no vX.Y.Z tag".into())
}

/// The state after the daily check, which runs when it is on, this is a
/// release build, a day has passed and the state file can be written; the
/// timestamp is written first, so a check that dies still counts. Any
/// failure keeps the newest release seen before. `None` when the check is
/// off, [`ENV_REPO`] is not `<owner>/<repo>`, or there is nowhere to keep
/// its state.
#[must_use]
pub fn refresh(
    env: &dyn Fn(&str) -> Option<String>,
    current: &str,
    now: u64,
    fetch: &dyn Fn(&str) -> Result<Version, String>,
) -> Option<State> {
    if !enabled(env) || Version::parse(current).is_none_or(Version::is_development) {
        return None;
    }
    let repo = repo(env).ok()?;
    let path = state_path(env)?;
    let mut state = read_state(&path);
    if due(&state, now) {
        state.checked = Some(now);
        write_state(&path, &state).ok()?;
        if let Ok(latest) = fetch(&repo) {
            state.latest = Some(latest);
            let _ = write_state(&path, &state);
        }
    }
    Some(state)
}

/// The launch notice, when the newest release seen is newer than `current`,
/// for a `tiles` the package installed when `packaged`.
#[must_use]
pub fn notice(current: &str, state: &State, packaged: bool) -> Option<String> {
    let current = Version::parse(current)?;
    let latest = state.latest.filter(|l| *l > current)?;
    Some(available(latest, packaged))
}

/// Seconds since the epoch now.
#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The daily check run in a thread, for `tiles run` to read before the
/// harness starts: the notice, or `None` when there is none or the check
/// did not finish within [`CHECK_DEADLINE`] and a little more.
pub struct Pending(std::sync::mpsc::Receiver<Option<String>>, Instant);

impl Pending {
    /// Starts the check.
    #[must_use]
    pub fn start() -> Self {
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let env = |k: &str| std::env::var(k).ok();
            let current = tiles_intent::TILES_VERSION;
            let state = refresh(&env, current, now(), &fetch_latest);
            let _ = send.send(state.and_then(|s| notice(current, &s, packaged_here(&env))));
        });
        Self(receive, Instant::now())
    }

    /// The notice, waiting at most until the check's deadline.
    #[must_use]
    pub fn notice(self) -> Option<String> {
        let wait = (CHECK_DEADLINE + Duration::from_millis(250)).saturating_sub(self.1.elapsed());
        self.0.recv_timeout(wait).ok().flatten()
    }
}

/// `tiles doctor`'s line on the check: the setting as read, and how long
/// before `now` it last ran.
#[must_use]
pub fn doctor_line(
    env: &dyn Fn(&str) -> Option<String>,
    current: &str,
    state: Option<&State>,
    now: u64,
) -> String {
    let repo = match repo(env) {
        Ok(r) => r,
        Err(e) => return format!("update check: {e}, so it does not run"),
    };
    let setting = env(ENV_CHECK).map_or_else(
        || format!("{ENV_CHECK} unset"),
        |v| format!("{ENV_CHECK}={}", sanitize::line(&v, 40)),
    );
    if !enabled(env) {
        return format!(
            "update check: off ({setting}); tiles update still installs a release from {repo}"
        );
    }
    let on = format!("on ({setting}; off, 0, false or no turns it off)");
    if Version::parse(current).is_none_or(Version::is_development) {
        return format!(
            "update check: none for a build that carries no release tag ({})",
            sanitize::line(current, 40)
        );
    }
    let Some(state) = state else {
        return format!(
            "update check: {on}, but there is no HOME or {ENV_STATE_DIR} to keep its timestamp in, so it does not run"
        );
    };
    let seen = state
        .latest
        .map_or_else(|| "none yet".to_string(), |v| format!("v{v}"));
    match state.checked {
        None => format!(
            "update check: {on}, not run yet; at most once a day it asks api.github.com for {repo}'s latest release"
        ),
        Some(t) => format!(
            "update check: {on}, last run {}, latest release of {repo} seen {seen}",
            ago(t, now)
        ),
    }
}

/// How long before `now` the time `then` was: minutes under an hour, hours
/// under a day, else days.
fn ago(then: u64, now: u64) -> String {
    let secs = now.saturating_sub(then);
    if secs < 3_600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        format!("{} h ago", secs / 3_600)
    } else {
        format!("{} d ago", secs / 86_400)
    }
}

#[cfg(test)]
mod tests;
