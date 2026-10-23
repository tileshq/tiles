//! The warehouse source (ADR-031): one `GoogleSQL` `SELECT` the AI writes,
//! CTEs and joins allowed, with dbt models written as dbt writes them,
//! `{{ ref('fct_payments') }}`, and the build they are read from.
//!
//! This module checks the text and reads the model references; it asks the
//! warehouse nothing. `tiles-warehouse` resolves each reference to its table
//! for the target through the dbt reader, puts the table in its place with
//! [`WarehouseSource::render`], and guards and runs the query.

use serde::{Deserialize, Serialize};

use super::{DEFAULT_OMNI_LIMIT, DataError, MAX_ROWS};

/// Most bytes of SQL a warehouse source takes.
pub const MAX_SQL_BYTES: usize = 256 * 1024;
/// Most `{{ ref(...) }}` placeholders one query takes.
pub const MAX_REFERENCES: usize = 64;
/// Longest model, package or version name in a reference, in bytes.
const MAX_REF_NAME: usize = 256;

/// The fix every refused placeholder names.
const REF_FIX: &str = "write a dbt model as {{ ref('fct_x') }} or {{ ref('package', 'fct_x', v=2) }}, and any other table by its full name, `project.dataset.table`";

/// A warehouse query: the SQL, the dbt build its model references are read
/// from, and how many rows to take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarehouseSource {
    /// One `GoogleSQL` `SELECT`. A dbt model is written
    /// `{{ ref('model') }}`; any other table by its full name.
    pub sql: String,
    /// The dbt build the references are read from: `prod`, `dev`,
    /// `mr:<iid>` or `mr:<iid>:full`. Needed when the SQL has a reference,
    /// refused when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Most rows to fetch; [`DEFAULT_OMNI_LIMIT`] when absent, at most
    /// [`MAX_ROWS`], as for Omni.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// One `{{ ref(...) }}` in a query, and where it sits in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelReference {
    /// The byte range of the whole placeholder, braces included.
    pub span: std::ops::Range<usize>,
    /// The package, for the two-name form `ref('package', 'model')`.
    pub package: Option<String>,
    /// The model's name.
    pub name: String,
    /// The version asked for with `v=` or `version=`, without a `v`.
    pub version: Option<String>,
}

impl ModelReference {
    /// The model as the dbt reader names it: `fct_x`, `fct_x.v2`, or
    /// `model.package.fct_x` with an optional `.v2`.
    #[must_use]
    pub fn model_name(&self) -> String {
        let mut out = String::new();
        if let Some(package) = &self.package {
            out.push_str("model.");
            out.push_str(package);
            out.push('.');
        }
        out.push_str(&self.name);
        if let Some(version) = &self.version {
            out.push_str(".v");
            out.push_str(version);
        }
        out
    }
}

impl WarehouseSource {
    /// Checks the SQL's text, its model references, the target and the
    /// limit, without asking the warehouse anything.
    ///
    /// # Errors
    ///
    /// The first problem found, naming the key and the fix.
    pub fn validate(&self) -> Result<(), DataError> {
        check_sql(&self.sql).map_err(|problem| {
            DataError::new(
                "data.warehouse.sql",
                problem,
                "send one GoogleSQL SELECT in plain text, at most 256 KiB",
            )
        })?;
        let references = self.references()?;
        match (&self.target, references.is_empty()) {
            (None, false) => {
                return Err(DataError::new(
                    "data.warehouse.target",
                    "is missing, and the query reads dbt models",
                    "say which dbt build to read: prod, dev, mr:<iid> or mr:<iid>:full; if the user has not said, ask them",
                ));
            }
            (Some(_), true) => {
                return Err(DataError::new(
                    "data.warehouse.target",
                    "names a dbt build, but the query has no {{ ref('model') }}",
                    "write each dbt model as {{ ref('model') }} so Tiles reads it from that build, or leave target out for tables named in full",
                ));
            }
            (Some(target), false) => check_target(target).map_err(|problem| {
                DataError::new(
                    "data.warehouse.target",
                    problem,
                    "name prod, dev, mr:<iid> or mr:<iid>:full",
                )
            })?,
            (None, true) => {}
        }
        if let Some(limit) = self.limit
            && (limit == 0 || limit as usize > MAX_ROWS)
        {
            return Err(DataError::new(
                "data.warehouse.limit",
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

    /// The query's `{{ ref(...) }}` placeholders, in order. Like dbt's own
    /// rendering, a placeholder counts wherever it is, in a comment or a
    /// string too, and any other `{{ ... }}` is refused.
    ///
    /// # Errors
    ///
    /// A [`DataError`] on `data.warehouse.sql` for a placeholder Tiles
    /// cannot read, naming where it starts but never quoting it.
    pub fn references(&self) -> Result<Vec<ModelReference>, DataError> {
        let sql = self.sql.as_str();
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(found) = sql[from..].find("{{") {
            let start = from + found;
            let at = sql[..start].chars().count();
            let refuse = |problem: String| DataError::new("data.warehouse.sql", problem, REF_FIX);
            let Some(close) = sql[start + 2..].find("}}") else {
                return Err(refuse(format!(
                    "opens {{{{ at character {at} without closing it"
                )));
            };
            let end = start + 2 + close + 2;
            let inner = &sql[start + 2..end - 2];
            let (package, name, version) = read_ref(inner)
                .map_err(|problem| refuse(format!("{problem} at character {at}")))?;
            if out.len() == MAX_REFERENCES {
                return Err(refuse(format!(
                    "has more than {MAX_REFERENCES} {{{{ ref(...) }}}} placeholders"
                )));
            }
            out.push(ModelReference {
                span: start..end,
                package,
                name,
                version,
            });
            from = end;
        }
        Ok(out)
    }

    /// The SQL with each reference replaced by what `table` gives for it,
    /// such as `` `acme-prod.dbt_marts.fct_payments` ``.
    ///
    /// # Errors
    ///
    /// The first error `table` gives, or [`WarehouseSource::references`]'
    /// as a `From` conversion.
    pub fn render<E: From<DataError>>(
        &self,
        mut table: impl FnMut(&ModelReference) -> Result<String, E>,
    ) -> Result<String, E> {
        let mut out = String::with_capacity(self.sql.len());
        let mut last = 0;
        for reference in self.references()? {
            out.push_str(&self.sql[last..reference.span.start]);
            out.push_str(&table(&reference)?);
            last = reference.span.end;
        }
        out.push_str(&self.sql[last..]);
        Ok(out)
    }
}

/// The package, model and version of the inside of one `{{ ... }}`, read
/// as `ref('model')`, `ref('package', 'model')`, either with `v=` or
/// `version=`, and dbt's `-` whitespace markers at either end.
fn read_ref(inner: &str) -> Result<(Option<String>, String, Option<String>), &'static str> {
    let inner = inner.trim();
    let inner = inner.strip_prefix('-').unwrap_or(inner);
    let inner = inner.strip_suffix('-').unwrap_or(inner).trim();
    let mut cursor = Cursor(inner);
    let call = cursor.word();
    if call == "source" {
        return Err(
            "has a {{ source(...) }}, which Tiles does not read; name the source's table in full",
        );
    }
    if call != "ref" || !cursor.eat('(') {
        return Err("has a {{ ... }} that is not a {{ ref('model') }}");
    }
    let mut names = Vec::new();
    let mut version = None;
    loop {
        if cursor.eat(')') {
            break;
        }
        if !names.is_empty() || version.is_some() {
            if !cursor.eat(',') {
                return Err("has a ref(...) whose arguments are not separated by commas");
            }
            if cursor.eat(')') {
                break;
            }
        }
        if let Some(text) = cursor.quoted()? {
            if version.is_some() {
                return Err("has a ref(...) naming a model after its version");
            }
            names.push(text);
            continue;
        }
        let key = cursor.word();
        if !matches!(key, "v" | "version") || !cursor.eat('=') {
            return Err("has a ref(...) argument other than a quoted name, v= or version=");
        }
        let value = match cursor.quoted()? {
            Some(text) => text,
            None => cursor.word().to_string(),
        };
        if version.replace(value).is_some() {
            return Err("has a ref(...) with two versions");
        }
    }
    if !cursor.rest().is_empty() {
        return Err("has text after its ref(...)");
    }
    let (package, name) = match <[String; 1]>::try_from(names) {
        Ok([name]) => (None, name),
        Err(names) => match <[String; 2]>::try_from(names) {
            Ok([package, name]) => (Some(package), name),
            Err(_) => return Err("has a ref(...) without one or two quoted names"),
        },
    };
    let word = |s: &str, dash: bool| {
        (1..=MAX_REF_NAME).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || (dash && b == b'-'))
    };
    if package.as_deref().is_some_and(|p| !word(p, false)) {
        return Err("has a ref(...) whose package is not letters, digits and _");
    }
    if !word(&name, true) {
        return Err("has a ref(...) whose model is not letters, digits, _ and -");
    }
    if version.as_deref().is_some_and(|v| !word(v, false)) {
        return Err("has a ref(...) whose version is not letters, digits and _");
    }
    Ok((package, name, version))
}

/// A cursor over the inside of a placeholder that skips white space
/// before each token.
struct Cursor<'a>(&'a str);

impl<'a> Cursor<'a> {
    fn rest(&mut self) -> &'a str {
        self.0 = self.0.trim_start();
        self.0
    }

    fn eat(&mut self, c: char) -> bool {
        match self.rest().strip_prefix(c) {
            Some(rest) => {
                self.0 = rest;
                true
            }
            None => false,
        }
    }

    /// Letters, digits and `_`; empty when the next token is not a word.
    fn word(&mut self) -> &'a str {
        let rest = self.rest();
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        self.0 = &rest[end..];
        &rest[..end]
    }

    /// A string in single or double quotes, with no escape in it; `None`
    /// when the next token is not a string.
    fn quoted(&mut self) -> Result<Option<String>, &'static str> {
        let rest = self.rest();
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '\'' | '"')) else {
            return Ok(None);
        };
        let body = &rest[1..];
        let Some(end) = body.find(quote) else {
            return Err("has a ref(...) with a name whose quote never closes");
        };
        if body[..end].contains('\\') {
            return Err("has a ref(...) with a backslash in a name");
        }
        self.0 = &body[end + 1..];
        Ok(Some(body[..end].to_string()))
    }
}

/// Checks a query's text: not blank, at most [`MAX_SQL_BYTES`], and no
/// control character but tab and line breaks and no bidirectional override,
/// which could show a reader different SQL from what runs.
///
/// # Errors
///
/// What is wrong, never quoting the text.
pub fn check_sql(text: &str) -> Result<(), &'static str> {
    if text.trim().is_empty() {
        return Err("is empty");
    }
    if text.len() > MAX_SQL_BYTES {
        return Err("is longer than 256 KiB");
    }
    if text.chars().any(forbidden_in_sql) {
        return Err("holds a control character or a bidirectional override");
    }
    Ok(())
}

fn forbidden_in_sql(c: char) -> bool {
    (c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
        || matches!(
            c,
            '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{2028}' | '\u{2029}'
        )
        || ('\u{202a}'..='\u{202e}').contains(&c)
        || ('\u{2066}'..='\u{2069}').contains(&c)
}

/// Checks a dbt target: `prod`, `dev`, `mr:<iid>` or `mr:<iid>:full`, the
/// MR's number from 1 to 999,999,999. `tiles-warehouse`'s `Target::parse`
/// reads the same text, and its tests hold the two together.
///
/// # Errors
///
/// What is wrong, never quoting the text.
pub fn check_target(text: &str) -> Result<(), &'static str> {
    if matches!(text, "prod" | "dev") {
        return Ok(());
    }
    let Some(rest) = text.strip_prefix("mr:") else {
        return Err("is not a dbt build Tiles knows");
    };
    let iid = rest.strip_suffix(":full").unwrap_or(rest);
    if (1..=9).contains(&iid.len())
        && iid.bytes().all(|b| b.is_ascii_digit())
        && iid.bytes().any(|b| b != b'0')
    {
        Ok(())
    } else {
        Err("names an MR by something other than its number")
    }
}
