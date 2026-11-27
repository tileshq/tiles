//! The shell wiring (ADR-069): after `tiles setup`, a `claude`, `codex`,
//! `opencode` or `pi` typed at an interactive zsh, bash or fish prompt starts
//! through `tiles run`, so the card draws inline without a per-session
//! command.
//!
//! For zsh and bash, setup writes Tiles' own [`script`] to `shell.sh` in the
//! Tiles config directory and appends two lines ([`block`]) to each startup
//! file it wires ([`startup_files`]). For fish it writes [`fish`] to
//! `fish/conf.d/tiles.fish`, which fish reads by itself, so no file of the
//! user's changes. Each script records the `tiles` that ran setup, as
//! installed ([`installed`]), and defines one function per harness that runs
//! `<that tiles> run --from-shell <harness>` when standard input and output
//! are a terminal, that file is executable and no `tiles run` is already
//! around the shell, and the harness itself otherwise. `tiles run
//! --from-shell` starts the harness untouched for the uses [`steps_aside`]
//! names, and when Tiles fails before the harness starts; a harness not on
//! `PATH` gets the shell's own `command not found` line and status 127.
//!
//! [`undo`], behind `tiles shell off`, removes exactly the bytes setup
//! appended, deletes a startup file setup created, and deletes Tiles' own
//! scripts.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use crate::harness::Harness;
use crate::install::Dirs;

/// Usage of `tiles shell`.
pub const USAGE: &str = "tiles shell off [--dry-run]";

/// The first line of every block setup appends, and of each Tiles script.
const MARK: &str = "# Added by tiles setup:";

/// What starts the line of the zsh and bash script that records `tiles`.
const SH_TILES: &str = "\n  local tiles=";

/// What starts the line of the fish script that records `tiles`.
const FISH_TILES: &str = "\n    set -l tiles ";

/// `p` as one single-quoted zsh or bash word.
fn sh_word(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "'\\''"))
}

/// `p` as one single-quoted fish word.
fn fish_word(p: &Path) -> String {
    format!(
        "'{}'",
        p.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
    )
}

/// The zsh and bash script, `shell.sh` in the Tiles config directory, which
/// runs `tiles`. `function name {` keeps an alias of the same name, which a
/// shell expands before a `name()` definition, from breaking the file.
#[must_use]
pub fn script(tiles: &Path) -> String {
    format!(
        r#"{MARK} claude, codex, opencode and pi typed at an interactive
# prompt start through `tiles run`, which draws chart cards in this terminal.
# `command claude` skips Tiles once; `tiles shell off` removes this file and
# the lines that read it.
function __tiles_start {{{SH_TILES}{tiles}
  if [ -t 0 ] && [ -t 1 ] && [ -z "${{TILES_RUN-}}" ] && [ -f "$tiles" ] && [ -x "$tiles" ]; then
    "$tiles" run --from-shell "$@"
  else
    command "$@"
  fi
}}
function claude {{ __tiles_start claude "$@"; }}
function codex {{ __tiles_start codex "$@"; }}
function opencode {{ __tiles_start opencode "$@"; }}
function pi {{ __tiles_start pi "$@"; }}
"#,
        tiles = sh_word(tiles)
    )
}

/// The fish script, `fish/conf.d/tiles.fish` in the XDG config directory,
/// which runs `tiles`.
#[must_use]
pub fn fish(tiles: &Path) -> String {
    format!(
        r#"{MARK} claude, codex, opencode and pi typed at an interactive
# prompt start through `tiles run`, which draws chart cards in this terminal.
# `command claude` skips Tiles once; `tiles shell off` removes this file.
function __tiles_start{FISH_TILES}{tiles}
    if isatty stdin; and isatty stdout; and test -z "$TILES_RUN"; and test -f $tiles; and test -x $tiles
        command $tiles run --from-shell $argv
    else
        command $argv
    end
end
function claude
    __tiles_start claude $argv
end
function codex
    __tiles_start codex $argv
end
function opencode
    __tiles_start opencode $argv
end
function pi
    __tiles_start pi $argv
end
"#,
        tiles = fish_word(tiles)
    )
}

/// The `tiles` a script [`script`] or [`fish`] wrote runs, read back from
/// its quoted word.
fn recorded(text: &str) -> Option<PathBuf> {
    if let Some((_, rest)) = text.split_once(SH_TILES) {
        let mut rest = rest.strip_prefix('\'')?;
        let mut out = String::new();
        loop {
            let end = rest.find('\'')?;
            out.push_str(&rest[..end]);
            rest = &rest[end + 1..];
            match rest.strip_prefix("\\''") {
                Some(more) => {
                    out.push('\'');
                    rest = more;
                }
                None => return Some(out.into()),
            }
        }
    }
    let (_, rest) = text.split_once(FISH_TILES)?;
    let mut chars = rest.strip_prefix('\'')?.chars();
    let mut out = String::new();
    loop {
        match chars.next()? {
            '\'' => return Some(out.into()),
            '\\' => match chars.next()? {
                c @ ('\\' | '\'') => out.push(c),
                c => {
                    out.push('\\');
                    out.push(c);
                }
            },
            c => out.push(c),
        }
    }
}

/// Whether `p` is a file the shell can run.
fn runnable(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// `this`, the running `tiles`, as installed: the first `tiles` in `path`, a
/// `PATH` value, that is the same file, such as Homebrew's link in its `bin`
/// rather than the versioned file it points to; else `this` itself.
#[must_use]
pub fn installed(path: &OsStr, this: &Path) -> PathBuf {
    let id = |p: &Path| fs::metadata(p).ok().map(|m| (m.dev(), m.ino()));
    let Some(here) = id(this) else {
        return this.to_path_buf();
    };
    std::env::split_paths(path)
        .filter(|d| d.is_absolute())
        .map(|d| d.join("tiles"))
        .find(|p| id(p) == Some(here))
        .unwrap_or_else(|| this.to_path_buf())
}

/// [`installed`] for the running `tiles` and this process's `PATH`.
#[must_use]
pub fn this_tiles() -> PathBuf {
    installed(
        &std::env::var_os("PATH").unwrap_or_default(),
        &crate::this_tiles(),
    )
}

/// A shell setup can wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// zsh.
    Zsh,
    /// bash.
    Bash,
    /// fish.
    Fish,
}

impl Kind {
    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Kind::Zsh => "zsh",
            Kind::Bash => "bash",
            Kind::Fish => "fish",
        }
    }
}

/// One change setup makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Write Tiles' own script.
    Write(PathBuf, String),
    /// Append [`block`] to a startup file of the user's, creating it when
    /// missing.
    Append(PathBuf, String),
}

impl Edit {
    /// The change in words, for `--verbose` and `--dry-run`.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Edit::Write(path, _) => format!("write {}", path.display()),
            Edit::Append(path, _) => format!("add 2 lines to the end of {}", path.display()),
        }
    }
}

/// Where the shell wiring stands on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The shells it covers, wired now or after [`Plan::edits`].
    pub shells: Vec<Kind>,
    /// The files that read Tiles' script, or will once the edits run.
    pub files: Vec<PathBuf>,
    /// The changes still to make; none when everything is wired.
    pub edits: Vec<Edit>,
    /// Startup files whose Tiles lines were changed by hand, left as they are.
    pub left: Vec<PathBuf>,
    /// The `tiles` Tiles' script on disk runs, `None` when there is none.
    pub recorded: Option<PathBuf>,
}

/// Where the wiring's files are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// Tiles' zsh and bash script.
    pub script: PathBuf,
    /// Tiles' fish script.
    pub fish: PathBuf,
    /// zsh's startup file.
    pub zshrc: PathBuf,
    /// The startup file of a bash that is not a login shell.
    pub bashrc: PathBuf,
    /// The files a login bash reads the first of that exists:
    /// `.bash_profile`, `.bash_login`, `.profile`.
    pub bash_login: [PathBuf; 3],
    /// fish's config directory.
    pub fish_dir: PathBuf,
}

impl Places {
    /// From the environment: the Tiles config directory as
    /// `tiles_mcp::settings::path` finds it, `ZDOTDIR`, and `dirs`.
    #[must_use]
    pub fn new(dirs: &Dirs, env: &dyn Fn(&str) -> Option<String>) -> Self {
        let config = tiles_mcp::settings::path(env)
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| dirs.config_home.join("tiles"));
        let zdotdir = env("ZDOTDIR")
            .filter(|d| !d.is_empty())
            .map_or_else(|| dirs.home.clone(), PathBuf::from);
        Self {
            script: config.join("shell.sh"),
            fish: dirs.config_home.join("fish/conf.d/tiles.fish"),
            zshrc: zdotdir.join(".zshrc"),
            bashrc: dirs.home.join(".bashrc"),
            bash_login: [
                dirs.home.join(".bash_profile"),
                dirs.home.join(".bash_login"),
                dirs.home.join(".profile"),
            ],
            fish_dir: dirs.config_home.join("fish"),
        }
    }

    /// The block for `rc`: guarded by `BASH_VERSION` in `.profile`, which
    /// `sh` reads too.
    fn block(&self, rc: &Path) -> String {
        block(&self.script, rc == self.bash_login[2])
    }
}

/// The two lines appended to a startup file: the mark, and a guarded read
/// of `script`, which leaves the status at 0 when the script is gone. With
/// `bash_only`, only bash reads it.
#[must_use]
pub fn block(script: &Path, bash_only: bool) -> String {
    let quoted = sh_word(script);
    let bash = if bash_only {
        "[ -n \"$BASH_VERSION\" ] && "
    } else {
        ""
    };
    format!(
        "{MARK} plain claude, codex, opencode and pi draw cards; tiles shell off removes it\nif {bash}[ -r {quoted} ]; then . {quoted}; fi\n"
    )
}

/// What a startup file holds of the wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    /// Nothing.
    None,
    /// The block, as setup writes it.
    Block,
    /// A Tiles mark with other lines than setup writes.
    Changed,
}

fn held(text: Option<&str>, block: &str) -> Held {
    match text {
        Some(t) if t.starts_with(block) || t.contains(&format!("\n{block}")) => Held::Block,
        Some(t) if t.contains(MARK) => Held::Changed,
        _ => Held::None,
    }
}

/// The shell `$SHELL` names.
fn login_shell(env: &dyn Fn(&str) -> Option<String>) -> Option<Kind> {
    let shell = env("SHELL")?;
    match Path::new(&shell).file_name()?.to_str()? {
        "zsh" => Some(Kind::Zsh),
        "bash" => Some(Kind::Bash),
        "fish" => Some(Kind::Fish),
        _ => None,
    }
}

/// The startup files to wire for zsh and bash. `.zshrc` when zsh is the
/// login shell or the file exists. bash when it is the login shell or
/// `.bashrc`, `.bash_profile` or `.bash_login` exists: the `.bashrc` that
/// exists, and the login file a login bash reads, as macOS terminals start
/// bash: the first of `.bash_profile`, `.bash_login` and `.profile` that
/// exists, else a new `.bash_profile`.
fn startup_files(places: &Places, login: Option<Kind>) -> Vec<(Kind, PathBuf)> {
    let mut out = Vec::new();
    if login == Some(Kind::Zsh) || places.zshrc.exists() {
        out.push((Kind::Zsh, places.zshrc.clone()));
    }
    let [profile, bash_login, _] = &places.bash_login;
    let bash = login == Some(Kind::Bash)
        || [&places.bashrc, profile, bash_login]
            .iter()
            .any(|p| p.exists());
    if bash {
        if places.bashrc.exists() {
            out.push((Kind::Bash, places.bashrc.clone()));
        }
        let read = places
            .bash_login
            .iter()
            .find(|p| p.exists())
            .unwrap_or(profile);
        out.push((Kind::Bash, read.clone()));
    }
    out
}

/// The plan on this machine for scripts that run `tiles`; `None` when no
/// zsh, bash or fish is found.
#[must_use]
pub fn plan(places: &Places, env: &dyn Fn(&str) -> Option<String>, tiles: &Path) -> Option<Plan> {
    let login = login_shell(env);
    let read = |p: &Path| fs::read_to_string(p).ok();
    let mut out = Plan::default();
    for (kind, rc) in startup_files(places, login) {
        if !out.shells.contains(&kind) {
            out.shells.push(kind);
        }
        let block = places.block(&rc);
        match held(read(&rc).as_deref(), &block) {
            Held::Block => out.files.push(rc),
            Held::None => {
                out.edits.push(Edit::Append(rc.clone(), block));
                out.files.push(rc);
            }
            Held::Changed => out.left.push(rc),
        }
    }
    let written = read(&places.script);
    if !out.files.is_empty() {
        let wanted = script(tiles);
        if written.as_deref() != Some(wanted.as_str()) {
            out.edits
                .insert(0, Edit::Write(places.script.clone(), wanted));
        }
    }
    let fished = read(&places.fish);
    if login == Some(Kind::Fish) || places.fish_dir.is_dir() {
        out.shells.push(Kind::Fish);
        out.files.push(places.fish.clone());
        let wanted = fish(tiles);
        if fished.as_deref() != Some(wanted.as_str()) {
            out.edits.push(Edit::Write(places.fish.clone(), wanted));
        }
    }
    out.recorded = [written, fished].iter().flatten().find_map(|t| recorded(t));
    (!out.shells.is_empty()).then_some(out)
}

/// Makes `edits`: Tiles' scripts are written whole; a startup file gets the
/// block appended after one newline, through the file itself so a link
/// stays a link, or is created holding only the block.
///
/// # Errors
///
/// The first file that could not be written, with why.
pub fn apply(edits: &[Edit]) -> Result<(), String> {
    for edit in edits {
        match edit {
            Edit::Write(path, text) => {
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir)
                        .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
                }
                fs::write(path, text)
                    .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            }
            Edit::Append(path, block) => {
                let exists = path.exists();
                let text = if exists {
                    format!("\n{block}")
                } else {
                    block.clone()
                };
                fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path)
                    .and_then(|mut f| f.write_all(text.as_bytes()))
                    .map_err(|e| format!("cannot add to {}: {e}", path.display()))?;
            }
        }
    }
    Ok(())
}

/// Points each Tiles script that runs another `tiles` at `tiles`, as `tiles
/// update` does after writing `tiles`. Returns the scripts it rewrote.
///
/// # Errors
///
/// The first script that could not be written, with why.
pub fn follow(places: &Places, tiles: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for (path, make) in [
        (&places.script, script as fn(&Path) -> String),
        (&places.fish, fish),
    ] {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if text.starts_with(MARK) && recorded(&text).is_some_and(|r| r != tiles) {
            fs::write(path, make(tiles))
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            out.push(path.clone());
        }
    }
    Ok(out)
}

/// What [`undo`] did, or would do, to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undone {
    /// Removed the block from this startup file.
    Removed(PathBuf),
    /// Deleted this file: a Tiles script, or a startup file setup created.
    Deleted(PathBuf),
    /// Left this startup file: its Tiles lines were changed by hand.
    Left(PathBuf),
}

impl Undone {
    /// The line for `tiles shell off`.
    #[must_use]
    pub fn describe(&self, dry_run: bool) -> String {
        let (done, path) = match (self, dry_run) {
            (Undone::Removed(p), false) => ("removed the Tiles lines from", p),
            (Undone::Removed(p), true) => ("would remove the Tiles lines from", p),
            (Undone::Deleted(p), false) => ("deleted", p),
            (Undone::Deleted(p), true) => ("would delete", p),
            (Undone::Left(p), _) => {
                return format!(
                    "left {}: its Tiles lines were changed; remove them by hand",
                    p.display()
                );
            }
        };
        format!("{done} {}", path.display())
    }
}

/// `text` without the block: the block and the newline before it, the block
/// that starts a file setup created, or `None` for a whole file that is only
/// the block, which setup created and undo deletes.
fn without(text: &str, block: &str) -> Option<String> {
    if text == block {
        return None;
    }
    let appended = format!("\n{block}");
    Some(if text.contains(&appended) {
        text.replacen(&appended, "", 1)
    } else {
        text.replacen(block, "", 1)
    })
}

/// Removes the wiring: the block from each startup file that holds it,
/// leaving every other byte as it was, a startup file that holds only the
/// block, and Tiles' scripts. With `dry_run`, says what it would do.
///
/// # Errors
///
/// The first file that could not be changed, with why.
pub fn undo(places: &Places, dry_run: bool) -> Result<Vec<Undone>, String> {
    let mut out = Vec::new();
    let rcs = [&places.zshrc, &places.bashrc]
        .into_iter()
        .chain(&places.bash_login);
    for rc in rcs {
        let Ok(text) = fs::read_to_string(rc) else {
            continue;
        };
        let block = places.block(rc);
        match held(Some(&text), &block) {
            Held::None => {}
            Held::Changed => out.push(Undone::Left(rc.clone())),
            Held::Block => {
                let kept = without(&text, &block);
                if !dry_run {
                    match &kept {
                        Some(kept) => fs::write(rc, kept),
                        None => fs::remove_file(rc),
                    }
                    .map_err(|e| format!("cannot change {}: {e}", rc.display()))?;
                }
                out.push(if kept.is_some() {
                    Undone::Removed(rc.clone())
                } else {
                    Undone::Deleted(rc.clone())
                });
            }
        }
    }
    for script in [&places.script, &places.fish] {
        let ours = fs::read_to_string(script).is_ok_and(|t| t.starts_with(MARK));
        if ours {
            if !dry_run {
                fs::remove_file(script)
                    .map_err(|e| format!("cannot delete {}: {e}", script.display()))?;
            }
            out.push(Undone::Deleted(script.clone()));
        }
    }
    Ok(out)
}

/// `tiles doctor`'s line: the shells wired and through which files, the
/// `tiles` their script runs when it is missing, or not wired.
#[must_use]
pub fn status_line(plan: Option<&Plan>) -> String {
    let Some(plan) = plan else {
        return "shell: no zsh, bash or fish found; start a session with tiles run <harness>"
            .to_string();
    };
    let names: Vec<&str> = plan.shells.iter().map(|k| k.name()).collect();
    let names = names.join(", ");
    let files: Vec<String> = plan.files.iter().map(|p| p.display().to_string()).collect();
    let files = files.join(", ");
    let typed = "claude, codex, opencode and pi typed in a new terminal";
    let read = !plan.files.is_empty() && plan.edits.iter().all(|e| matches!(e, Edit::Write(..)));
    let mut line = match &plan.recorded {
        Some(tiles) if read && !runnable(tiles) => format!(
            "shell: wired for {names} through {files}, but {} is missing, so {typed} start plain; run tiles setup",
            tiles.display()
        ),
        _ if read && plan.edits.is_empty() => {
            format!("shell: wired for {names} through {files}: {typed} start through tiles run")
        }
        Some(tiles) if read => format!(
            "shell: wired for {names} through {files}: {typed} start through {}; tiles setup rewrites the script for this tiles",
            tiles.display()
        ),
        _ => format!(
            "shell: not wired for {names}: plain sessions get no inline card; run tiles setup"
        ),
    };
    for p in &plan.left {
        let _ = write!(
            line,
            "; {} holds changed Tiles lines, left as they are",
            p.display()
        );
    }
    line
}

/// Claude Code's subcommands (2.1.332's `--help`); none opens a conversation
/// Tiles can join.
const CLAUDE_COMMANDS: &[&str] = &[
    "agents",
    "attach",
    "auth",
    "auto-mode",
    "config",
    "doctor",
    "gateway",
    "import",
    "install",
    "kill",
    "logs",
    "mcp",
    "migrate-installer",
    "plugin",
    "plugins",
    "purge",
    "respawn",
    "rm",
    "setup-token",
    "stop",
    "ultrareview",
    "update",
    "upgrade",
];

/// Codex's subcommands but `resume` and `fork`, which open a conversation
/// (codex-cli 0.172.0's `--help`).
const CODEX_COMMANDS: &[&str] = &[
    "a",
    "agents",
    "app",
    "app-server",
    "apply",
    "archive",
    "cloud",
    "completion",
    "debug",
    "delete",
    "doctor",
    "e",
    "exec",
    "exec-server",
    "features",
    "help",
    "login",
    "logout",
    "mcp",
    "mcp-server",
    "migrate-nfs-db",
    "migrate-rollouts",
    "plugin",
    "queue",
    "remote-control",
    "review",
    "sandbox",
    "unarchive",
    "update",
];

/// `OpenCode`'s subcommands; its default command takes a project directory.
const OPENCODE_COMMANDS: &[&str] = &[
    "acp",
    "agent",
    "attach",
    "auth",
    "completion",
    "db",
    "debug",
    "export",
    "github",
    "import",
    "mcp",
    "models",
    "plug",
    "plugin",
    "pr",
    "providers",
    "run",
    "serve",
    "session",
    "stats",
    "uninstall",
    "upgrade",
    "web",
];

/// Pi's subcommands.
const PI_COMMANDS: &[&str] = &[
    "auth",
    "config",
    "install",
    "list",
    "mcp",
    "remove",
    "uninstall",
    "update",
];

/// Whether `tiles run --from-shell` starts the harness untouched for these
/// arguments: help and version flags, a one-shot or scripted mode, and the
/// subcommands that open no conversation. Only the interactive session gets
/// Tiles, the one `tiles run` was proved on. Arguments after `--` are the
/// conversation's, and a subcommand is the first argument not starting with
/// `-`.
#[must_use]
pub fn steps_aside(harness: Harness, args: &[String]) -> bool {
    let args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .take_while(|a| *a != "--")
        .collect();
    let has = |flags: &[&str]| {
        args.iter().any(|a| {
            flags
                .iter()
                .any(|f| a == f || (f.starts_with("--") && a.starts_with(&format!("{f}="))))
        })
    };
    if has(&["-h", "--help", "-v", "-V", "--version"]) {
        return true;
    }
    let first = args.iter().find(|a| !a.starts_with('-')).copied();
    let command = |names: &[&str]| first.is_some_and(|f| names.contains(&f));
    match harness {
        Harness::Claude => {
            has(&["-p", "--print", "--output-format", "--input-format", "--bg"])
                || command(CLAUDE_COMMANDS)
        }
        Harness::Codex => command(CODEX_COMMANDS),
        Harness::OpenCode => command(OPENCODE_COMMANDS),
        Harness::Pi => {
            let scripted = args.windows(2).any(|w| w[0] == "--mode" && w[1] != "text")
                || args
                    .iter()
                    .any(|a| a.starts_with("--mode=") && *a != "--mode=text");
            scripted || has(&["-p", "--print", "--export", "--list-models"]) || command(PI_COMMANDS)
        }
    }
}

/// Runs `tiles shell off [--dry-run]`: removes the wiring.
pub fn main(args: &[String]) -> std::process::ExitCode {
    use std::process::ExitCode;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let dry_run = match args.as_slice() {
        ["off"] => false,
        ["off", "--dry-run"] => true,
        _ => {
            eprintln!("tiles: usage: {USAGE}");
            return ExitCode::from(2);
        }
    };
    let env = crate::serve::env;
    let Some(dirs) = Dirs::from_env(&env) else {
        eprintln!("tiles: HOME is not set");
        return ExitCode::FAILURE;
    };
    let places = Places::new(&dirs, &env);
    let clean = |s: &str| tiles_term::sanitize::line(s, 600);
    match undo(&places, dry_run) {
        Ok(done) if done.is_empty() => {
            println!("Nothing to remove: no shell is wired.");
            ExitCode::SUCCESS
        }
        Ok(done) => {
            for d in &done {
                println!("{}", clean(&d.describe(dry_run)));
            }
            if !dry_run {
                println!(
                    "Open a new terminal tab: claude, codex, opencode and pi start plain there. tiles setup wires them again."
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("tiles: {}", clean(&e));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
