//! `tiles open <id>` (ADR-021, ADR-023): in a plain shell it tells a person
//! where the card is, drawn again only in a new theme, with the window
//! turned off here; inside `tiles run` it prints what the chart tool would.
//! A card drawn from Omni rows leaves no row on disk, so only the session
//! that fetched them draws it again (ADR-027).

use std::path::Path;
use std::process::Command;

const TILES: &str = env!("CARGO_BIN_EXE_tiles");

fn tiles(dir: &Path, args: &[&str], draw: Option<&str>) -> String {
    let mut cmd = Command::new(TILES);
    cmd.args(args)
        .env("TILES_CACHE_DIR", dir.join("cache"))
        .env("TILES_CONFIG_DIR", dir.join("config"))
        .env_remove("TILES_THEME")
        .env_remove("TILES_SESSION")
        .env_remove("TILES_DRAW")
        .env("TILES_WINDOW", "off");
    if let Some(d) = draw {
        cmd.env("TILES_DRAW", d);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn open_in_a_shell_names_the_redrawn_card_and_its_file() {
    let dir = std::env::temp_dir().join(format!("tiles-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let intent =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles-engine/tests/intents/column.json");
    let card: serde_json::Value = serde_json::from_str(&tiles(
        &dir,
        &["card", intent.to_str().unwrap()],
        Some("image"),
    ))
    .unwrap();
    let id = card["card"].as_str().unwrap();
    tiles(&dir, &["theme", "light"], None);

    let said = tiles(&dir, &["open", id], None);
    let (head, path) = said.trim_end().split_once(": ").unwrap();
    let new = head
        .strip_prefix("Card ")
        .and_then(|h| h.strip_suffix(" in light"))
        .unwrap_or_else(|| panic!("{said}"));
    assert_ne!(new, id, "a new card");
    assert_eq!(
        Path::new(path),
        dir.join("cache").join(format!("cards/{new}.png"))
    );
    assert!(Path::new(path).is_file());
    assert!(!said.contains("sees the chart"), "{said}");
    let again = tiles(&dir, &["open", new], None);
    assert!(
        again.starts_with(&format!("Card {new} in light: ")),
        "already light, so the same card: {again}"
    );

    let said = tiles(&dir, &["open", id], Some("block"));
    assert!(said.contains("Tiles drew"), "{said}");
    assert!(said.lines().any(|l| l.starts_with("[tile:")), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
}

const KEY: &str = "omni_planted_key_7Yq2xL9vR4";
const PLANTED: [&str; 4] = [
    "Planted Region 7Yq2xL9vR4",
    "918273.25",
    "Second Planted Row",
    "64.125",
];

// ADR-027: an Omni card's call file carries the call, the geometry and the
// topic, never a row. This test process stands in for the session that
// fetched the rows; `tiles open` runs as another process after it.
#[test]
fn an_omni_card_leaves_no_row_on_disk_and_opens_as_drawn_after_its_session() {
    use tiles_mcp::cache::Cache;
    use tiles_mcp::card::{self, EnginePainter, Look, Theme};
    use tiles_omni::replay::{Exchange, ReplayServer, recorded};

    let dir = std::env::temp_dir().join(format!("tiles-open-omni-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let rows = format!(
        r#"[{{"orders.region":"{}","orders.revenue":{}}},{{"orders.region":"{}","orders.revenue":{}}}]"#,
        PLANTED[0], PLANTED[1], PLANTED[2], PLANTED[3]
    );
    let server = ReplayServer::start(
        KEY,
        std::iter::once(Exchange::new("POST", "/v1/query/run", 200, rows))
            .chain(recorded())
            .collect(),
    );
    let client = tiles_omni::OmniClient::new(server.config());
    let fetch = |q: card::Query<'_>, _: std::time::Instant| match q {
        card::Query::Omni(s) => client
            .query(s)
            .map(card::Fetched::omni)
            .map_err(|e| e.to_string()),
        card::Query::Warehouse(_) => Err("no warehouse".to_string()),
    };
    let intent = serde_json::json!({
        "v": 1, "form": "bar", "title": "One region leads revenue",
        "fields": { "x": "orders.region", "y": "orders.revenue" },
        "data": { "omni": { "topic": "orders", "fields": ["orders.region", "orders.revenue"] } }
    });
    let cache = Cache::open(&dir.join("cache")).unwrap();
    let painter = EnginePainter::new().unwrap();
    let drawn = card::make(
        &intent.to_string(),
        &painter,
        &fetch,
        &cache,
        &Look::default(),
    )
    .unwrap();
    assert_eq!(drawn.faces[0].fetched.as_deref(), Some("Omni"));
    assert_eq!(server.unrecorded(), []);
    let id = drawn.id;

    let cards = dir.join("cache/cards");
    let mut written = 0;
    for entry in std::fs::read_dir(&cards).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x == "png") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for value in PLANTED {
            assert!(
                !text.contains(value),
                "{value} in {}: {text}",
                path.display()
            );
        }
        written += 1;
    }
    assert_eq!(written, 2, "the geometry and the call");
    let call: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cards.join(format!("{id}.call.json"))).unwrap())
            .unwrap();
    assert_eq!(call["omni"], serde_json::json!([true]));
    assert_eq!(call["call"]["data"]["omni"]["topic"], "orders");

    // Inside the session the rows are in memory, so the card changes theme
    // in place, and still no row reaches the disk.
    card::redraw(&id, &painter, &cache, Theme::from_name("light").unwrap()).unwrap();
    for name in [format!("{id}.json"), format!("{id}.call.json")] {
        let text = std::fs::read_to_string(cards.join(name)).unwrap();
        assert!(PLANTED.iter().all(|v| !text.contains(v)), "{text}");
    }
    drop(cache);

    tiles(&dir, &["theme", "dark"], None);
    let note = format!(
        "Card {id} was drawn from Omni rows, which Tiles keeps only in the memory of the session \
         that fetched them, so only that session can draw it again in another theme; ask for the \
         chart again to see it in dark."
    );
    let png = cards.join(format!("{id}.png"));
    let expected = format!("Card {id} in light: {}\n{note}\n", png.display());
    assert_eq!(tiles(&dir, &["open", &id], None), expected);
    assert_eq!(
        tiles(&dir, &["open", &id], Some("block")),
        expected,
        "inside tiles run too, the card as drawn"
    );
    let left: Vec<_> = std::fs::read_dir(&cards)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .collect();
    assert_eq!(left.len(), 1, "no new card was drawn");
    let _ = std::fs::remove_dir_all(&dir);
}
