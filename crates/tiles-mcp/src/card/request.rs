//! Reading a call: its cards, rows and `dashboard` heading, then each
//! card's rows fetched and its width asked for.

use std::time::Instant;

use tiles_intent::{Intent, IntentError, Request};

use super::{
    Asked, Fetch, Fetched, Heading, MAX_CARDS, MAX_ROWS, MAX_TILES, Painter, Query, Refusal, within,
};
use crate::board;

/// One dashboard text: plain, within `max` characters, `None` when empty.
fn heading_text(
    key: &str,
    v: Option<&serde_json::Value>,
    max: usize,
) -> Result<Option<String>, IntentError> {
    let s = match v {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(serde_json::Value::String(s)) => s.trim(),
        Some(_) => return Err(IntentError::new(key, "must be text", "send a short string")),
    };
    let n = s.chars().count();
    if n > max {
        return Err(IntentError::new(
            key,
            format!("{n} characters is over the {max} character limit"),
            "shorten it",
        ));
    }
    if let Some(c) = tiles_intent::text::first_refused(s) {
        return Err(IntentError::new(
            key,
            format!(
                "contains the {} character U+{:04X}",
                if c.is_control() {
                    "control"
                } else {
                    "invisible"
                },
                u32::from(c)
            ),
            "send plain text without control, escape, bidi or invisible characters",
        ));
    }
    Ok((!s.is_empty()).then(|| s.to_string()))
}

/// One card under `row` or `rows`, its refusals named by `key`. A card
/// nested under one `intent` key is lifted before any other check.
fn card_request(key: String, v: &serde_json::Value) -> Result<(String, Request), IntentError> {
    let mut v = v.clone();
    let unnested = tiles_intent::unnest(&mut v);
    for nested in ["row", "rows", "dashboard"] {
        if v.get(nested).is_some() {
            return Err(IntentError::new(
                format!("{key}.{nested}"),
                format!("a card in a row cannot hold its own {nested}"),
                "list every row under the call's own rows",
            ));
        }
    }
    let request = read(&key, &v, unnested)?;
    Ok((key, request))
}

/// One card's request, its refusals named by `key`, noted when its keys
/// came nested under `intent`.
fn read(key: &str, v: &serde_json::Value, unnested: bool) -> Result<Request, IntentError> {
    let mut request = tiles_intent::parse_request(&v.to_string()).map_err(|e| within(key, e))?;
    if unnested {
        request.notes.insert(0, tiles_intent::UNNESTED.into());
    }
    Ok(request)
}

/// The `dashboard` key: its heading, or none when absent, null or empty.
fn dashboard(v: Option<serde_json::Value>) -> Result<Option<Heading>, IntentError> {
    use serde_json::Value;
    Ok(match v {
        None | Some(Value::Null) => None,
        Some(Value::Object(d)) => {
            if let Some(k) = d.keys().find(|k| !matches!(k.as_str(), "title" | "note")) {
                return Err(IntentError::new(
                    format!("dashboard.{}", tiles_intent::one_line(k)),
                    "is not a dashboard key",
                    "send dashboard with title and optionally note",
                ));
            }
            let title = heading_text(
                "dashboard.title",
                d.get("title"),
                tiles_intent::MAX_TITLE_CHARS,
            )?;
            let note = heading_text(
                "dashboard.note",
                d.get("note"),
                tiles_intent::MAX_SUBTITLE_CHARS,
            )?;
            match (title, note) {
                (Some(title), note) => Some(Heading { title, note }),
                (None, None) => None,
                (None, Some(_)) => {
                    return Err(IntentError::new(
                        "dashboard.title",
                        "a dashboard note needs a title",
                        "give the dashboard a short title, or leave dashboard out",
                    ));
                }
            }
        }
        Some(_) => {
            return Err(IntentError::new(
                "dashboard",
                "dashboard must be an object",
                "send dashboard as {\"title\": ..., \"note\": ...}",
            ));
        }
    })
}

/// The cards one call asks for: its own intent first, up to three more
/// beside it under `row`, further rows under `rows`, and an optional
/// `dashboard` heading. An empty or null `row` or `rows` is none. A call
/// nested under one `intent` key is lifted before any other check.
pub(super) fn requests(json: &str) -> Result<Asked, IntentError> {
    use serde_json::Value;
    let one = |request| Asked {
        rows: vec![vec![(String::new(), request)]],
        heading: None,
    };
    let Ok(mut value) = serde_json::from_str::<Value>(json) else {
        return Ok(one(tiles_intent::parse_request(json)?));
    };
    let unnested = tiles_intent::unnest(&mut value);
    let mut object = match value {
        Value::Object(o)
            if ["row", "rows", "dashboard"]
                .iter()
                .any(|k| o.contains_key(*k)) =>
        {
            o
        }
        value => return Ok(one(read("", &value, unnested)?)),
    };
    let list = |key: &str, v: Option<Value>, what: &str| match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(IntentError::new(
            key,
            format!("{key} must be an array of {what}"),
            format!("put {what} in a list under {key}"),
        )),
    };
    let row = list("row", object.remove("row"), "cards")?;
    let rows = list("rows", object.remove("rows"), "rows of cards")?;
    let heading = dashboard(object.remove("dashboard"))?;
    if row.len() + 1 > MAX_CARDS {
        return Err(IntentError::new(
            "row",
            format!(
                "a row holds 1 to {MAX_CARDS} cards and this one has {}",
                row.len() + 1
            ),
            "send at most 3 cards under row, and put the others in a row under rows",
        ));
    }
    if rows.len() + 1 > MAX_ROWS {
        return Err(IntentError::new(
            "rows",
            format!(
                "a dashboard holds 1 to {MAX_ROWS} rows and this one has {}",
                rows.len() + 1
            ),
            "send at most 3 rows under rows",
        ));
    }
    let mut first = vec![(String::new(), read("", &Value::Object(object), unnested)?)];
    for (i, v) in row.iter().enumerate() {
        first.push(card_request(format!("row[{i}]"), v)?);
    }
    let mut out = vec![first];
    for (r, line) in rows.iter().enumerate() {
        let key = format!("rows[{r}]");
        let cards = match line {
            Value::Array(a) if (1..=MAX_CARDS).contains(&a.len()) => a,
            Value::Array(a) => {
                return Err(IntentError::new(
                    key,
                    format!(
                        "a row holds 1 to {MAX_CARDS} cards and this one has {}",
                        a.len()
                    ),
                    "send each row as a list of 1 to 4 cards",
                ));
            }
            _ => {
                return Err(IntentError::new(
                    key,
                    "each row must be an array of cards",
                    "send rows as a list of lists of cards",
                ));
            }
        };
        out.push(
            cards
                .iter()
                .enumerate()
                .map(|(i, v)| card_request(format!("rows[{r}][{i}]"), v))
                .collect::<Result<_, _>>()?,
        );
    }
    let total: usize = out.iter().map(Vec::len).sum();
    if total > MAX_TILES {
        return Err(IntentError::new(
            "rows",
            format!("a dashboard holds at most {MAX_TILES} cards and this one has {total}"),
            "send the metrics that matter most, at most 8 cards",
        ));
    }
    Ok(Asked { rows: out, heading })
}

/// The intents asked for, in order, with their refusal keys and the rows
/// fetched for each (`None` when the model sent them): each request's rows
/// fetched when it names an Omni topic or a warehouse query, all within one
/// time budget from `started`, then corrected by [`Painter::mend`].
#[allow(clippy::type_complexity)] // Three lists, one entry per card, read together.
pub(super) fn load(
    rows: Vec<Vec<(String, Request)>>,
    painter: &dyn Painter,
    fetch: &Fetch<'_>,
) -> Result<(Vec<String>, Vec<Intent>, Vec<Option<Fetched>>), Refusal> {
    let started = Instant::now();
    let mut keys = Vec::new();
    let mut intents = Vec::new();
    let mut tables = Vec::new();
    for (key, request) in rows.into_iter().flatten() {
        let fetched = if let Some(source) = request.omni() {
            Some(fetch(Query::Omni(source), started).map_err(Refusal::Omni)?)
        } else if let Some(source) = request.warehouse() {
            Some(fetch(Query::Warehouse(source), started).map_err(Refusal::Warehouse)?)
        } else {
            None
        };
        let mut intent = match &fetched {
            Some(f) => request.with_table(&f.table),
            None => request.into_intent(),
        }
        .map_err(|e| Refusal::Intent(within(&key, e)))?;
        if let Some(f) = &fetched {
            intent.notes.extend(f.notes.iter().cloned());
        }
        tables.push(fetched);
        painter.mend(&mut intent);
        keys.push(key);
        intents.push(intent);
    }
    Ok((keys, intents, tables))
}

/// Writes where fetched rows were read from on the picture: as a lone
/// card's subtitle when it has none, or as a dashboard's note when it has
/// none and every fetched card shares the label ([`Fetched::label`]).
pub(super) fn label(
    intents: &mut [Intent],
    heading: Option<&mut Heading>,
    tables: &[Option<Fetched>],
) {
    let clean =
        |l: &str| tiles_intent::text::sanitize_truncated(l, tiles_intent::MAX_SUBTITLE_CHARS);
    let labels: Vec<&str> = tables
        .iter()
        .flatten()
        .filter_map(|f| f.label.as_deref())
        .collect();
    let Some(first) = labels.first() else {
        return;
    };
    match (heading, intents) {
        (None, [only]) if only.subtitle.is_none() => only.subtitle = Some(clean(first)),
        (Some(heading), _)
            if heading.note.is_none()
                && labels.len() == tables.iter().flatten().count()
                && labels.iter().all(|l| l == first) =>
        {
            heading.note = Some(clean(first));
        }
        _ => {}
    }
}

/// The cards as [`board::plan`] takes them, rows of `shape` cards each.
pub(super) fn asks(
    intents: &[Intent],
    keys: &[String],
    shape: &[usize],
    painter: &dyn Painter,
) -> Result<Vec<Vec<board::Ask>>, Refusal> {
    let among = intents.len() > 1;
    let mut cards = intents.iter().zip(keys).map(|(i, k)| {
        Ok(board::Ask {
            fit: painter
                .fit(i, among)
                .map_err(|e| Refusal::Intent(within(k, e)))?,
            scorecard: painter.scorecard(i),
        })
    });
    shape
        .iter()
        .map(|n| cards.by_ref().take(*n).collect())
        .collect()
}
