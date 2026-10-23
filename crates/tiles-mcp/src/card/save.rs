//! Saving a drawn picture as a new card in the cache.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Card, Drawn, Look, Refusal, cells_of};
use crate::cache::Cache;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh card id: FNV-1a over the PNG, the time, the process and a counter,
/// so two identical charts in one conversation still get their own ids.
pub(super) fn card_id(png: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let salt = format!(
        "{nanos}:{}:{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    for b in png.iter().chain(salt.as_bytes()) {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:06x}", h & 0x00ff_ffff)
}

/// A fresh card id with `png` saved under it. An id the cache already holds
/// is never reused, so an older block naming it keeps showing its own card.
fn add(cache: &Cache, png: &[u8]) -> Result<(String, PathBuf), Refusal> {
    for _ in 0..16 {
        let id = card_id(png);
        if let Some(path) = cache.add_card(&id, png).map_err(Refusal::Internal)? {
            return Ok((id, path));
        }
    }
    Err(Refusal::Internal("no free card id in the cache".into()))
}

/// Saves a drawn picture as a new card, with its call beside it. The call
/// file says which cards' rows were fetched and from where; the rows go
/// only to [`Cache::hold`].
pub(super) fn save(
    call: &serde_json::Value,
    drawn: Drawn,
    cache: &Cache,
    look: &Look,
) -> Result<Card, Refusal> {
    let Drawn {
        art,
        stacked,
        tables,
        heading,
        lines,
    } = drawn;
    let (id, path) = add(cache, &art.png)?;
    cache
        .write_frames(&id, &art.frames)
        .map_err(Refusal::Internal)?;
    let title = heading.clone().unwrap_or_else(|| {
        let titles: Vec<&str> = art.faces.iter().map(|f| f.title.as_str()).collect();
        titles.join(" · ")
    });
    let cells = cells_of(&art);
    cache
        .write_call(
            &id,
            &serde_json::json!({
                "call": call,
                "omni": tables.iter().map(|t| t.as_ref().is_some_and(|f| f.origin == "Omni")).collect::<Vec<_>>(),
                "fetched": tables.iter().map(|t| t.as_ref().map(|f| f.origin.clone())).collect::<Vec<_>>(),
                "labels": tables.iter().map(|t| t.as_ref().and_then(|f| f.label.clone())).collect::<Vec<_>>(),
                "look": {
                    "term_cols": look.term_cols,
                    "term_rows": look.term_rows,
                    "cell_px": [look.cell_px.0, look.cell_px.1],
                    "scale": look.scale,
                },
            }),
        )
        .map_err(Refusal::Internal)?;
    if tables.iter().any(Option::is_some) {
        cache.hold(
            &id,
            tables.into_iter().map(|t| t.map(|f| f.table)).collect(),
        );
    }
    cache
        .write_meta(
            &id,
            &serde_json::json!({
                "cols": cells.0,
                "rows": cells.1,
                "title": title,
                "frames": art.frames.len(),
                "frame_ms": art.frame_ms,
                "session": look.session.map(|s| format!("{s:016x}")),
                "theme": look.theme.name(),
            }),
        )
        .map_err(Refusal::Internal)?;
    Ok(Card {
        id,
        path,
        title,
        cells,
        size_px: (art.width_px, art.height_px),
        png: art.png,
        faces: art.faces,
        heading,
        lines,
        stacked,
    })
}
