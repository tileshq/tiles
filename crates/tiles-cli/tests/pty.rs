//! Byte-level tests of `tiles run` through a real pseudo-terminal, one per
//! image route (ADR-004): the binary under test runs a fake harness that
//! prints a placeholder block for a card made by `tiles card`, stamping the
//! card's geometry first with the run's session token as the MCP server under
//! `tiles run` would write it, and the test reads every byte the wrapper
//! writes to the terminal.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

/// A fake harness's first line after `#!/bin/sh`: it answers `--version` as
/// Codex 0.163.0 does, which `tiles run` asks before the launch (ADR-032),
/// so the question never runs the rest of the script.
const ANSWERS_VERSION: &str = "[ \"$1\" = --version ] && { echo 'codex-cli 0.163.0'; exit 0; }\n";

struct Lab {
    dir: PathBuf,
    cache: PathBuf,
    bin: PathBuf,
    block: PathBuf,
    /// The card's geometry file, which the fake harness stamps with the
    /// run's session token.
    card: PathBuf,
}

/// Makes a card, a block file naming it, and a fake `codex` that prints it.
fn lab(name: &str) -> Lab {
    lab_on(name, "")
}

/// [`lab`], with `prefix` written before each block line, as a harness that
/// paints its conversation and indents the block does.
fn lab_on(name: &str, prefix: &str) -> Lab {
    lab_beside(name, prefix, "")
}

/// [`lab_on`], with `suffix` written after each block line, as a harness
/// with a side panel writes the panel's lines on the same rows.
fn lab_beside(name: &str, prefix: &str, suffix: &str) -> Lab {
    let dir = std::env::temp_dir().join(format!("tiles-pty-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = dir.join("cache");
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let intent =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles-engine/tests/intents/column.json");
    let out = Command::new(TILES)
        .args(["card", intent.to_str().unwrap()])
        .env("TILES_CACHE_DIR", &cache)
        .env("TILES_COLS", "50")
        .env("TILES_CELL_PX", "16x32")
        .env("TILES_DRAW", "block")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let card: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text = card["text"].as_str().unwrap();
    let block: Vec<&str> = text.lines().filter(|l| l.starts_with("[tile:")).collect();
    assert_eq!(
        block.len(),
        usize::try_from(card["rows"].as_u64().unwrap()).unwrap()
    );
    let card_meta = cache.join(format!("cards/{}.json", card["card"].as_str().unwrap()));
    let block_file = dir.join("block.txt");
    std::fs::write(
        &block_file,
        format!(
            "assistant reply\n{}\ndone\n",
            block
                .iter()
                .map(|l| format!("{prefix}{l}\x1b[0m{suffix}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .unwrap();
    let fake = bin.join("codex");
    std::fs::write(&fake, fake_harness("\"$TILES_SESSION\"")).unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    Lab {
        dir,
        cache,
        bin,
        block: block_file,
        card: card_meta,
    }
}

/// A fake harness that writes `session` (JSON, shell-expanded) into the
/// card's geometry, then prints the block.
fn fake_harness(session: &str) -> String {
    format!(
        "#!/bin/sh\n{ANSWERS_VERSION}\
         sed \"s/\\\"session\\\":null/\\\"session\\\":{}/\" \"$TILES_TEST_CARD\" > \"$TILES_TEST_CARD.new\" \\\n\
         && mv \"$TILES_TEST_CARD.new\" \"$TILES_TEST_CARD\"\n\
         cat \"$TILES_TEST_BLOCK\"\n\
         sleep 1\n",
        session.replace('"', "\\\"")
    )
}

/// Runs `tiles run --terminal <terminal> codex` in a 40x120 pseudo-terminal
/// and returns everything it wrote.
fn run(lab: &Lab, terminal: &str) -> Vec<u8> {
    let (mut pty, pts) = pty_process::blocking::open().unwrap();
    pty.resize(pty_process::Size::new(40, 120)).unwrap();
    let path = format!(
        "{}:{}",
        lab.bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child = pty_process::blocking::Command::new(TILES)
        .args(["run", "--terminal", terminal, "codex"])
        .env("PATH", path)
        .env("TILES_CACHE_DIR", &lab.cache)
        .env("TILES_CELL_PX", "16x32")
        .env("TILES_TEST_BLOCK", &lab.block)
        .env("TILES_TEST_CARD", &lab.card)
        .env("TILES_MOTION", "on")
        .env("TERM", "xterm-256color")
        .env_remove("TERM_PROGRAM")
        .env_remove("TILES_THEME")
        .env_remove("TMUX")
        .env_remove("STY")
        .env_remove("HERDR_PANE_ID")
        .env_remove("HERDR_SESSION")
        .spawn(pts)
        .unwrap();
    let mut out = Vec::new();
    let mut buf = vec![0u8; 65536];
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match pty.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
        }
    }
    let _ = child.wait();
    out
}

fn has(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn done(lab: &Lab) {
    let _ = std::fs::remove_dir_all(&lab.dir);
}

#[test]
fn every_route_starts_by_asking_the_terminal() {
    let lab = lab("query");
    let out = run(&lab, "kitty");
    assert!(has(&out, tiles_term::detect::QUERIES.as_bytes()));
    done(&lab);
}

#[test]
fn kitty_draws_through_placeholder_cells() {
    let lab = lab("kitty");
    let out = run(&lab, "kitty");
    assert!(
        has(&out, b"\x1b_Ga=T,f=100,t=d,q=2,U=1,i="),
        "virtual placement sent"
    );
    assert!(
        has(&out, "\u{10EEEE}".as_bytes()),
        "placeholder cells written"
    );
    assert!(has(&out, b"\x1b[?2026h"), "inside synchronized output");
    assert!(
        has(&out, b"assistant reply") && has(&out, b"done"),
        "the harness's text passes through"
    );
    done(&lab);
}

#[test]
fn kitty_card_cells_take_the_conversation_background() {
    let lab = lab_on("kitty-bg", "\x1b[48;2;10;10;10m  ");
    let out = run(&lab, "kitty");
    assert!(
        has(&out, b"\x1b[0;48;2;10;10;10m\x1b[38;2;"),
        "placeholder rows on the conversation's colour"
    );
    assert!(has(&out, "\u{10EEEE}".as_bytes()));
    assert!(
        !has(&out, b"\x1b[0m\x1b[38;2;"),
        "no placeholder row on the terminal's default"
    );
    done(&lab);
}

#[test]
fn a_fence_around_the_block_is_blanked_on_every_route() {
    for terminal in ["kitty", "warp", "foot"] {
        // A model that fences the block, shown as Pi shows it: the
        // fence lines at the margin and the block two columns in.
        let lab = lab_on(&format!("fence-{terminal}"), "  ");
        let block = std::fs::read_to_string(&lab.block).unwrap();
        let fenced = block
            .replacen("assistant reply\n", "assistant reply\n```tiles\n", 1)
            .replacen("\ndone\n", "\n```\ndone\n", 1);
        std::fs::write(&lab.block, fenced).unwrap();
        let out = run(&lab, terminal);
        let mut term = vt100::Parser::new(40, 120, 0);
        term.process(&out);
        let screen = term.screen().contents();
        assert!(
            screen.contains("assistant reply") && screen.contains("done"),
            "{terminal}: {screen}"
        );
        assert!(!screen.contains("```"), "{terminal}: {screen}");
        done(&lab);
    }
}

#[test]
fn kitty_cuts_a_card_at_a_side_panel_beside_its_block() {
    // The card is 42 columns wide; a panel's border and label sit at
    // column 39 on every row of its block, as OpenCode's sidebar does.
    let lab = lab_beside("kitty-panel", "", "\x1b[40G\u{2502} Context");
    let out = run(&lab, "kitty");
    let text = String::from_utf8_lossy(&out);
    let widths: Vec<usize> = text
        .split("\x1b[38;2;")
        .skip(1)
        .filter_map(|row| row.split_once("\x1b[39m"))
        .map(|(row, _)| row.matches('\u{10EEEE}').count())
        .filter(|n| *n > 0)
        .collect();
    assert!(!widths.is_empty(), "placeholder rows written");
    assert!(
        widths.iter().all(|w| *w == 39),
        "every row stops at the panel: {widths:?}"
    );
    assert!(
        has(&out, "\u{2502} Context".as_bytes()),
        "the panel passes through"
    );
    done(&lab);
}

/// Warp places each picture where the cursor is (`C=1`), no placeholder
/// cells. The whole card goes up over the rows its block will take from
/// the block's first row (ADR-044), so however the pty hands the fake
/// harness's `cat` over, the card's 23 rows go up as one picture.
#[test]
fn warp_draws_a_placed_picture_without_placeholders() {
    let lab = lab("warp");
    let out = run(&lab, "warp");
    let shown = String::from_utf8_lossy(&out);
    let strips: Vec<u16> = shown
        .split("\x1b_Ga=T,f=100,t=d,q=2,C=1,i=")
        .skip(1)
        .filter_map(|keys| keys.split_once(';'))
        .filter_map(|(keys, _)| keys.split_once(",c=42,r="))
        .filter_map(|(_, rows)| rows.split(',').next()?.parse().ok())
        .collect();
    assert_eq!(strips, [23], "the card's 23 rows as one picture");
    assert!(
        !has(&out, "\u{10EEEE}".as_bytes()),
        "no placeholders on Warp"
    );
    done(&lab);
}

/// The security review's F9: a block naming a card made for no session or
/// for another `tiles run` (an earlier session in scrollback, a second tab,
/// a file the harness shows) stays text on every route, even when the card
/// was written during this run.
#[test]
fn another_sessions_card_is_not_drawn() {
    let lab = lab("stale");
    for session in ["null", "\"0000000000000001\""] {
        std::fs::write(lab.bin.join("codex"), fake_harness(session)).unwrap();
        for terminal in ["kitty", "warp", "foot"] {
            let out = run(&lab, terminal);
            let case = format!("{terminal}, session {session}");
            assert!(has(&out, b"[tile:"), "{case}: the block passes through");
            assert!(!has(&out, b"\x1b_Ga="), "{case}: no kitty picture");
            assert!(!has(&out, b"\x1bP0;1;0q"), "{case}: no sixel picture");
        }
    }
    done(&lab);
}

#[test]
fn foot_draws_sixel() {
    let lab = lab("foot");
    let out = run(&lab, "foot");
    assert!(has(&out, b"\x1bP0;1;0q\"1;1;"), "a sixel picture");
    assert!(
        !has(&out, b"\x1b_Ga="),
        "no kitty commands on the sixel route"
    );
    done(&lab);
}

#[test]
fn a_terminal_without_images_runs_the_harness_unwrapped() {
    let lab = lab("text");
    let out = run(&lab, "apple-terminal");
    assert!(!has(&out, b"\x1b_Ga=") && !has(&out, b"\x1bP0;1;0q"));
    assert!(
        has(&out, b"[tile:"),
        "the block stays as the harness printed it"
    );
    done(&lab);
}

#[test]
fn narrowing_the_pane_past_a_wide_character_keeps_the_harness_alive() {
    // vt100 0.16.2 panics when a wide character ends at the new last column;
    // before the painter contained it, `tiles run` exited 101 and the harness
    // got SIGHUP before writing its mark.
    let lab = lab("resize");
    let mark = lab.dir.join("mark.txt");
    std::fs::write(
        lab.bin.join("codex"),
        format!(
            "#!/bin/sh\n{ANSWERS_VERSION}printf '%s\\342\\234\\205' {}\nsleep 2\nprintf 'K\\r\\n'\nsleep 0.5\necho alive > \"$TILES_TEST_MARK\"\n",
            "x".repeat(118)
        ),
    )
    .unwrap();
    let (pty, pts) = pty_process::blocking::open().unwrap();
    pty.resize(pty_process::Size::new(40, 120)).unwrap();
    let path = format!(
        "{}:{}",
        lab.bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child = pty_process::blocking::Command::new(TILES)
        .args(["run", "--terminal", "warp", "codex"])
        .env("PATH", path)
        .env("TILES_CACHE_DIR", &lab.cache)
        .env("TILES_CELL_PX", "16x32")
        .env("TILES_TEST_MARK", &mark)
        .env("TERM", "xterm-256color")
        .env_remove("TERM_PROGRAM")
        .env_remove("TMUX")
        .env_remove("STY")
        .env_remove("HERDR_PANE_ID")
        .env_remove("HERDR_SESSION")
        .spawn(pts)
        .unwrap();
    let reader = std::os::fd::AsFd::as_fd(&pty).try_clone_to_owned().unwrap();
    let (chunks, read) = std::sync::mpsc::channel::<Vec<u8>>();
    let output = std::thread::spawn(move || {
        let mut from = std::fs::File::from(reader);
        let mut out = Vec::new();
        let mut buf = vec![0u8; 65536];
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            match from.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    let _ = chunks.send(buf[..n].to_vec());
                }
            }
        }
        out
    });
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !has(&seen, "\u{2705}".as_bytes()) && Instant::now() < deadline {
        if let Ok(chunk) = read.recv_timeout(Duration::from_millis(100)) {
            seen.extend_from_slice(&chunk);
        }
    }
    pty.resize(pty_process::Size::new(40, 119)).unwrap();
    let status = child.wait().unwrap();
    let out = output.join().unwrap();
    assert!(
        status.success(),
        "tiles exited {status:?}: {}",
        String::from_utf8_lossy(&out)
    );
    assert!(!has(&out, b"panicked"), "no panic message on the terminal");
    assert!(
        has(&out, b"cards stopped drawing"),
        "the resize reached the screen model's panic"
    );
    assert!(mark.exists(), "the harness ran to its end");
    done(&lab);
}

/// In kitty the harness's own card (made as the MCP server makes it, with the
/// frame count `tiles run` hands the harness) goes to the terminal before its
/// block is typed, the crisp card first and its entrance after as an image
/// of its own; the entrance is placed on the block's first row and runs, all
/// before the block's last row arrives; then it stops on its settled frame
/// and is deleted, handing over to the crisp card's placeholder cells.
#[test]
fn kitty_places_the_card_on_its_first_row_and_plays_its_entrance() {
    let lab = lab("kitty-motion");
    let intent =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles-engine/tests/intents/column.json");
    let script = format!(
        "#!/bin/sh\n{ANSWERS_VERSION}\
         \"{tiles}\" card \"{intent}\" > \"{dir}/card.json\"\n\
         sleep 1.5\n\
         python3 -c 'import json,sys; t=json.load(open(sys.argv[1]))[\"text\"]; \
         rows=[l for l in t.splitlines() if l.startswith(\"[tile:\")]; \
         open(sys.argv[2],\"w\").write(rows[0]); open(sys.argv[3],\"w\").write(\"\\n\".join(rows[1:]))' \
         \"{dir}/card.json\" \"{dir}/first.txt\" \"{dir}/rest.txt\"\n\
         printf 'assistant reply\\n'; cat \"{dir}/first.txt\"; printf '\\n'\n\
         sleep 0.4\n\
         cat \"{dir}/rest.txt\"; printf '\\ndone\\n'\n\
         sleep 1.5\n",
        tiles = TILES,
        intent = intent.display(),
        dir = lab.dir.display(),
    );
    std::fs::write(lab.bin.join("codex"), script).unwrap();
    let out = run(&lab, "kitty");
    let at = |needle: &[u8]| {
        out.windows(needle.len())
            .position(|w| w == needle)
            .unwrap_or_else(|| panic!("{} not written", String::from_utf8_lossy(needle)))
    };
    let card: serde_json::Value =
        serde_json::from_slice(&std::fs::read(lab.dir.join("card.json")).unwrap()).unwrap();
    let rows = card["rows"].as_u64().unwrap();
    let id = u64::from_str_radix(card["card"].as_str().unwrap(), 16).unwrap();
    let crisp = tiles_term::kitty::image_id(id);
    // The wrapper's first image id (paint.rs FIRST_IMAGE) is the entrance's.
    let image = 0x50_0000;
    let settled = at(format!("\x1b_Ga=T,f=100,t=d,q=2,U=1,i={crisp},").as_bytes());
    let sent = at(format!("\x1b_Ga=t,f=100,t=d,q=2,i={image},").as_bytes());
    let frame = at(format!("\x1b_Ga=f,f=100,t=d,q=2,i={image},z=").as_bytes());
    let first_row = at(format!("[tile:{} 01/", card["card"].as_str().unwrap()).as_bytes());
    let placed = at(format!("C=1,i={image},p=2,").as_bytes());
    // Any frame after the first visible one, kitty's frame 2: a late tick
    // may skip some.
    let later = (3..=9)
        .filter_map(|c| {
            let needle = format!("\x1b_Ga=a,q=2,i={image},c={c},s=1");
            out.windows(needle.len())
                .position(|w| w == needle.as_bytes())
        })
        .min()
        .expect("a later frame is shown");
    let last_row = at(format!("{rows:02}/{rows:02}]").as_bytes());
    assert!(
        settled < sent && sent < first_row && frame < first_row,
        "the crisp card, then the entrance, before the block"
    );
    assert!(
        first_row < placed && placed < last_row,
        "placed on the first row"
    );
    assert!(
        later < last_row,
        "the entrance moves on while the block is typed"
    );
    // 10 frames: the settled card, the 8 visible entrance frames 50 ms
    // apart (frame 0, the card at no opacity, is never sent), the settled
    // card.
    let stop = at(format!("\x1b_Ga=a,q=2,i={image},c=10,s=1").as_bytes());
    let gone = at(format!("\x1b_Ga=d,d=i,q=2,i={image},p=2").as_bytes());
    let freed = at(format!("\x1b_Ga=d,d=I,q=2,i={image}\x1b").as_bytes());
    assert!(
        stop < gone && last_row < gone && gone < freed,
        "stopped, then handed over once the block is whole, and its frames freed"
    );
    assert!(
        has(&out, "\u{10EEEE}".as_bytes()),
        "the card's own cells remain"
    );
    done(&lab);
}

/// ADR-030 and ADR-044: in Warp the card's entrance frames go to the
/// terminal before its block is typed, as images of their own; the whole
/// card goes up on the block's first row, before its last, each 50 ms tick
/// placing the frame for the time passed, and the crisp card ends it. The
/// session numbers its pictures from an id of its own, never the fixed
/// first id every session used before.
#[test]
fn warp_sends_the_entrance_ahead_and_ends_on_the_crisp_card() {
    let lab = lab("warp-motion");
    let intent =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles-engine/tests/intents/column.json");
    let script = format!(
        "#!/bin/sh\n{ANSWERS_VERSION}\
         \"{tiles}\" card \"{intent}\" > \"{dir}/card.json\"\n\
         sleep 1.5\n\
         python3 -c 'import json,sys; t=json.load(open(sys.argv[1]))[\"text\"]; \
         rows=[l for l in t.splitlines() if l.startswith(\"[tile:\")]; \
         open(sys.argv[2],\"w\").write(rows[0]); open(sys.argv[3],\"w\").write(\"\\n\".join(rows[1:]))' \
         \"{dir}/card.json\" \"{dir}/first.txt\" \"{dir}/rest.txt\"\n\
         printf 'assistant reply\\n'; cat \"{dir}/first.txt\"; printf '\\n'\n\
         sleep 0.2\n\
         cat \"{dir}/rest.txt\"; printf '\\ndone\\n'\n\
         sleep 1.5\n",
        tiles = TILES,
        intent = intent.display(),
        dir = lab.dir.display(),
    );
    std::fs::write(lab.bin.join("codex"), script).unwrap();
    let out = run(&lab, "warp");
    let at = |needle: &[u8]| {
        out.windows(needle.len())
            .position(|w| w == needle)
            .unwrap_or_else(|| panic!("{} not written", String::from_utf8_lossy(needle)))
    };
    let card: serde_json::Value =
        serde_json::from_slice(&std::fs::read(lab.dir.join("card.json")).unwrap()).unwrap();
    let (cols, rows) = (
        card["cols"].as_u64().unwrap(),
        card["rows"].as_u64().unwrap(),
    );
    let sent = at(b"\x1b_Ga=t,f=100,t=d,q=2,i=");
    let first_id: u32 = String::from_utf8_lossy(&out[sent + 23..sent + 40])
        .split(',')
        .next()
        .and_then(|id| id.parse().ok())
        .expect("the first frame's id");
    assert!(
        first_id >= 0x0100_0000,
        "a session's own ids, above every card's: {first_id:#x}"
    );
    let first_row = at(format!("[tile:{} 01/", card["card"].as_str().unwrap()).as_bytes());
    let last_row = at(format!("{rows:02}/{rows:02}]").as_bytes());
    assert!(sent < first_row, "the entrance goes ahead of the block");
    // The whole card, every row, is placed before the last row is typed.
    let whole = format!(",c={cols},r={rows},z=");
    let early = out[first_row..last_row]
        .windows(whole.len())
        .any(|w| w == whole.as_bytes());
    assert!(early, "the whole card shows from the block's first row");
    assert!(
        !has(&out, b"a=f,") && !has(&out, b"U=1"),
        "no kitty animation and no placeholders on Warp"
    );
    // From the first row, a placement per tick: more than a few between
    // the first row and the end.
    let placement = |w: &[u8]| w == b"\x1b_Ga=T" || w == b"\x1b_Ga=p";
    let placements = out[first_row..].windows(6).filter(|w| placement(w)).count();
    assert!(placements >= 4, "{placements} placements");
    // The last placement is the crisp card, whole, after the last row.
    let crisp = format!(",c={cols},r={rows}");
    let last = out.windows(6).rposition(placement).unwrap();
    let keys = String::from_utf8_lossy(&out[last..(last + 80).min(out.len())]).into_owned();
    assert!(
        last > last_row && keys.starts_with("\u{1b}_Ga=T,f=100") && keys.contains(&crisp),
        "{keys}"
    );
    done(&lab);
}

/// ADR-021: switching theme inside a kitty session draws the session's
/// card again under its id, and the wrapper sends the new picture under the
/// card's image id with its virtual placement, so the placeholder cells
/// already on screen and in scrollback show it. Nothing is placed again and
/// no row is typed again.
#[test]
fn kitty_swaps_a_card_in_place_when_the_theme_changes() {
    let lab = lab("kitty-theme");
    // A theme other than the build's default, so switching to it redraws.
    let other = tiles_engine::Theme::all()
        .iter()
        .find(|t| **t != tiles_engine::Theme::default())
        .expect("the build carries more than one theme")
        .name();
    let intent =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles-engine/tests/intents/column.json");
    let script = format!(
        "#!/bin/sh\n{ANSWERS_VERSION}\
         export TILES_CONFIG_DIR=\"{dir}/config\"\n\
         \"{tiles}\" card \"{intent}\" > \"{dir}/card.json\"\n\
         cp \"{dir}/cache/cards/$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[\"card\"])' \"{dir}/card.json\").png\" \"{dir}/before.png\"\n\
         python3 -c 'import json,sys; t=json.load(open(sys.argv[1]))[\"text\"]; \
         open(sys.argv[2],\"w\").write(\"\\n\".join(l for l in t.splitlines() if l.startswith(\"[tile:\")))' \
         \"{dir}/card.json\" \"{dir}/rows.txt\"\n\
         printf 'assistant reply\\n'; cat \"{dir}/rows.txt\"; printf '\\ndone\\n'\n\
         sleep 2\n\
         printf 'SWAP' > \"{dir}/mark\"\n\
         \"{tiles}\" theme {other} > \"{dir}/theme.txt\"\n\
         sleep 1.5\n",
        tiles = TILES,
        intent = intent.display(),
        dir = lab.dir.display(),
    );
    std::fs::write(lab.bin.join("codex"), script).unwrap();
    let out = run(&lab, "kitty");
    let said = std::fs::read_to_string(lab.dir.join("theme.txt")).unwrap();
    assert!(
        said.contains("The 1 card drawn earlier in this session switches in place."),
        "{said}"
    );
    let card: serde_json::Value =
        serde_json::from_slice(&std::fs::read(lab.dir.join("card.json")).unwrap()).unwrap();
    let name = card["card"].as_str().unwrap();
    let (cols, rows) = (
        u16::try_from(card["cols"].as_u64().unwrap()).unwrap(),
        u16::try_from(card["rows"].as_u64().unwrap()).unwrap(),
    );
    let image = tiles_term::kitty::image_id(u64::from_str_radix(name, 16).unwrap());
    let before = std::fs::read(lab.dir.join("before.png")).unwrap();
    let after = std::fs::read(lab.cache.join(format!("cards/{name}.png"))).unwrap();
    assert_ne!(before, after, "the card was drawn again");
    let meta: serde_json::Value = serde_json::from_slice(
        &std::fs::read(lab.cache.join(format!("cards/{name}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(meta["theme"], other);
    let new = tiles_term::kitty::virtual_png(image, &after, cols, rows);
    let at = out
        .windows(new.len())
        .position(|w| w == new.as_slice())
        .expect("the redrawn card is sent under the card's image id");
    let old = tiles_term::kitty::virtual_png(image, &before, cols, rows);
    let settled = out
        .windows(old.len())
        .position(|w| w == old.as_slice())
        .expect("the card in the default theme was sent first");
    assert!(settled < at);
    let tail = &out[at + new.len()..];
    assert!(
        !has(tail, format!("i={image},").as_bytes()),
        "sent once, and no new placement after the swap"
    );
    assert!(
        !has(tail, format!("[tile:{name}").as_bytes()) && !has(tail, "\u{10EEEE}".as_bytes()),
        "no row is typed or painted again"
    );
    assert_eq!(
        out.windows(new.len())
            .filter(|w| *w == new.as_slice())
            .count(),
        1,
        "no duplicate"
    );
    done(&lab);
}
