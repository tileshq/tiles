use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use super::{
    Edit, Found, Kind, Places, Undone, answers_as_pi, apply, block, fish, follow, installed, plan,
    script, status_line, steps_aside, undo,
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

/// What [`super::found`] finds when each of `harnesses` is on `PATH` and a
/// `pi` among them answers as Pi.
fn of(harnesses: &[Harness]) -> Found {
    Found {
        harnesses: harnesses.to_vec(),
        ..Found::default()
    }
}

/// The `tiles` the tests' scripts run.
const TILES: &str = "/opt/homebrew/bin/tiles";

/// [`plan`] for scripts that run [`TILES`], with `name` as the login shell.
fn plan_for(places: &Places, name: &'static str) -> Option<super::Plan> {
    plan(
        places,
        &shell_is(name),
        Path::new(TILES),
        &of(&Harness::ALL),
    )
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
    let b = block(Path::new("/Users/o'neil/.config/tiles/shell.sh"), false);
    assert_eq!(
        b,
        "# Added by tiles setup: plain claude, codex, opencode and pi draw cards; tiles shell off removes it\n\
         if [ -r '/Users/o'\\''neil/.config/tiles/shell.sh' ]; then . '/Users/o'\\''neil/.config/tiles/shell.sh'; fi\n"
    );
    assert_eq!(b.lines().count(), 2);
    let only_bash = block(Path::new("/h/shell.sh"), true);
    assert!(
        only_bash.ends_with(
            "\nif [ -n \"$BASH_VERSION\" ] && [ -r '/h/shell.sh' ]; then . '/h/shell.sh'; fi\n"
        ),
        "{only_bash}"
    );
}

#[test]
fn no_shell_found_means_no_plan() {
    let (root, places) = home("none");
    assert_eq!(
        plan(&places, &|_| None, Path::new(TILES), &of(&Harness::ALL)),
        None
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn zsh_as_the_login_shell_gets_its_zshrc_and_the_script() {
    let (root, places) = home("zsh");
    let p = plan_for(&places, "zsh").unwrap();
    assert_eq!(p.shells, [Kind::Zsh]);
    assert_eq!(p.files, std::slice::from_ref(&places.zshrc));
    assert_eq!(
        p.edits,
        [
            Edit::Write(
                places.script.clone(),
                script(Path::new(TILES), &Harness::ALL)
            ),
            Edit::Append(places.zshrc.clone(), block(&places.script, false)),
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

/// The bash startup files a plan wires on a home holding `files`, with
/// `login` as the login shell.
fn bash_files(name: &str, login: &'static str, files: &[&str]) -> Vec<String> {
    let (root, places) = home(name);
    for f in files {
        fs::write(root.join(f), "export A=1\n").unwrap();
    }
    let p = plan_for(&places, login).unwrap();
    let out = p
        .files
        .iter()
        .map(|f| f.strip_prefix(&root).unwrap().display().to_string())
        .collect();
    let _ = fs::remove_dir_all(root);
    out
}

#[test]
fn bash_gets_its_bashrc_and_the_login_file_a_login_bash_reads() {
    let cases: [(&str, &[&str], &[&str]); 7] = [
        ("none", &[], &[".bash_profile"]),
        ("profile", &[".profile"], &[".profile"]),
        ("bashrc", &[".bashrc"], &[".bashrc", ".bash_profile"]),
        (
            "both",
            &[".bashrc", ".bash_profile", ".profile"],
            &[".bashrc", ".bash_profile"],
        ),
        ("login", &[".bash_login", ".profile"], &[".bash_login"]),
        (
            "first",
            &[".bash_profile", ".bash_login"],
            &[".bash_profile"],
        ),
        (
            "rc-and-profile",
            &[".bashrc", ".profile"],
            &[".bashrc", ".profile"],
        ),
    ];
    for (name, files, wired) in cases {
        assert_eq!(
            bash_files(&format!("bash-{name}"), "bash", files),
            wired,
            "{name}"
        );
    }
    assert_eq!(
        bash_files("bash-zsh", "zsh", &[".bashrc"]),
        [".zshrc", ".bashrc", ".bash_profile"],
        "a bash file wires bash too"
    );
    assert_eq!(
        bash_files("bash-zsh-profile", "zsh", &[".profile"]),
        [".zshrc"],
        ".profile alone is not bash's"
    );
}

#[test]
fn profile_gets_a_block_only_bash_reads_and_undo_gives_every_file_back() {
    let (root, places) = home("profile");
    let profile = root.join(".profile");
    let rc = root.join(".bashrc");
    fs::write(&profile, "export B=2").unwrap();
    fs::write(&rc, "alias l=ls\n").unwrap();
    let p = plan_for(&places, "bash").unwrap();
    assert_eq!(
        p.edits[1..],
        [
            Edit::Append(rc.clone(), block(&places.script, false)),
            Edit::Append(profile.clone(), block(&places.script, true)),
        ]
    );
    apply(&p.edits).unwrap();
    assert_eq!(plan_for(&places, "bash").unwrap().edits, Vec::<Edit>::new());
    let done = undo(&places, false).unwrap();
    assert!(done.contains(&Undone::Removed(profile.clone())), "{done:?}");
    assert_eq!(fs::read_to_string(&profile).unwrap(), "export B=2");
    assert_eq!(fs::read_to_string(&rc).unwrap(), "alias l=ls\n");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_bash_profile_setup_created_is_deleted_by_undo() {
    let (root, places) = home("created");
    let made = root.join(".bash_profile");
    apply(&plan_for(&places, "bash").unwrap().edits).unwrap();
    assert_eq!(
        fs::read_to_string(&made).unwrap(),
        block(&places.script, false)
    );
    assert!(
        undo(&places, false)
            .unwrap()
            .contains(&Undone::Deleted(made.clone()))
    );
    assert!(!made.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn fish_gets_a_file_of_tiles_own_and_no_startup_file_changes() {
    let (root, places) = home("fish");
    let p = plan_for(&places, "fish").unwrap();
    assert_eq!(p.shells, [Kind::Fish]);
    let wanted = fish(Path::new(TILES), &Harness::ALL);
    assert_eq!(p.edits, [Edit::Write(places.fish.clone(), wanted.clone())]);
    apply(&p.edits).unwrap();
    assert_eq!(fs::read_to_string(&places.fish).unwrap(), wanted);
    assert_eq!(plan_for(&places, "fish").unwrap().edits, Vec::<Edit>::new());
    let _ = fs::remove_dir_all(root);
}

/// Wires zsh over `before` (`None`: no `.zshrc`), checks a second plan has
/// nothing to do, then undoes it and returns the `.zshrc` left.
fn round_trip(name: &str, before: Option<&[u8]>) -> Option<Vec<u8>> {
    let (root, places) = home(name);
    if let Some(bytes) = before {
        fs::write(&places.zshrc, bytes).unwrap();
    }
    let p = plan_for(&places, "zsh").unwrap();
    apply(&p.edits).unwrap();
    let wired = fs::read_to_string(&places.zshrc).unwrap();
    assert!(wired.ends_with(&block(&places.script, false)), "{wired}");
    assert_eq!(
        fs::read_to_string(&places.script).unwrap(),
        script(Path::new(TILES), &Harness::ALL)
    );
    let again = plan_for(&places, "zsh").unwrap();
    assert!(again.edits.is_empty(), "{again:?}");
    assert!(status_line(Some(&again)).starts_with("shell: wired for zsh through "));
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
    apply(&plan_for(&places, "zsh").unwrap().edits).unwrap();
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
    apply(&plan_for(&places, "zsh").unwrap().edits).unwrap();
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
    let p = plan_for(&places, "zsh").unwrap();
    assert_eq!(p.left, std::slice::from_ref(&places.zshrc));
    assert!(
        p.edits.is_empty(),
        "nothing reads the script, so it is not written"
    );
    assert!(
        status_line(Some(&p)).contains("holds changed Tiles lines"),
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
    apply(&plan_for(&places, "zsh").unwrap().edits).unwrap();
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
    let tiles = root.join("bin/tiles");
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::write(&tiles, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&tiles, fs::Permissions::from_mode(0o755)).unwrap();
    let p = plan(&places, &shell_is("zsh"), &tiles, &of(&Harness::ALL)).unwrap();
    assert_eq!(
        status_line(Some(&p)),
        "shell: not wired for zsh: plain sessions get no inline card; run tiles setup"
    );
    apply(&p.edits).unwrap();
    let p = plan(&places, &shell_is("zsh"), &tiles, &of(&Harness::ALL)).unwrap();
    assert_eq!(p.recorded.as_deref(), Some(tiles.as_path()));
    assert_eq!(
        status_line(Some(&p)),
        format!(
            "shell: wired for zsh through {}: pi, opencode, claude and codex typed in a new terminal start through tiles run",
            places.zshrc.display()
        )
    );
    let other = plan_for(&places, "zsh").unwrap();
    assert!(
        status_line(Some(&other)).ends_with(&format!(
            "start through {}; tiles setup rewrites the script for this tiles",
            tiles.display()
        )),
        "{other:?}"
    );
    fs::remove_file(&tiles).unwrap();
    let gone = plan(&places, &shell_is("zsh"), &tiles, &of(&Harness::ALL)).unwrap();
    assert_eq!(
        status_line(Some(&gone)),
        format!(
            "shell: wired for zsh through {}, but {} is missing, so pi, opencode, claude and codex typed in a new terminal start plain; run tiles setup",
            places.zshrc.display(),
            tiles.display()
        )
    );
    assert!(status_line(None).starts_with("shell: no zsh, bash or fish found"));
    let _ = fs::remove_dir_all(root);
}

/// A runnable stub `tiles` under `root`.
fn stub_tiles(root: &Path) -> std::path::PathBuf {
    let tiles = root.join("bin/tiles");
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::write(&tiles, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&tiles, fs::Permissions::from_mode(0o755)).unwrap();
    tiles
}

#[test]
fn a_wired_shell_reads_as_wired_while_another_waits_for_its_lines() {
    let (root, places) = home("partial");
    let tiles = stub_tiles(&root);
    let zsh = |h: &[Harness]| plan(&places, &shell_is("zsh"), &tiles, &of(h)).unwrap();
    apply(&zsh(&Harness::ALL).edits).unwrap();
    fs::write(&places.bashrc, "export N=1\n").unwrap();
    let p = zsh(&Harness::ALL);
    assert_eq!(p.to_wire(), [Kind::Bash]);
    assert_eq!(
        status_line(Some(&p)),
        format!(
            "shell: wired for zsh through {}: pi, opencode, claude and codex typed in a new terminal start through tiles run; bash not wired yet: run tiles setup",
            places.zshrc.display()
        )
    );
    fs::remove_file(&tiles).unwrap();
    assert_eq!(
        status_line(Some(&zsh(&Harness::ALL))),
        format!(
            "shell: wired for zsh through {}, but {} is missing, so pi, opencode, claude and codex typed in a new terminal start plain; run tiles setup; bash not wired yet: run tiles setup",
            places.zshrc.display(),
            tiles.display()
        )
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_scripts_follow_the_harnesses_on_path() {
    let (root, places) = home("found");
    fs::create_dir_all(&places.fish_dir).unwrap();
    let tiles = stub_tiles(&root);
    let zsh = |h: &[Harness]| plan(&places, &shell_is("zsh"), &tiles, &of(h)).unwrap();
    apply(&zsh(&[Harness::Pi]).edits).unwrap();
    let p = zsh(&[Harness::Pi]);
    assert_eq!(p.edits, Vec::<Edit>::new());
    assert_eq!(p.commands, [Harness::Pi]);
    assert!(
        status_line(Some(&p)).ends_with(": pi typed in a new terminal starts through tiles run"),
        "{p:?}"
    );
    let more = zsh(&[Harness::Pi, Harness::Claude]);
    assert_eq!(
        more.edits,
        [
            Edit::Write(
                places.script.clone(),
                script(&tiles, &[Harness::Pi, Harness::Claude])
            ),
            Edit::Write(
                places.fish.clone(),
                fish(&tiles, &[Harness::Pi, Harness::Claude])
            ),
        ]
    );
    assert_eq!(more.to_wire(), [Kind::Zsh, Kind::Fish]);
    assert!(
        status_line(Some(&more)).ends_with(
            ": pi typed in a new terminal starts through tiles run; claude not wired yet: run tiles setup"
        ),
        "{more:?}"
    );
    apply(&more.edits).unwrap();
    let p = zsh(&[Harness::Pi, Harness::Claude]);
    assert_eq!(p.commands, [Harness::Pi, Harness::Claude]);
    assert!(
        status_line(Some(&p))
            .ends_with(": pi and claude typed in a new terminal start through tiles run"),
        "{p:?}"
    );
    let fewer = zsh(&[Harness::Claude]);
    assert!(
        status_line(Some(&fewer)).ends_with(
            ": pi and claude typed in a new terminal start through tiles run; tiles setup removes the function for pi, no longer on PATH"
        ),
        "{fewer:?}"
    );
    apply(&fewer.edits).unwrap();
    assert_eq!(zsh(&[Harness::Claude]).commands, [Harness::Claude]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_pi_is_wired_unless_it_answers_as_another_program() {
    for answer in ["1.4.0\n", "Debugger attached.\n1.4.0\n"] {
        assert!(answers_as_pi(answer), "{answer:?}");
    }
    for answer in [
        "pi (cln) 1.3\n",
        "",
        "3.14159265358979323846\n",
        "2.1.316 (Claude Code)\n",
    ] {
        assert!(!answers_as_pi(answer), "{answer:?}");
    }
    let (root, places) = home("others");
    let tiles = stub_tiles(&root);
    let wired = |found: &Found| {
        apply(
            &plan(&places, &shell_is("zsh"), &tiles, found)
                .unwrap()
                .edits,
        )
        .unwrap();
        plan(&places, &shell_is("zsh"), &tiles, found).unwrap()
    };
    let p = wired(&Found {
        harnesses: vec![Harness::Claude],
        others: vec![Harness::Pi],
        ..Found::default()
    });
    assert_eq!(p.commands, [Harness::Claude]);
    assert!(
        status_line(Some(&p)).ends_with(
            ": claude typed in a new terminal starts through tiles run; the pi on PATH answered --version as another program, so tiles setup does not wire pi"
        ),
        "{p:?}"
    );
    let p = wired(&Found {
        harnesses: vec![Harness::Pi, Harness::Claude],
        unanswered: vec![Harness::Pi],
        ..Found::default()
    });
    assert_eq!(p.commands, [Harness::Pi, Harness::Claude]);
    assert!(
        status_line(Some(&p)).ends_with(
            ": pi and claude typed in a new terminal start through tiles run; the pi on PATH gave no --version answer in time, so tiles setup wires pi anyway"
        ),
        "{p:?}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn each_script_records_its_tiles_however_the_path_is_quoted() {
    for (name, tiles) in [
        ("plain", "/Users/me/.local/bin/tiles"),
        ("quote", "/Users/o'neil/bin/tiles"),
        ("backslash", "/opt/a\\b'\\/tiles"),
        ("space", "/Users/me/my bin/tiles"),
    ] {
        let (root, places) = home(&format!("record-{name}"));
        fs::create_dir_all(&places.fish_dir).unwrap();
        let tiles = Path::new(tiles);
        apply(
            &plan(&places, &shell_is("zsh"), tiles, &of(&Harness::ALL))
                .unwrap()
                .edits,
        )
        .unwrap();
        assert_eq!(
            plan(&places, &shell_is("zsh"), tiles, &of(&Harness::ALL))
                .unwrap()
                .recorded
                .as_deref(),
            Some(tiles),
            "{name}"
        );
        fs::remove_file(&places.script).unwrap();
        fs::remove_file(&places.zshrc).unwrap();
        assert_eq!(
            plan(&places, &shell_is("fish"), tiles, &of(&Harness::ALL))
                .unwrap()
                .recorded
                .as_deref(),
            Some(tiles),
            "{name}: fish"
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn the_recorded_tiles_is_the_one_path_names_not_the_file_it_links_to() {
    let (root, _) = home("installed");
    let cellar = root.join("Cellar/tiles/0.2.0/bin");
    let bin = root.join("bin");
    let old = root.join("old");
    for d in [&cellar, &bin, &old] {
        fs::create_dir_all(d).unwrap();
    }
    let real = cellar.join("tiles");
    fs::write(&real, "new").unwrap();
    fs::write(old.join("tiles"), "old").unwrap();
    std::os::unix::fs::symlink(&real, bin.join("tiles")).unwrap();
    let path = std::env::join_paths([&old, &bin]).unwrap();
    assert_eq!(installed(&path, &real), bin.join("tiles"));
    assert_eq!(installed(&path, &bin.join("tiles")), bin.join("tiles"));
    let elsewhere = root.join("elsewhere/tiles");
    assert_eq!(installed(&path, &elsewhere), elsewhere);
    let off_path = std::env::join_paths([&old]).unwrap();
    assert_eq!(installed(&off_path, &real), real);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn an_update_points_the_scripts_at_the_new_tiles() {
    let (root, places) = home("follow");
    fs::create_dir_all(&places.fish_dir).unwrap();
    let old = Path::new("/opt/homebrew/bin/tiles");
    let new = root.join(".local/bin/tiles");
    apply(
        &plan(&places, &shell_is("zsh"), old, &of(&Harness::ALL))
            .unwrap()
            .edits,
    )
    .unwrap();
    assert_eq!(
        follow(&places, &new, &Harness::ALL).unwrap(),
        [places.script.clone(), places.fish.clone()]
    );
    assert_eq!(
        fs::read_to_string(&places.script).unwrap(),
        script(&new, &Harness::ALL)
    );
    assert_eq!(
        fs::read_to_string(&places.fish).unwrap(),
        fish(&new, &Harness::ALL)
    );
    assert_eq!(
        plan(&places, &shell_is("zsh"), &new, &of(&Harness::ALL))
            .unwrap()
            .edits,
        Vec::<Edit>::new()
    );
    assert_eq!(
        follow(&places, &new, &Harness::ALL).unwrap(),
        Vec::<std::path::PathBuf>::new()
    );
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
