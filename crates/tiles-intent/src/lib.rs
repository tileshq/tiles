//! The chart intent language: what a model sends to ask for a chart.
//!
//! ADR-003 fixes the shape (form, fields, claim, highlight, title) and ADR-009
//! the limits. [`parse_request`] turns untrusted JSON into a validated
//! [`Request`] or a one-line [`IntentError`] that names the key, the problem
//! and the fix; it never panics, whatever the input. A request's `data` is
//! inline rows, an Omni topic (ADR-008) or a warehouse query (ADR-031):
//! [`Request::into_intent`] finishes an inline one, [`Request::with_table`]
//! one whose rows were fetched, and [`parse`] does both steps for inline
//! rows. [`source`] holds the `data` key and the typed [`source::Table`] the
//! fetched sources produce; [`text`] holds the
//! plain text rules every input and output path shares (ADR-009); [`schema`]
//! is the JSON Schema of the intent that the MCP tool publishes.
//!
//! Which roles a form has, and which columns fill them, is checked by the
//! engine against the form's definition file ([`Intent::column`] and
//! [`Intent::require_category_values`] are the helpers it uses).

pub mod schema;
pub mod source;
pub mod text;

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde_json::Value;

/// The intent schema version this crate speaks.
pub const SCHEMA_VERSION: u32 = 1;
/// The Tiles release this build is: `TILES_VERSION` at compile time, which
/// the release job sets to the tag without its `v`, else the workspace's
/// 0.0.0 for a development build.
pub const TILES_VERSION: &str = match option_env!("TILES_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
/// Largest intent accepted, in bytes of JSON (ADR-009).
pub const MAX_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
/// Most rows an inline intent may carry (ADR-003).
pub const MAX_ROWS: usize = 10_000;
/// Most columns a row may carry.
pub const MAX_COLUMNS: usize = 32;
/// Longest title, in characters (ADR-003).
pub const MAX_TITLE_CHARS: usize = 90;
/// Longest subtitle, in characters (ADR-003).
pub const MAX_SUBTITLE_CHARS: usize = 120;
/// Longest form name, field name, cell text or highlight value, in characters.
pub const MAX_TEXT_CHARS: usize = 120;
/// Most highlight values.
pub const MAX_HIGHLIGHT: usize = 50;
/// Longest note, in characters (ADR-012): a short callout, not a caption.
pub const MAX_NOTE_CHARS: usize = 48;
/// Largest magnitude a number cell may hold. Sums of up to 60 categories of
/// 5 series stay exact enough to draw and short enough to print; 1.7e308
/// stacked twice overflowed to infinity and drew a silently wrong chart.
pub const MAX_MAGNITUDE: f64 = 1e15;

/// Why an intent was refused: the key, the problem and the fix, printed on one
/// line because the reader is often a model that will retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentError {
    /// The key that failed, as a path such as `data.rows[3].week`.
    pub key: String,
    /// What is wrong with it.
    pub problem: String,
    /// What to send instead.
    pub fix: String,
}

impl IntentError {
    /// Builds an error; every part is cleaned to one line of plain text.
    #[must_use]
    pub fn new(key: impl AsRef<str>, problem: impl AsRef<str>, fix: impl AsRef<str>) -> Self {
        Self {
            key: one_line(key.as_ref()),
            problem: one_line(problem.as_ref()),
            fix: one_line(fix.as_ref()),
        }
    }
}

impl fmt::Display for IntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}; {}", self.key, self.problem, self.fix)
    }
}

impl std::error::Error for IntentError {}

/// What kind of finding the title states (ADR-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimKind {
    /// One item stands out in an ordering.
    Rank,
    /// A value moved between periods.
    Change,
    /// Two or more groups differ.
    Compare,
    /// A direction over time.
    Trend,
    /// A part of a whole.
    Share,
    /// How values spread.
    Distribution,
    /// Two measures move together.
    Relation,
    /// Progress against a goal.
    Target,
}

impl ClaimKind {
    /// Every kind, in the order the schema lists them.
    pub const ALL: [ClaimKind; 8] = [
        ClaimKind::Rank,
        ClaimKind::Change,
        ClaimKind::Compare,
        ClaimKind::Trend,
        ClaimKind::Share,
        ClaimKind::Distribution,
        ClaimKind::Relation,
        ClaimKind::Target,
    ];

    /// The kind as an intent spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ClaimKind::Rank => "rank",
            ClaimKind::Change => "change",
            ClaimKind::Compare => "compare",
            ClaimKind::Trend => "trend",
            ClaimKind::Share => "share",
            ClaimKind::Distribution => "distribution",
            ClaimKind::Relation => "relation",
            ClaimKind::Target => "target",
        }
    }
}

/// Whether the finding is good, bad or neither; it picks the accent colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    /// A good result: the accent colour.
    Good,
    /// A bad result: the red accent (playbook K2).
    Bad,
    /// Neither: the accent colour.
    Neutral,
}

impl Tone {
    /// Every tone, in the order the schema lists them.
    pub const ALL: [Tone; 3] = [Tone::Good, Tone::Bad, Tone::Neutral];

    /// The tone as an intent spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Tone::Good => "good",
            Tone::Bad => "bad",
            Tone::Neutral => "neutral",
        }
    }
}

/// What the measure counts, so every number on the card carries it
/// (ADR-012). Percent values are given in points: 31 reads 31%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    /// Percent points, written 31%.
    Percent,
    /// Euros, written €8.2M.
    Eur,
    /// US dollars, written $8.2M.
    Usd,
    /// Pounds sterling, written £8.2M.
    Gbp,
}

impl Unit {
    /// Every unit, in the order the schema lists them.
    pub const ALL: [Unit; 4] = [Unit::Percent, Unit::Eur, Unit::Usd, Unit::Gbp];

    /// The unit as an intent spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Unit::Percent => "percent",
            Unit::Eur => "eur",
            Unit::Usd => "usd",
            Unit::Gbp => "gbp",
        }
    }
}

/// The finding the title states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    /// The kind of finding.
    pub kind: ClaimKind,
    /// Its tone.
    pub tone: Tone,
}

/// One cell of the data table.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// A finite number.
    Number(f64),
    /// Plain text with no control characters.
    Text(String),
    /// A missing value.
    Null,
}

impl Cell {
    /// The cell as a number, if it is one.
    #[must_use]
    pub fn number(&self) -> Option<f64> {
        match self {
            Cell::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The cell as a number, or the number its text spells as written
    /// (`"48600"`, not `"48,600"`), which Tiles reads in its place.
    #[must_use]
    pub fn plain_number(&self) -> Option<f64> {
        match self {
            Cell::Number(n) => Some(*n),
            Cell::Text(t) => t.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
            Cell::Null => None,
        }
    }

    /// The cell as a category label: text as given, whole numbers without a
    /// decimal point, a missing value as an empty string.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Cell::Text(s) => s.clone(),
            Cell::Number(n) => number_label(*n),
            Cell::Null => String::new(),
        }
    }
}

fn number_label(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

/// The rows as a table: columns in order of first appearance.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    /// Column names.
    pub columns: Vec<String>,
    /// One entry per row, one cell per column.
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    /// The index of a column by name.
    #[must_use]
    pub fn index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

/// A validated chart intent.
#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    /// The form name, checked by the engine against its form list.
    pub form: String,
    /// The headline: the finding in words.
    pub title: String,
    /// The optional second line.
    pub subtitle: Option<String>,
    /// The kind and tone of the finding.
    pub claim: Claim,
    /// Role name to column name.
    pub fields: BTreeMap<String, String>,
    /// The values the title names, as category labels.
    pub highlight: Vec<String>,
    /// What the measure counts (ADR-012).
    pub unit: Option<Unit>,
    /// A short callout drawn beside the first highlighted mark (ADR-012).
    pub note: Option<String>,
    /// The rows.
    pub table: Table,
    /// What Tiles corrected or left out of the intent rather than refusing
    /// it, one sentence each, for the result text (ADR-020).
    pub notes: Vec<String>,
}

impl Intent {
    /// The column index that fills `role`, if the intent names one.
    #[must_use]
    pub fn column(&self, role: &str) -> Option<usize> {
        self.fields.get(role).and_then(|c| self.table.index(c))
    }

    /// Refuses a highlight value that names no category in `column`: no
    /// silent wrong highlight (ADR-003).
    ///
    /// # Errors
    ///
    /// The first highlight value that occurs in no row of that column.
    pub fn require_category_values(&self, columns: &[usize]) -> Result<(), IntentError> {
        for (i, h) in self.highlight.iter().enumerate() {
            let found = self
                .table
                .rows
                .iter()
                .any(|row| columns.iter().any(|&c| row[c].label() == *h));
            if !found {
                let names: Vec<&str> = columns
                    .iter()
                    .map(|&c| self.table.columns[c].as_str())
                    .collect();
                return Err(IntentError::new(
                    format!("highlight[{i}]"),
                    format!("\"{}\" occurs in no row of {}", clip(h), names.join(" or ")),
                    "name values exactly as they appear in the data, or drop highlight",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIntent {
    form: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: Option<String>,
    claim: Claim,
    fields: BTreeMap<String, String>,
    #[serde(default)]
    highlight: Vec<Value>,
    #[serde(default, deserialize_with = "blank_unit")]
    unit: Option<Unit>,
    #[serde(default)]
    note: Option<String>,
    data: source::DataSource,
}

/// A unit, with an empty string read as none: told to leave the unit out
/// for a count, a model sent `"unit": ""` and the refusal cost a retry.
fn blank_unit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Unit>, D::Error> {
    match Option::<String>::deserialize(d)?.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(name) => Unit::deserialize(serde::de::value::StrDeserializer::new(name)).map(Some),
    }
}

/// A validated intent whose rows may still have to be fetched: every rule
/// that does not need the rows has been checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The form name, checked by the engine against its form list.
    pub form: String,
    /// The headline: the finding in words.
    pub title: String,
    /// The optional second line.
    pub subtitle: Option<String>,
    /// The kind and tone of the finding.
    pub claim: Claim,
    /// Role name to column name.
    pub fields: BTreeMap<String, String>,
    /// The values the title names, as category labels.
    pub highlight: Vec<String>,
    /// What the measure counts (ADR-012).
    pub unit: Option<Unit>,
    /// A short callout beside the first highlighted mark (ADR-012).
    pub note: Option<String>,
    /// Where the rows come from.
    pub data: source::DataSource,
    /// What Tiles corrected or left out so far, one sentence each.
    pub notes: Vec<String>,
}

impl Request {
    /// The Omni query the rows come from, when they are not inline.
    #[must_use]
    pub fn omni(&self) -> Option<&source::OmniSource> {
        match &self.data {
            source::DataSource::Omni(omni) => Some(omni),
            source::DataSource::Rows(_) | source::DataSource::Warehouse(_) => None,
        }
    }

    /// The warehouse query the rows come from, when they are not inline.
    #[must_use]
    pub fn warehouse(&self) -> Option<&source::WarehouseSource> {
        match &self.data {
            source::DataSource::Warehouse(warehouse) => Some(warehouse),
            source::DataSource::Rows(_) | source::DataSource::Omni(_) => None,
        }
    }

    /// The intent with its inline rows checked and typed.
    ///
    /// # Errors
    ///
    /// When a row breaks a rule, a field names a missing column, or the rows
    /// live in Omni or a warehouse (use [`Request::with_table`] once they
    /// are fetched).
    pub fn into_intent(mut self) -> Result<Intent, IntentError> {
        let rows = match &mut self.data {
            source::DataSource::Rows(rows) => std::mem::take(rows),
            source::DataSource::Omni(_) => {
                return Err(IntentError::new(
                    "data.omni",
                    "needs its rows fetched from Omni first",
                    "configure Omni (see tiles doctor) or send the rows inline as data.rows",
                ));
            }
            source::DataSource::Warehouse(_) => {
                return Err(IntentError::new(
                    "data.warehouse",
                    "needs its rows fetched from the warehouse first",
                    "configure the warehouse (see tiles doctor) or send the rows inline as data.rows",
                ));
            }
        };
        if rows.is_empty() {
            return Err(IntentError::new(
                "data.rows",
                "is empty",
                "send at least one row",
            ));
        }
        if rows.len() > MAX_ROWS {
            return Err(IntentError::new(
                "data.rows",
                format!("{} rows is over the {MAX_ROWS} row limit", rows.len()),
                "aggregate the rows first",
            ));
        }
        let table = table(rows)?;
        self.finish(table, "the rows")
    }

    /// The intent drawn from rows fetched for its Omni or warehouse query.
    /// Booleans become the labels `true` and `false`.
    ///
    /// # Errors
    ///
    /// When a field names a column the query did not return, or the result has
    /// no rows.
    pub fn with_table(self, fetched: &source::Table) -> Result<Intent, IntentError> {
        let (key, fix, origin) = match self.data {
            source::DataSource::Warehouse(_) => (
                "data.warehouse",
                "check the query's filters and joins, or chart another table",
                "the query's columns",
            ),
            _ => (
                "data.omni",
                "check the topic's default filters in Omni, or chart another field",
                "the Omni fields",
            ),
        };
        if fetched.rows().is_empty() {
            return Err(IntentError::new(key, "returned no rows", fix));
        }
        let table = Table {
            columns: fetched.columns().iter().map(|c| c.name.clone()).collect(),
            rows: fetched
                .rows()
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| match cell {
                            source::Cell::Null => Cell::Null,
                            source::Cell::Bool(b) => Cell::Text(b.to_string()),
                            source::Cell::Number(n) => Cell::Number(*n),
                            source::Cell::Text(t) => Cell::Text(t.clone()),
                        })
                        .collect()
                })
                .collect(),
        };
        self.finish(table, origin)
    }

    fn finish(mut self, table: Table, origin: &str) -> Result<Intent, IntentError> {
        for (role, column) in &mut self.fields {
            if table.index(column).is_some() {
                continue;
            }
            // A column named with other capitals or spaces is that column.
            let fold = |s: &str| s.trim().to_lowercase();
            match table.columns.iter().find(|c| fold(c) == fold(column)) {
                Some(found) => {
                    self.notes.push(format!(
                        "Tiles read fields.{} \"{}\" as the column \"{}\".",
                        clip(role),
                        clip(column),
                        clip(found)
                    ));
                    found.clone_into(column);
                }
                None => {
                    return Err(IntentError::new(
                        format!("fields.{}", clip(role)),
                        format!("column \"{}\" is not in {origin}", clip(column)),
                        format!("use one of: {}", table.columns.join(", ")),
                    ));
                }
            }
        }
        let fetched = match self.data {
            source::DataSource::Omni(_) => Some("data.omni"),
            source::DataSource::Warehouse(_) => Some("data.warehouse"),
            source::DataSource::Rows(_) => None,
        };
        magnitudes(&table, fetched)?;
        prescaled(&self.title, &table, self.fields.get("y"))?;
        if fetched.is_none() {
            fractions(&self.title, self.unit, &table, &self.fields)?;
        }
        if counts_as_percent(self.unit, &table, self.fields.get("y")) {
            self.unit = None;
            self.notes.push(
                "The measure reaches 1,000 or more, so Tiles drew it as a count, not a percentage."
                    .into(),
            );
        }
        Ok(Intent {
            form: self.form,
            title: self.title,
            subtitle: self.subtitle,
            claim: self.claim,
            fields: self.fields,
            highlight: self.highlight,
            unit: self.unit,
            note: self.note,
            table,
            notes: self.notes,
        })
    }
}

/// Each run of digits a title writes, as its words, its value and the index
/// of the character after it: "1,250.5" is 1250.5.
fn digit_runs(chars: &[char]) -> Vec<(String, f64, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let starts = chars[i].is_ascii_digit() && (i == 0 || !chars[i - 1].is_alphanumeric());
        if !starts {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < chars.len()
            && (chars[end].is_ascii_digit()
                || (matches!(chars[end], ',' | '.')
                    && chars.get(end + 1).is_some_and(char::is_ascii_digit)))
        {
            end += 1;
        }
        let digits: String = chars[i..end].iter().filter(|c| **c != ',').collect();
        if let Ok(n) = digits.parse::<f64>() {
            out.push((chars[i..end].iter().collect(), n, end));
        }
        i = end;
    }
    out
}

/// The numbers a title writes, each with its step (1 for none) and its full
/// value: "$128K" is 128 on a step of 1000, 128000 in full.
fn title_numbers(title: &str) -> Vec<(String, f64, f64)> {
    let chars: Vec<char> = title.chars().collect();
    digit_runs(&chars)
        .into_iter()
        .filter_map(|(word, n, end)| {
            let rest: String = chars[end..].iter().take(3).collect();
            let step = [("bn", 1e9), ("K", 1e3), ("k", 1e3), ("M", 1e6), ("B", 1e9)]
                .into_iter()
                .find(|(s, _)| {
                    rest.starts_with(s)
                        && !rest[s.len()..]
                            .chars()
                            .next()
                            .is_some_and(char::is_alphanumeric)
                });
            let plain = !rest.chars().next().is_some_and(char::is_alphanumeric);
            match step {
                Some((s, scale)) => Some((format!("{word}{s}"), scale, n * scale)),
                None if plain => Some((word, 1.0, n)),
                None => None,
            }
        })
        .collect()
}

/// Refuses rows given in thousands or millions when the title writes the
/// full figure: a title of "$128K" over a value of 128 drew $128 (ADR-016).
/// A stepped number in the title must read as the headline (the measure's
/// last value) written on its own step, and no number in the title may match
/// a row as written; "NPS among 50K customers held at 48" passes on its 48.
/// The refusal names the title's words and never a row's value (ADR-013).
fn prescaled(title: &str, table: &Table, y: Option<&String>) -> Result<(), IntentError> {
    let Some(c) = y.and_then(|y| table.index(y)) else {
        return Ok(());
    };
    let values: Vec<f64> = table
        .rows
        .iter()
        .filter_map(|row| row[c].plain_number())
        .map(f64::abs)
        .collect();
    let Some(&headline) = values.last() else {
        return Ok(());
    };
    let near = |a: f64, b: f64| (0.8..=1.25).contains(&(a / b));
    let numbers = title_numbers(title);
    if numbers
        .iter()
        .any(|(_, _, n)| values.iter().any(|v| near(*n, *v)))
    {
        return Ok(());
    }
    match numbers
        .iter()
        .find(|(_, step, n)| *step > 1.0 && near(n / step, headline))
    {
        Some((word, _, _)) => Err(IntentError::new(
            "data.rows",
            format!(
                "values look pre-scaled: the title says {} but the rows hold it without its step",
                clip(word)
            ),
            "give full values (128000, not 128 for $128K); Tiles abbreviates them",
        )),
        None => Ok(()),
    }
}

/// The percents a title writes, such as 4.1 for "4.1%" or "4.1 percent".
fn title_percents(title: &str) -> Vec<f64> {
    let chars: Vec<char> = title.chars().collect();
    digit_runs(&chars)
        .into_iter()
        .filter(|(_, _, end)| {
            let rest: String = chars[*end..]
                .iter()
                .skip_while(|c| **c == ' ')
                .take(7)
                .collect::<String>()
                .to_lowercase();
            rest.starts_with('%') || rest.starts_with("percent") || rest.starts_with("pct")
        })
        .map(|(_, n, _)| n)
        .collect()
}

/// Refuses a percent measure given as fractions of 1: a model sent 0.041 for
/// a 4.1% conversion and the card read 0.041%. When every value lies between
/// -1 and 1, the title or that measure's column name decides, never a row:
/// points or a written percent of 1 or less mark small points, while a
/// written percent above 1, or a rate, share or percent word, marks
/// fractions. Each measure the unit is written on (y, or lo and hi on a
/// dumbbell) is checked on its own, and the refusal names every one that
/// fails. Tiles never multiplies, and the refusal names no row value
/// (ADR-048).
fn fractions(
    title: &str,
    unit: Option<Unit>,
    table: &Table,
    fields: &BTreeMap<String, String>,
) -> Result<(), IntentError> {
    if unit != Some(Unit::Percent) {
        return Ok(());
    }
    let percents = title_percents(title);
    let share = [
        "rate",
        "rates",
        "share",
        "percent",
        "percentage",
        "pct",
        "ratio",
    ];
    let refused: Vec<String> = ["y", "lo", "hi"]
        .into_iter()
        .filter_map(|role| fields.get(role))
        .filter_map(|column| Some((column, table.index(column)?)))
        .filter(|(column, c)| {
            let values: Vec<f64> = table
                .rows
                .iter()
                .filter_map(|row| row[*c].plain_number())
                .collect();
            if values.iter().all(|v| *v == 0.0) || values.iter().any(|v| v.abs() > 1.0) {
                return false;
            }
            let words = format!("{title} {column}").to_lowercase();
            let has = |list: &[&str]| {
                words
                    .split(|c: char| !c.is_alphanumeric())
                    .map(|w| w.trim_start_matches(|c: char| c.is_ascii_digit()))
                    .any(|w| list.contains(&w))
            };
            if has(&["point", "points", "pts", "pp", "bps"]) || percents.iter().any(|p| *p <= 1.0) {
                return false;
            }
            !percents.is_empty() || words.contains('%') || has(&share)
        })
        .map(|(column, _)| format!("\"{}\"", clip(column)))
        .collect();
    if refused.is_empty() {
        return Ok(());
    }
    Err(IntentError::new(
        "data.rows",
        format!(
            "every {} value lies between -1 and 1, so the rows look like fractions of 1 \
             where the percent unit takes points",
            refused.join(" and ")
        ),
        "resend the values in points (4.1, not 0.041); if they are points under 1, \
         say so in the title (0.4%, or 0.5 points)",
    ))
}

/// Whether a percent unit sits on a measure that reaches 1,000 or more: a
/// model gave monthly active users of 48,600 the percent unit, and the card
/// read 48.6K%. The card drops the unit and draws the count.
fn counts_as_percent(unit: Option<Unit>, table: &Table, y: Option<&String>) -> bool {
    let Some(c) = y
        .and_then(|y| table.index(y))
        .filter(|_| unit == Some(Unit::Percent))
    else {
        return false;
    };
    table
        .rows
        .iter()
        .filter_map(|row| row[c].plain_number())
        .any(|v| v.abs() >= 1000.0)
}

/// Parses and validates an intent with inline rows from JSON text.
///
/// # Errors
///
/// An [`IntentError`] naming the first key that breaks a rule, including an
/// intent whose rows live in Omni.
pub fn parse(json: &str) -> Result<Intent, IntentError> {
    parse_request(json)?.into_intent()
}

/// Parses and validates an intent from JSON text, leaving an Omni source's
/// rows to be fetched.
///
/// # Errors
///
/// An [`IntentError`] naming the first key that breaks a rule.
pub fn parse_request(json: &str) -> Result<Request, IntentError> {
    if json.len() > MAX_PAYLOAD_BYTES {
        return Err(IntentError::new(
            ARGUMENTS,
            format!(
                "{} bytes is over the {MAX_PAYLOAD_BYTES} byte limit",
                json.len()
            ),
            "send fewer rows or aggregate them first",
        ));
    }
    let invalid = |e: serde_json::Error| {
        IntentError::new(
            ARGUMENTS,
            format!("not a valid chart: {e}"),
            "send v, form, title, claim, fields and data as keys of this object itself, \
             not nested under another key, and optionally subtitle, highlight, unit and note",
        )
    };
    let mut value: Value = serde_json::from_str(json).map_err(invalid)?;
    let mut notes = Vec::new();
    let left_out = loosen(&mut value, &mut notes);
    // A refusal names the keys Tiles left out, since the model may have put
    // a key it needs under one of them.
    let naming = |mut e: IntentError| {
        if !left_out.is_empty() {
            e.problem = one_line(&format!(
                "{}, and Tiles left out the keys it does not take: {}",
                e.problem,
                left_out.join(", ")
            ));
        }
        e
    };
    let raw: RawIntent = serde_json::from_value(value).map_err(|e| naming(invalid(e)))?;
    validate(raw, notes).map_err(naming)
}

/// The key a refusal names when it is about the whole call rather than
/// one key in it.
pub const ARGUMENTS: &str = "arguments";

/// The note a call carries when its keys came nested under one `intent`
/// key and Tiles read them as the call's own (ADR-045).
pub const UNNESTED: &str = "Tiles read the keys under intent as the call's own; send them at the top level, not under intent.";

/// Lifts a call sent as exactly one `intent` key holding an object to that
/// object, and says whether it did. A model that read the tool's text as a
/// "chart intent" nested its keys under `intent` (ADR-045). [`parse_request`]
/// does not lift; the MCP server lifts each card before reading it.
pub fn unnest(value: &mut Value) -> bool {
    let Value::Object(o) = value else {
        return false;
    };
    if o.len() != 1 || !o.get("intent").is_some_and(Value::is_object) {
        return false;
    }
    if let Some(inner) = o.remove("intent") {
        *value = inner;
    }
    true
}

/// The keys an intent takes.
const KEYS: [&str; 10] = [
    "v",
    "form",
    "title",
    "subtitle",
    "claim",
    "fields",
    "highlight",
    "unit",
    "note",
    "data",
];

/// Reads what a model meant where nothing it sent can make a number wrong,
/// noting each change: keys Tiles does not know are left out, any version
/// is read as 1, a claim kind or tone is read in lower case and a missing
/// or unknown one takes the default, claim keys Tiles does not know are
/// left out, a unit spelled as a symbol or a word is read as that unit and
/// an unknown one is left out, and a lone highlight value is a list of one.
/// Returns the keys it left out, as the note names them.
fn loosen(value: &mut Value, notes: &mut Vec<String>) -> Vec<String> {
    let Value::Object(o) = value else {
        return Vec::new();
    };
    let unknown: Vec<String> = o
        .keys()
        .filter(|k| !KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    let names: Vec<String> = unknown.iter().map(|k| clip(k)).collect();
    if !unknown.is_empty() {
        for k in &unknown {
            o.remove(k);
        }
        notes.push(format!(
            "Tiles left out the keys it does not take: {}.",
            names.join(", ")
        ));
    }
    // The version has been checked here and is not kept.
    match o.remove("v") {
        None | Some(Value::Null) => {}
        Some(v) if v.as_u64() == Some(u64::from(SCHEMA_VERSION)) => {}
        Some(_) => notes.push(format!("Tiles read v as {SCHEMA_VERSION}.")),
    }
    let mut claim = match o.remove("claim") {
        Some(Value::Object(c)) => c,
        _ => serde_json::Map::new(),
    };
    for (key, names, default) in [
        (
            "kind",
            ClaimKind::ALL.map(ClaimKind::name).to_vec(),
            "compare",
        ),
        ("tone", vec!["good", "bad", "neutral"], "neutral"),
    ] {
        let sent = claim.get(key).and_then(Value::as_str).map(str::to_string);
        let given = sent.as_deref().map(|s| s.trim().to_lowercase());
        let known = given.as_deref().filter(|g| names.contains(g));
        if let Some(k) = known {
            if sent.as_deref() != Some(k) {
                notes.push(format!("Tiles read claim.{key} as {k}."));
            }
            claim.insert(key.into(), k.into());
        } else {
            claim.insert(key.into(), default.into());
            notes.push(format!("Tiles read claim.{key} as {default}."));
        }
    }
    let extra: Vec<String> = claim
        .keys()
        .filter(|k| *k != "kind" && *k != "tone")
        .map(|k| clip(k))
        .collect();
    if !extra.is_empty() {
        claim.retain(|k, _| k == "kind" || k == "tone");
        notes.push(format!(
            "Tiles left out the claim keys it does not take: {}.",
            extra.join(", ")
        ));
    }
    o.insert("claim".into(), Value::Object(claim));
    loosen_unit(o, notes);
    match o.get("highlight").cloned() {
        Some(Value::Null) => {
            o.remove("highlight");
        }
        Some(single @ (Value::String(_) | Value::Number(_))) => {
            o.insert("highlight".into(), Value::Array(vec![single]));
            notes.push("Tiles read highlight as a list of one value.".into());
        }
        _ => {}
    }
    names
}

/// [`loosen`] for the unit: a symbol or a word is read as that unit, and an
/// unknown one, or one that is not text, is left out.
fn loosen_unit(o: &mut serde_json::Map<String, Value>, notes: &mut Vec<String>) {
    match o.get("unit").cloned() {
        None | Some(Value::Null) => {}
        Some(Value::String(u)) => {
            let shown = clip(&one_line(&u));
            let read = match u.trim().to_lowercase().as_str() {
                "" | "count" | "number" | "none" => {
                    notes.push(format!("Tiles read unit \"{shown}\" as no unit."));
                    None
                }
                "%" | "percent" | "percentage" | "pct" | "pp" | "points" => Some("percent"),
                "$" | "usd" | "dollar" | "dollars" | "us$" => Some("usd"),
                "€" | "eur" | "euro" | "euros" => Some("eur"),
                "£" | "gbp" | "pound" | "pounds" => Some("gbp"),
                _ => {
                    notes.push(format!(
                        "Tiles drew the values without a unit, since \"{shown}\" is not one it \
                         writes (percent, eur, usd, gbp)."
                    ));
                    None
                }
            };
            if let Some(name) = read.filter(|name| *name != u) {
                notes.push(format!("Tiles read unit \"{shown}\" as {name}."));
            }
            match read {
                Some(name) => o.insert("unit".into(), name.into()),
                None => o.remove("unit"),
            };
        }
        Some(_) => {
            o.remove("unit");
            notes.push("Tiles drew the values without a unit, since unit was not text.".into());
        }
    }
}

/// Text a model wrote for the card, trimmed and cut at a word with an
/// ellipsis past `max` characters, the cut noted. Control and bidi
/// characters are still refused (ADR-009): they never come from a model
/// writing a chart, only from hostile data.
fn mend_text(
    key: &str,
    s: &str,
    max: usize,
    notes: &mut Vec<String>,
) -> Result<String, IntentError> {
    text(key, s, usize::MAX)?;
    let mut out = s.trim().to_string();
    if out.chars().count() > max {
        let cut: String = out.chars().take(max.saturating_sub(1)).collect();
        let cut = match cut.rfind(' ') {
            Some(at) if at > cut.len() / 2 => cut[..at].trim_end().to_string(),
            _ => cut,
        };
        out = format!("{cut}…");
        notes.push(format!("Tiles cut {key} to {max} characters."));
    }
    Ok(out)
}

/// A note, trimmed; blank is none. It sits beside a highlighted mark, so
/// without one it is left out (ADR-012).
fn note(
    note: Option<&str>,
    highlighted: bool,
    notes: &mut Vec<String>,
) -> Result<Option<String>, IntentError> {
    let Some(n) = note.map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    let n = mend_text("note", n, MAX_NOTE_CHARS, notes)?;
    if !highlighted {
        notes.push("Tiles left the note out, since no highlighted mark carries it.".into());
        return Ok(None);
    }
    Ok(Some(n))
}

fn validate(raw: RawIntent, mut notes: Vec<String>) -> Result<Request, IntentError> {
    text("form", &raw.form, MAX_TEXT_CHARS)?;
    let mut title = mend_text("title", &raw.title, MAX_TITLE_CHARS, &mut notes)?;
    if title.is_empty() {
        title = raw
            .fields
            .get("y")
            .map(|y| y.trim().chars().take(MAX_TITLE_CHARS).collect::<String>())
            .filter(|t| !t.is_empty() && text::first_refused(t).is_none())
            .unwrap_or_else(|| raw.form.clone());
        notes.push(format!(
            "The call had no title, so Tiles titled the card \"{}\".",
            clip(&title)
        ));
    }
    let subtitle = match raw.subtitle.as_deref() {
        Some(s) => Some(mend_text("subtitle", s, MAX_SUBTITLE_CHARS, &mut notes)?),
        None => None,
    }
    .filter(|s| !s.is_empty());
    let note = note(raw.note.as_deref(), !raw.highlight.is_empty(), &mut notes)?;
    for (role, column) in &raw.fields {
        text(&format!("fields.{}", clip(role)), role, MAX_TEXT_CHARS)?;
        text(&format!("fields.{}", clip(role)), column, MAX_TEXT_CHARS)?;
    }
    let checked = match &raw.data {
        source::DataSource::Omni(omni) => omni.validate(),
        source::DataSource::Warehouse(warehouse) => warehouse.validate(),
        source::DataSource::Rows(_) => Ok(()),
    };
    checked.map_err(|e| IntentError::new(e.key(), e.problem(), e.fix()))?;
    if raw.highlight.len() > MAX_HIGHLIGHT {
        notes.push(format!(
            "Tiles kept the first {MAX_HIGHLIGHT} highlight values of {}.",
            raw.highlight.len()
        ));
    }
    let mut highlight = Vec::with_capacity(raw.highlight.len().min(MAX_HIGHLIGHT));
    let mut dropped = 0;
    for (i, v) in raw.highlight.iter().take(MAX_HIGHLIGHT).enumerate() {
        match v {
            Value::String(s) => {
                text(&format!("highlight[{i}]"), s, usize::MAX)?;
                let s = s.trim();
                if s.is_empty() || s.chars().count() > MAX_TEXT_CHARS {
                    dropped += 1;
                } else {
                    highlight.push(s.to_string());
                }
            }
            Value::Number(n) => match n.as_f64() {
                Some(f) if f.is_finite() => highlight.push(number_label(f)),
                _ => dropped += 1,
            },
            _ => dropped += 1,
        }
    }
    if dropped > 0 {
        notes.push(format!(
            "Tiles left out {dropped} highlight value{} that named nothing it can draw.",
            if dropped == 1 { "" } else { "s" }
        ));
    }
    Ok(Request {
        form: raw.form,
        title,
        subtitle,
        claim: raw.claim,
        fields: raw.fields,
        highlight,
        unit: raw.unit,
        note,
        data: raw.data,
        notes,
    })
}

fn table(rows: Vec<serde_json::Map<String, Value>>) -> Result<Table, IntentError> {
    let mut columns: Vec<String> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for name in row.keys() {
            if !columns.iter().any(|c| c == name) {
                text(&format!("data.rows[{r}]"), name, MAX_TEXT_CHARS)?;
                if name.is_empty() {
                    return Err(IntentError::new(
                        format!("data.rows[{r}]"),
                        "has an empty column name",
                        "name every column",
                    ));
                }
                if columns.len() == MAX_COLUMNS {
                    return Err(IntentError::new(
                        format!("data.rows[{r}]"),
                        format!("more than {MAX_COLUMNS} columns"),
                        "send only the columns the chart uses",
                    ));
                }
                columns.push(name.clone());
            }
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    for (r, row) in rows.into_iter().enumerate() {
        let mut cells = vec![Cell::Null; columns.len()];
        for (name, value) in row {
            let key = format!("data.rows[{r}].{}", clip(&name));
            let c = columns.iter().position(|c| *c == name).unwrap_or(0);
            cells[c] = match value {
                Value::Null => Cell::Null,
                Value::Number(n) => match n.as_f64() {
                    Some(f) if f.is_finite() => Cell::Number(f),
                    _ => {
                        return Err(IntentError::new(
                            key,
                            "is not a finite number",
                            "send finite numbers only",
                        ));
                    }
                },
                Value::String(s) => {
                    text(&key, &s, MAX_TEXT_CHARS)?;
                    Cell::Text(s)
                }
                Value::Bool(_) => {
                    return Err(IntentError::new(
                        key,
                        "is true or false",
                        "send a number or text",
                    ));
                }
                Value::Array(_) | Value::Object(_) => {
                    return Err(IntentError::new(
                        key,
                        "is a nested value",
                        "flatten each row to plain numbers and text",
                    ));
                }
            };
        }
        out.push(cells);
    }
    Ok(Table { columns, rows: out })
}

/// Refuses a number cell larger than [`MAX_MAGNITUDE`], naming its row and
/// column, so no stack, running total or label overflows (ADR-003: no silent
/// wrong chart). Fetched rows are named by their source's key, `fetched`.
fn magnitudes(table: &Table, fetched: Option<&str>) -> Result<(), IntentError> {
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let Cell::Number(n) = cell else { continue };
            if n.abs() <= MAX_MAGNITUDE {
                continue;
            }
            let column = clip(&table.columns[c]);
            let key = fetched.map_or_else(|| format!("data.rows[{r}].{column}"), str::to_string);
            return Err(IntentError::new(
                key,
                format!("{column} holds a number larger than 1e15 in row {r}"),
                "chart the values in a larger unit, such as millions",
            ));
        }
    }
    Ok(())
}

/// Refuses text that is too long or carries control, bidi or invisible
/// characters other than a joiner between two emoji
/// ([`text::first_refused`]).
fn text(key: &str, s: &str, max: usize) -> Result<(), IntentError> {
    let n = s.chars().count();
    if n > max {
        return Err(IntentError::new(
            key,
            format!("{n} characters is over the {max} character limit"),
            "shorten it",
        ));
    }
    if let Some(c) = text::first_refused(s) {
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
    Ok(())
}

/// C0 and C1 controls, DEL, and the characters that reorder or hide text:
/// [`text::is_unsafe_char`], the one list input and output share.
#[must_use]
pub fn forbidden(c: char) -> bool {
    text::is_unsafe_char(c)
}

/// Cleans text for a one-line message: forbidden characters become `?` and
/// the result is cut to 300 characters.
#[must_use]
pub fn one_line(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if forbidden(c) { '?' } else { c })
        .take(300)
        .collect();
    if s.chars().count() > 300 {
        out.push('…');
    }
    out
}

fn clip(s: &str) -> String {
    let short: String = s.chars().take(40).collect();
    if s.chars().count() > 40 {
        format!("{short}…")
    } else {
        short
    }
}

#[cfg(test)]
mod tests;
