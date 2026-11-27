use std::fs;
use std::path::{Path, PathBuf};

use super::{
    Edit, FISH, Kind, Places, SCRIPT, Undone, apply, block, plan, status_line, steps_aside, undo,
};
use crate::harness::Harness;
use crate::install::Dirs;

/// A home of its own under the temporary directory.
fn home(name: &str) -> (PathBuf, Places) {
    let root = std::env::temp_dir().join(format!("tiles-shell-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let dirs = Dirs {
        home: root.clone(),
        config_home: root.join(".config"),
        codex_home: root.join(".codex"),
    };
    let places = Places::new(&dirs, &|_| None);
    (root, places)
}

fn shell_is(name: &'static str) -> impl Fn(&str) -> Option<String> {
    move |k: &str| (k == "SHELL").then(|| format!("/bin/{name}"))
}

#[test]
fn places_follow_the_tiles_config_dir_and_zdotdir() {
    let dirs = Dirs {
        home: "/h".into(),
        config_home: "/h/.config".into(),
        codex_home: "/h/.codex".into(),
    };
    let p = Places::new(&dirs, &|k| match k {
        "HOME" => Some("/h".into()),
        "ZDOTDIR" => Some("/z".into()),
        _ => None,
    });
    assert_eq!(p.script, Path::new("/h/.config/tiles/shell.sh"));
    assert_eq!(p.zshrc, Path::new("/z/.zshrc"));
    assert_eq!(p.fish, Path::new("/h/.config/fish/conf.d/tiles.fish"));
    let moved = Places::new(&dirs, &|k| (k == "TILES_CONFIG_DIR").then(|| "/t".into()));
    assert_eq!(moved.script, Path::new("/t/shell.sh"));
    assert_eq!(moved.zshrc, Path::new("/h/.zshrc"));
}

#[test]
fn the_block_reads_the_script_only_when_it_is_there_and_quotes_its_path() {
    let b = block(Path::new("/Users/o'neil/.config/tiles/shell.sh"));
    assert_eq!(
        b,
        "# Added by tiles setup: plain claude, codex, opencode and pi draw cards; tiles shell off removes it\n\
         if [ -r '/Users/o'\\''neil/.config/tiles/shell.sh' ]; then . '/Users/o'\\''neil/.config/tiles/shell.sh'; fi\n"
    );
    assert_eq!(b.lines().count(), 2);
}

#[test]
fn no_shell_found_means_no_plan() {
    let (root, places) = home("none");
    assert_eq!(plan(&places, &|_| None), None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn zsh_as_the_login_shell_gets_its_zshrc_and_the_script() {
    let (root, places) = home("zsh");
    let p = plan(&places, &shell_is("zsh")).unwrap();
    assert_eq!(p.shells, [Kind::Zsh]);
    assert_eq!(p.files, std::slice::from_ref(&places.zshrc));
    assert_eq!(
        p.edits,
        [
            Edit::Write(places.script.clone(), SCRIPT),
            Edit::Append(places.zshrc.clone(), block(&places.script)),
        ]
    );
    assert_eq!(
        p.edits.iter().map(Edit::describe).collect::<Vec<_>>(),
        [
            format!("write {}", places.script.display()),
            format!("add 2 lines to the end of {}", places.zshrc.display()),
        ]
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bash_gets_each_startup_file_that_exists_or_its_bashrc() {
    let (root, places) = home("bash");
    let p = plan(&places, &shell_is("bash")).unwrap();
    assert_eq!(p.files, [places.bash[0].clone()], "no file: .bashrc");
    fs::write(&places.bash[1], "export A=1\n").unwrap();
    let p = plan(&places, &shell_is("bash")).unwrap();
    assert_eq!(p.files, [places.bash[1].clone()], "only .bash_profile");
    fs::write(&places.bash[0], "").unwrap();
    let p = plan(&places, &shell_is("zsh")).unwrap();
    assert_eq!(
        p.shells,
        [Kind::Zsh, Kind::Bash],
        "a bash file wires bash too"
    );
    assert_eq!(
        p.files,
        [
            places.zshrc.clone(),
            places.bash[0].clone(),
            places.bash[1].clone()
        ]
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn fish_gets_a_file_of_tiles_own_and_no_startup_file_changes() {
    let (root, places) = home("fish");
    let p = plan(&places, &shell_is("fish")).unwrap();
    assert_eq!(p.shells, [Kind::Fish]);
    assert_eq!(p.edits, [Edit::Write(places.fish.clone(), FISH)]);
    apply(&p.edits).unwrap();
    assert_eq!(fs::read_to_string(&places.fish).unwrap(), FISH);
    assert_eq!(
        plan(&places, &shell_is("fish")).unwrap().edits,
        Vec::<Edit>::new()
    );
    let _ = fs::remove_dir_all(root);
}

/// Wires zsh over `before` (`None`: no `.zshrc`), checks a second plan has
/// nothing to do, then undoes it and returns the `.zshrc` left.
fn round_trip(name: &str, before: Option<&[u8]>) -> Option<Vec<u8>> {
    let (root, places) = home(name);
    if let Some(bytes) = before {
        fs::write(&places.zshrc, bytes).unwrap();
    }
    let p = plan(&places, &shell_is("zsh")).unwrap();
    apply(&p.edits).unwrap();
    let wired = fs::read_to_string(&places.zshrc).unwrap();
    assert!(wired.ends_with(&block(&places.script)), "{wired}");
    assert_eq!(fs::read_to_string(&places.script).unwrap(), SCRIPT);
    let again = plan(&places, &shell_is("zsh")).unwrap();
    assert!(again.edits.is_empty(), "{again:?}");
    assert!(status_line(Some(&again), &|_| None).starts_with("shell: wired for zsh through "));
    let done = undo(&places, false).unwrap();
    assert!(done.contains(&Undone::Deleted(places.script.clone())));
    assert!(!places.script.exists());
    let left = fs::read(&places.zshrc).ok();
    let _ = fs::remove_dir_all(root);
    left
}

#[test]
fn undo_restores_a_startup_file_byte_for_byte() {
    for (name, before) in [
        ("newline", &b"export PATH=/x:$PATH\n"[..]),
        ("no-newline", &b"alias ll='ls -l'"[..]),
        ("empty", &b""[..]),
        ("blank-lines", &b"a\n\n\n"[..]),
        ("crlf", &b"a\r\nb\r\n"[..]),
    ] {
        assert_eq!(
            round_trip(name, Some(before)).as_deref(),
            Some(before),
            "{name}"
        );
    }
    assert_eq!(round_trip("missing", None), None, "a file setup made goes");
}

#[test]
fn undo_keeps_lines_the_user_added_after_the_block() {
    let (root, places) = home("after");
    fs::write(&places.zshrc, "a\n").unwrap();
    apply(&plan(&places, &shell_is("zsh")).unwrap().edits).unwrap();
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(&places.zshrc)
        .unwrap();
    std::io::Write::write_all(&mut f, b"b\n").unwrap();
    assert_eq!(
        undo(&places, false).unwrap()[0],
        Undone::Removed(places.zshrc.clone())
    );
    assert_eq!(fs::read_to_string(&places.zshrc).unwrap(), "a\nb\n");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_linked_startup_file_stays_a_link() {
    let (root, places) = home("link");
    let real = root.join("dotfiles-zshrc");
    fs::write(&real, "a\n").unwrap();
    std::os::unix::fs::symlink(&real, &places.zshrc).unwrap();
    apply(&plan(&places, &shell_is("zsh")).unwrap().edits).unwrap();
    assert!(fs::symlink_metadata(&places.zshrc).unwrap().is_symlink());
    assert!(fs::read_to_string(&real).unwrap().contains("tiles setup"));
    undo(&places, false).unwrap();
    assert!(fs::symlink_metadata(&places.zshrc).unwrap().is_symlink());
    assert_eq!(fs::read_to_string(&real).unwrap(), "a\n");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn changed_tiles_lines_are_left_by_setup_and_by_undo() {
    let (root, places) = home("changed");
    let changed = "a\n# Added by tiles setup: mine now\nsource ~/elsewhere.sh\n";
    fs::write(&places.zshrc, changed).unwrap();
    let p = plan(&places, &shell_is("zsh")).unwrap();
    assert_eq!(p.left, std::slice::from_ref(&places.zshrc));
    assert!(
        p.edits.is_empty(),
        "nothing reads the script, so it is not written"
    );
    assert!(
        status_line(Some(&p), &|_| None).contains("holds changed Tiles lines"),
        "{p:?}"
    );
    assert_eq!(
        undo(&places, false).unwrap(),
        [Undone::Left(places.zshrc.clone())]
    );
    assert_eq!(fs::read_to_string(&places.zshrc).unwrap(), changed);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_dry_run_undo_changes_nothing() {
    let (root, places) = home("dry");
    fs::write(&places.zshrc, "a\n").unwrap();
    apply(&plan(&places, &shell_is("zsh")).unwrap().edits).unwrap();
    let wired = fs::read(&places.zshrc).unwrap();
    let said = undo(&places, true).unwrap();
    assert_eq!(
        said[0].describe(true),
        format!(
            "would remove the Tiles lines from {}",
            places.zshrc.display()
        )
    );
    assert_eq!(fs::read(&places.zshrc).unwrap(), wired);
    assert!(places.script.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_status_line_says_what_is_wired_and_what_is_not() {
    let (root, places) = home("status");
    let p = plan(&places, &shell_is("zsh")).unwrap();
    assert_eq!(
        status_line(Some(&p), &|_| None),
        "shell: not wired for zsh: plain sessions get no inline card; run tiles setup"
    );
    apply(&p.edits).unwrap();
    let p = plan(&places, &shell_is("zsh")).unwrap();
    let plain = |k: &str| (k == "TILES_PLAIN").then(|| "1".to_string());
    assert!(
        status_line(Some(&p), &plain)
            .ends_with("; TILES_PLAIN is set, so this shell keeps them plain")
    );
    assert!(status_line(None, &|_| None).starts_with("shell: no zsh, bash or fish found"));
    let _ = fs::remove_dir_all(root);
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

#[test]
fn an_interactive_session_goes_through_tiles() {
    for (h, args) in [
        (Harness::Claude, ""),
        (Harness::Claude, "--resume"),
        (Harness::Claude, "-c --model opus"),
        (Harness::Claude, "fix the tests"),
        (Harness::Codex, ""),
        (Harness::Codex, "resume --last"),
        (Harness::Codex, "fork"),
        (Harness::Codex, "-m gpt-5 explain"),
        (Harness::Codex, "-c model=o3"),
        (Harness::OpenCode, ""),
        (Harness::OpenCode, "./project"),
        (Harness::OpenCode, "-c"),
        (Harness::Pi, ""),
        (Harness::Pi, "-c"),
        (Harness::Pi, "--mode text"),
        (Harness::Pi, "explain"),
        (Harness::Pi, "-- -p"),
    ] {
        assert!(!steps_aside(h, &words(args)), "{} {args}", h.name());
    }
    // A quoted prompt is one argument, never a subcommand.
    for h in Harness::ALL {
        assert!(
            !steps_aside(h, &["list the mcp servers".to_string()]),
            "{}",
            h.name()
        );
    }
}

#[test]
fn one_shot_scripted_and_management_uses_start_the_harness_untouched() {
    for (h, args) in [
        (Harness::Claude, "-p hi"),
        (Harness::Claude, "--print hi"),
        (Harness::Claude, "--output-format=json -p hi"),
        (Harness::Claude, "mcp list"),
        (Harness::Claude, "update"),
        (Harness::Claude, "--version"),
        (Harness::Claude, "-v"),
        (Harness::Claude, "--help"),
        (Harness::Codex, "exec fix it"),
        (Harness::Codex, "e fix it"),
        (Harness::Codex, "mcp add x"),
        (Harness::Codex, "login"),
        (Harness::Codex, "-V"),
        (Harness::OpenCode, "run hello"),
        (Harness::OpenCode, "serve"),
        (Harness::OpenCode, "auth login"),
        (Harness::OpenCode, "-v"),
        (Harness::Pi, "-p hi"),
        (Harness::Pi, "--mode json"),
        (Harness::Pi, "--mode=rpc"),
        (Harness::Pi, "install npm:x"),
        (Harness::Pi, "--list-models"),
        (Harness::Pi, "--export out.html"),
    ] {
        assert!(steps_aside(h, &words(args)), "{} {args}", h.name());
    }
}

#[test]
fn both_scripts_define_one_function_per_harness_and_respect_the_switches() {
    for h in Harness::ALL {
        assert!(SCRIPT.contains(&format!(
            "function {} {{ __tiles_start {} \"$@\"; }}",
            h.name(),
            h.name()
        )));
        assert!(FISH.contains(&format!(
            "function {}\n    __tiles_start {} $argv\nend",
            h.name(),
            h.name()
        )));
    }
    for script in [SCRIPT, FISH] {
        assert!(script.starts_with(super::MARK));
        for needed in ["TILES_PLAIN", "TILES_RUN", "tiles run --from-shell"] {
            assert!(script.contains(needed), "{needed}");
        }
    }
}
