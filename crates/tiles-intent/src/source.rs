//! Where an intent's rows come from (ADR-003 `data`, ADR-008, ADR-031), and
//! the typed table the fetched sources produce for the engine.
//!
//! `data` is exactly one of `{"rows": [...]}` (inline rows from the model),
//! `{"omni": {...}}` (an Omni topic Tiles queries) or `{"warehouse": {...}}`
//! (a `SELECT` the AI wrote, which Tiles guards and runs, [`WarehouseSource`]).
//! Either way the engine gets a [`Table`]: named columns of one kind each,
//! finite numbers only, plain text only.

mod warehouse;

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::text::{first_unsafe, is_format_char, is_unsafe_char, strip_format};

pub use warehouse::{
    MAX_REFERENCES, MAX_SQL_BYTES, ModelReference, WarehouseSource, check_sql, check_target,
};

/// Most rows a chart takes, from either source (ADR-003, ADR-009).
pub const MAX_ROWS: usize = 10_000;
/// The Omni row limit when the intent names none (ADR-008).
pub const DEFAULT_OMNI_LIMIT: u32 = 5_000;
/// Most columns a chart takes from one source.
pub const MAX_COLUMNS: usize = 16;
/// Longest text cell, in characters.
pub const MAX_TEXT_CHARS: usize = 1_000;
/// Longest column, topic or field name, in characters.
pub const MAX_NAME_CHARS: usize = 200;

/// The intent's `data`: exactly one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum DataSource {
    /// Inline rows, each an object keyed by column name.
    Rows(Vec<Map<String, Value>>),
    /// An Omni topic query.
    Omni(OmniSource),
    /// A warehouse query.
    Warehouse(WarehouseSource),
}

/// An Omni topic query: the topic, the fields to fetch, how to sort them and how
/// many rows to take. Queries carry the topic's own default filters and no
/// others (filters are stage 2, ADR-001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OmniSource {
    /// The topic name, as `tiles omni topics` lists it.
    pub topic: String,
    /// Fully qualified field names (`view.field`), each one the topic exposes.
    /// Parsing lowercases a timeframe: `[WEEK]` becomes `[week]`.
    #[serde(deserialize_with = "canonical_fields")]
    pub fields: Vec<String>,
    /// Sort order, first key first. Each field must also be in `fields`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sorts: Vec<Sort>,
    /// Most rows to fetch; [`DEFAULT_OMNI_LIMIT`] when absent, at most [`MAX_ROWS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// One sort key of an [`OmniSource`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sort {
    /// The field to sort by, its timeframe lowercased like [`OmniSource::fields`].
    #[serde(deserialize_with = "canonical_field")]
    pub field: String,
    /// Largest first when true.
    #[serde(default)]
    pub desc: bool,
}

/// `name` with its timeframe lowercased, the one spelling Omni lists and keys
/// rows by: `orders.created_at[WEEK]` becomes `orders.created_at[week]`.
fn lowercase_timeframe(mut name: String) -> String {
    if let Some(open) = name.find('[') {
        name[open..].make_ascii_lowercase();
    }
    name
}

fn canonical_fields<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let fields = Vec::<String>::deserialize(d)?;
    Ok(fields.into_iter().map(lowercase_timeframe).collect())
}

fn canonical_field<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    String::deserialize(d).map(lowercase_timeframe)
}

impl OmniSource {
    /// Checks the names, the sorts and the limit, without asking Omni anything.
    ///
    /// # Errors
    ///
    /// The first problem found, naming the key and the fix.
    pub fn validate(&self) -> Result<(), DataError> {
        check_topic_name(&self.topic).map_err(|problem| {
            DataError::new(
                "data.omni.topic",
                problem,
                "use a topic name exactly as `tiles omni topics` lists it",
            )
        })?;
        if self.fields.is_empty() || self.fields.len() > MAX_COLUMNS {
            return Err(DataError::new(
                "data.omni.fields",
                format!("names {} fields", self.fields.len()),
                format!("name between 1 and {MAX_COLUMNS} fields"),
            ));
        }
        let mut seen = HashSet::new();
        for (i, field) in self.fields.iter().enumerate() {
            let key = format!("data.omni.fields[{i}]");
            check_field_name(field).map_err(|problem| {
                DataError::new(
                    &key,
                    problem,
                    "use a field name exactly as `tiles omni fields` lists it",
                )
            })?;
            if !seen.insert(field.as_str()) {
                return Err(DataError::new(
                    &key,
                    format!("repeats `{field}`"),
                    "name each field once",
                ));
            }
        }
        if self.sorts.len() > self.fields.len() {
            return Err(DataError::new(
                "data.omni.sorts",
                format!(
                    "has {} keys for {} fields",
                    self.sorts.len(),
                    self.fields.len()
                ),
                "sort by each field at most once",
            ));
        }
        let mut sorted = HashSet::new();
        for (i, sort) in self.sorts.iter().enumerate() {
            let key = format!("data.omni.sorts[{i}].field");
            if !seen.contains(sort.field.as_str()) {
                let shown = if check_field_name(&sort.field).is_ok() {
                    format!("`{}` is not in data.omni.fields", sort.field)
                } else {
                    "is not in data.omni.fields".to_owned()
                };
                return Err(DataError::new(
                    &key,
                    shown,
                    "sort only by a field you fetch",
                ));
            }
            if !sorted.insert(sort.field.as_str()) {
                return Err(DataError::new(
                    &key,
                    format!("sorts by `{}` twice", sort.field),
                    "sort by each field at most once",
                ));
            }
        }
        if let Some(limit) = self.limit
            && (limit == 0 || limit as usize > MAX_ROWS)
        {
            return Err(DataError::new(
                "data.omni.limit",
                format!("is {limit}"),
                format!("use a limit between 1 and {MAX_ROWS}"),
            ));
        }
        Ok(())
    }

    /// The row limit the query runs with.
    #[must_use]
    pub fn effective_limit(&self) -> u32 {
        self.limit.unwrap_or(DEFAULT_OMNI_LIMIT)
    }
}

/// Checks an Omni topic name: 1 to [`MAX_NAME_CHARS`] ASCII letters, digits,
/// `_` or `-`, not starting with `-`. The name goes into a URL path, so nothing
/// else is let through.
///
/// # Errors
///
/// What is wrong, without echoing the name.
pub fn check_topic_name(name: &str) -> Result<(), String> {
    check_name_length(name)?;
    if name.starts_with('-') {
        return Err("starts with `-`".to_owned());
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("may hold only ASCII letters, digits, `_` and `-`".to_owned());
    }
    Ok(())
}

/// Checks a fully qualified Omni field name: dot-separated segments of ASCII
/// letters, digits and `_` (`view.field`, `view.joined_view.field`), with an
/// optional timeframe suffix such as `[week]`.
///
/// # Errors
///
/// What is wrong, without echoing the name.
pub fn check_field_name(name: &str) -> Result<(), String> {
    check_name_length(name)?;
    let (path, timeframe) = match name.split_once('[') {
        Some((path, rest)) => match rest.strip_suffix(']') {
            Some(frame) => (path, Some(frame)),
            None => return Err("opens `[` without closing it at the end".to_owned()),
        },
        None => (name, None),
    };
    let segments: Vec<&str> = path.split('.').collect();
    if segments.len() < 2 {
        return Err("is not of the form `view.field`".to_owned());
    }
    let word = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if !segments.iter().all(|s| word(s)) {
        return Err("may hold only ASCII letters, digits, `_` and `.` between names".to_owned());
    }
    if let Some(frame) = timeframe
        && !word(frame)
    {
        return Err("has a timeframe that is not a plain word".to_owned());
    }
    Ok(())
}

fn check_name_length(name: &str) -> Result<(), String> {
    let chars = name.chars().count();
    if chars == 0 {
        return Err("is empty".to_owned());
    }
    if chars > MAX_NAME_CHARS {
        return Err(format!("is {chars} characters, over {MAX_NAME_CHARS}"));
    }
    Ok(())
}

/// One value of a [`Table`].
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// No value.
    Null,
    /// A true or false value.
    Bool(bool),
    /// A finite number.
    Number(f64),
    /// Plain text: no control, escape, bidi or invisible characters but a
    /// joiner inside an emoji sequence.
    Text(String),
}

impl Cell {
    /// The cell a JSON value gives; `key` names it in the error.
    fn from_json(value: Value, key: &dyn Fn() -> String) -> Result<Self, DataError> {
        match value {
            Value::Null => Ok(Self::Null),
            Value::Bool(b) => Ok(Self::Bool(b)),
            Value::Number(n) => match n.as_f64() {
                Some(f) if f.is_finite() => Ok(Self::Number(f)),
                _ => Err(DataError::new(
                    &key(),
                    "is not a finite number",
                    "send finite numbers only",
                )),
            },
            Value::String(s) => {
                let chars = s.chars().count();
                if chars > MAX_TEXT_CHARS {
                    return Err(DataError::new(
                        &key(),
                        format!("is {chars} characters, over {MAX_TEXT_CHARS}"),
                        "chart a shorter field",
                    ));
                }
                if let Some((at, c)) = s
                    .chars()
                    .enumerate()
                    .find(|&(_, c)| is_unsafe_char(c) && !is_format_char(c))
                {
                    let what = if c.is_control() {
                        "a control"
                    } else {
                        "an invisible"
                    };
                    return Err(DataError::new(
                        &key(),
                        format!("has {what} character at position {at}"),
                        "send plain text only",
                    ));
                }
                Ok(Self::Text(strip_format(&s)))
            }
            Value::Array(_) | Value::Object(_) => Err(DataError::new(
                &key(),
                "is a list or an object",
                "send numbers, text, booleans or null",
            )),
        }
    }

    /// The column kind this cell fits.
    #[must_use]
    pub fn kind(&self) -> ColumnKind {
        match self {
            Self::Null => ColumnKind::Empty,
            Self::Bool(_) => ColumnKind::Bool,
            Self::Number(_) => ColumnKind::Number,
            Self::Text(_) => ColumnKind::Text,
        }
    }
}

/// What every non-null cell of a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    /// Every cell is null.
    Empty,
    /// True or false values.
    Bool,
    /// Finite numbers.
    Number,
    /// Plain text.
    Text,
}

impl ColumnKind {
    /// The kind's name as messages print it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Bool => "bool",
            Self::Number => "number",
            Self::Text => "text",
        }
    }
}

/// A named column of a [`Table`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The column name: an Omni field name or an inline row key.
    pub name: String,
    /// What its cells hold.
    pub kind: ColumnKind,
}

/// Validated rows for the engine: every row has every column, each column holds
/// one kind of value, numbers are finite, text is plain and within limits.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
}

impl Table {
    /// Builds a table from JSON row objects.
    ///
    /// With `columns` given, every row must hold exactly those keys, and the
    /// table keeps their order. Without, the first row's keys are the columns
    /// and every other row must hold the same keys. `origin` prefixes each
    /// error's key, such as `data.rows` or `omni.query`. Text cells lose
    /// their format characters ([`strip_format`]): the rows are fetched, so
    /// nobody chose those characters for the chart.
    ///
    /// # Errors
    ///
    /// The first problem found, naming the row and the column but never a value.
    pub fn from_json_rows(
        rows: Vec<Map<String, Value>>,
        columns: Option<&[String]>,
        origin: &str,
    ) -> Result<Self, DataError> {
        if rows.len() > MAX_ROWS {
            return Err(DataError::new(
                origin,
                format!("has {} rows, over {MAX_ROWS}", rows.len()),
                "aggregate the data or lower the limit",
            ));
        }
        let names: Vec<String> = match columns {
            Some(names) => names.to_vec(),
            None => rows
                .first()
                .map(|row| row.keys().cloned().collect())
                .unwrap_or_default(),
        };
        if names.len() > MAX_COLUMNS {
            return Err(DataError::new(
                origin,
                format!("has {} columns, over {MAX_COLUMNS}", names.len()),
                "keep only the columns the chart uses",
            ));
        }
        for (i, name) in names.iter().enumerate() {
            if let Err(problem) = check_column_name(name) {
                return Err(DataError::new(
                    &format!("{origin}[0] column {i}"),
                    format!("has a name that {problem}"),
                    "name columns in plain text",
                ));
            }
        }
        let mut kinds = vec![ColumnKind::Empty; names.len()];
        let mut table_rows = Vec::with_capacity(rows.len());
        for (r, mut row) in rows.into_iter().enumerate() {
            if row.len() != names.len() || !names.iter().all(|n| row.contains_key(n)) {
                return Err(DataError::new(
                    &format!("{origin}[{r}]"),
                    format!("does not hold exactly the {} expected columns", names.len()),
                    "give every row the same keys",
                ));
            }
            let mut cells = Vec::with_capacity(names.len());
            for (c, name) in names.iter().enumerate() {
                let key = || format!("{origin}[{r}].{name}");
                let cell = Cell::from_json(row.remove(name).unwrap_or(Value::Null), &key)?;
                let kind = cell.kind();
                if kind != ColumnKind::Empty {
                    if kinds[c] == ColumnKind::Empty {
                        kinds[c] = kind;
                    } else if kinds[c] != kind {
                        return Err(DataError::new(
                            &key(),
                            format!("is {} in a {} column", kind.name(), kinds[c].name()),
                            "give each column one kind of value",
                        ));
                    }
                }
                cells.push(cell);
            }
            table_rows.push(cells);
        }
        let columns = names
            .into_iter()
            .zip(kinds)
            .map(|(name, kind)| Column { name, kind })
            .collect();
        Ok(Self {
            columns,
            rows: table_rows,
        })
    }

    /// The columns, in order.
    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// The rows, each with one cell per column in column order.
    #[must_use]
    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.rows
    }

    /// The index of the column called `name`.
    #[must_use]
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

fn check_column_name(name: &str) -> Result<(), String> {
    check_name_length(name)?;
    if first_unsafe(name).is_some() {
        return Err("holds a control character".to_owned());
    }
    Ok(())
}

/// A refused source or row, in one line: the key, the problem and the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataError {
    key: String,
    problem: String,
    fix: String,
}

impl DataError {
    /// A refusal of `key` because of `problem`, with the `fix` to try.
    #[must_use]
    pub fn new(key: &str, problem: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            key: crate::text::sanitize(key),
            problem: problem.into(),
            fix: fix.into(),
        }
    }

    /// The key that was refused, such as `data.omni.fields[2]`.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// What is wrong with the key.
    #[must_use]
    pub fn problem(&self) -> &str {
        &self.problem
    }

    /// What to send instead.
    #[must_use]
    pub fn fix(&self) -> &str {
        &self.fix
    }
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}; {}", self.key, self.problem, self.fix)
    }
}

impl std::error::Error for DataError {}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::{
        Cell, ColumnKind, DataSource, MAX_ROWS, MAX_TEXT_CHARS, OmniSource, Sort, Table,
        check_field_name, check_topic_name,
    };

    fn rows(value: Value) -> Vec<Map<String, Value>> {
        serde_json::from_value(value).expect("rows")
    }

    fn source(fields: &[&str]) -> OmniSource {
        OmniSource {
            topic: "orders".into(),
            fields: fields.iter().map(|f| (*f).to_owned()).collect(),
            sorts: vec![],
            limit: None,
        }
    }

    #[test]
    fn data_parses_as_exactly_one_source() {
        let omni: DataSource = serde_json::from_value(json!({
            "omni": { "topic": "orders", "fields": ["orders.week", "orders.count"], "limit": 12 }
        }))
        .expect("omni source");
        assert!(matches!(omni, DataSource::Omni(ref s) if s.limit == Some(12)));
        let inline: DataSource =
            serde_json::from_value(json!({ "rows": [{ "a": 1 }] })).expect("rows source");
        assert!(matches!(inline, DataSource::Rows(ref r) if r.len() == 1));
        let both = serde_json::from_value::<DataSource>(json!({ "rows": [], "omni": {} }));
        assert!(both.is_err());
    }

    #[test]
    fn parsing_lowercases_timeframes_only() {
        let data: DataSource = serde_json::from_value(json!({ "omni": {
            "topic": "orders",
            "fields": ["Orders.Created_At[WEEK]", "orders.count"],
            "sorts": [{ "field": "Orders.Created_At[Week]" }]
        }}))
        .expect("omni source");
        let DataSource::Omni(s) = data else {
            panic!("not omni");
        };
        assert_eq!(s.fields, ["Orders.Created_At[week]", "orders.count"]);
        assert_eq!(s.sorts[0].field, "Orders.Created_At[week]");
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn omni_source_refuses_unknown_keys() {
        let typo = serde_json::from_value::<DataSource>(json!({
            "omni": { "topic": "orders", "fields": ["orders.count"], "filter": "x" }
        }));
        assert!(typo.is_err());
    }

    #[test]
    fn a_plain_source_validates_with_the_default_limit() {
        let s = source(&["orders.created_at[week]", "orders.count"]);
        assert_eq!(s.validate(), Ok(()));
        assert_eq!(s.effective_limit(), 5_000);
    }

    #[test]
    fn hostile_topic_names_are_refused() {
        for name in [
            "",
            "../models",
            "orders/../x",
            "a b",
            "-x",
            "orders\u{1b}[2J",
            "o?x=1",
        ] {
            assert!(check_topic_name(name).is_err(), "{name:?}");
        }
        assert!(check_topic_name(&"a".repeat(201)).is_err());
        assert!(check_topic_name("order_items-v2").is_ok());
    }

    #[test]
    fn hostile_field_names_are_refused() {
        for name in [
            "count",
            "orders.",
            ".count",
            "orders..count",
            "orders.count[week",
            "orders.count[we ek]",
            "orders.count\u{7}",
            "orders.\u{202E}tnuoc",
            "orders.count; DROP",
        ] {
            assert!(check_field_name(name).is_err(), "{name:?}");
        }
        assert!(check_field_name("main__customers.users.created_at[month]").is_ok());
    }

    #[test]
    fn a_refused_name_is_never_echoed() {
        let mut s = source(&["orders.count"]);
        s.fields.push("orders.\u{1b}]52;c;eA==\u{7}".into());
        let error = s.validate().expect_err("hostile field");
        assert_eq!(error.key(), "data.omni.fields[1]");
        assert!(!error.to_string().contains('\u{1b}'));
    }

    #[test]
    fn sorts_must_name_fetched_fields_once() {
        let mut s = source(&["orders.week", "orders.count"]);
        s.sorts = vec![Sort {
            field: "orders.total".into(),
            desc: true,
        }];
        assert!(
            s.validate()
                .expect_err("unfetched")
                .to_string()
                .contains("not in")
        );
        s.sorts = vec![
            Sort {
                field: "orders.count".into(),
                desc: true,
            },
            Sort {
                field: "orders.count".into(),
                desc: false,
            },
        ];
        assert!(
            s.validate()
                .expect_err("twice")
                .to_string()
                .contains("twice")
        );
    }

    #[test]
    fn limits_and_field_counts_are_bounded() {
        let mut s = source(&["orders.count"]);
        s.limit = Some(0);
        assert!(s.validate().is_err());
        s.limit = Some(10_001);
        assert!(s.validate().is_err());
        s.limit = Some(10_000);
        assert!(s.validate().is_ok());
        assert!(source(&[]).validate().is_err());
        let many: Vec<String> = (0..17).map(|i| format!("orders.f{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert!(source(&refs).validate().is_err());
        assert!(
            source(&["orders.count", "orders.count"])
                .validate()
                .is_err()
        );
    }

    #[test]
    fn a_table_types_each_column() {
        let table = Table::from_json_rows(
            rows(json!([
                { "orders.week": "W1", "orders.count": 412, "orders.open": true },
                { "orders.week": "W2", "orders.count": null, "orders.open": false },
            ])),
            Some(&[
                "orders.week".into(),
                "orders.count".into(),
                "orders.open".into(),
            ]),
            "omni.query",
        )
        .expect("table");
        let kinds: Vec<ColumnKind> = table.columns().iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            [ColumnKind::Text, ColumnKind::Number, ColumnKind::Bool]
        );
        assert_eq!(table.rows()[1][1], Cell::Null);
        assert_eq!(table.rows()[0][1], Cell::Number(412.0));
        assert_eq!(table.column_index("orders.open"), Some(2));
    }

    #[test]
    fn a_table_refuses_mixed_kinds_without_echoing_values() {
        let error = Table::from_json_rows(
            rows(json!([{ "a": 1 }, { "a": "secret-value" }])),
            None,
            "data.rows",
        )
        .expect_err("mixed");
        assert_eq!(error.key(), "data.rows[1].a");
        assert!(!error.to_string().contains("secret-value"));
    }

    #[test]
    fn a_table_refuses_hostile_and_huge_text() {
        let escape = Table::from_json_rows(
            rows(json!([{ "a": "fine" }, { "a": "\u{1b}]8;;https://evil.test\u{1b}\\" }])),
            None,
            "data.rows",
        )
        .expect_err("escape");
        assert!(escape.to_string().contains("control character"));
        assert!(!escape.to_string().contains("evil.test"));
        let huge = Table::from_json_rows(
            rows(json!([{ "a": "x".repeat(MAX_TEXT_CHARS + 1) }])),
            None,
            "data.rows",
        )
        .expect_err("huge");
        assert!(huge.to_string().contains("over 1000"));
    }

    #[test]
    fn a_table_refuses_nested_values_missing_and_extra_keys() {
        let columns = ["a".to_owned(), "b".to_owned()];
        for bad in [
            json!([{ "a": [1], "b": 1 }]),
            json!([{ "a": { "x": 1 }, "b": 1 }]),
            json!([{ "a": 1 }]),
            json!([{ "a": 1, "b": 2, "c": 3 }]),
            json!([{ "a": 1, "c": 2 }]),
        ] {
            assert!(
                Table::from_json_rows(rows(bad.clone()), Some(&columns), "q").is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_table_refuses_hostile_column_names_and_too_many_rows() {
        let hostile = Table::from_json_rows(rows(json!([{ "a\u{1b}[31m": 1 }])), None, "data.rows")
            .expect_err("hostile name");
        assert!(!hostile.to_string().contains('\u{1b}'));
        let many = vec![Map::new(); MAX_ROWS + 1];
        assert!(Table::from_json_rows(many, None, "data.rows").is_err());
    }

    #[test]
    fn an_empty_result_is_a_table_with_typed_empty_columns() {
        let table = Table::from_json_rows(vec![], Some(&["orders.count".into()]), "omni.query")
            .expect("empty table");
        assert_eq!(table.rows().len(), 0);
        assert_eq!(table.columns()[0].kind, ColumnKind::Empty);
    }
}
