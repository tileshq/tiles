//! The Omni client: shared models, their topics, a topic's fields, and a topic
//! query that returns a validated [`Table`].

use std::collections::HashSet;
use std::fmt::{self, Write as _};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use tiles_intent::source::{OmniSource, Table, check_field_name, check_topic_name};
use tiles_intent::text::sanitize_truncated;

use crate::api::{self, schema};
use crate::config::{ModelId, OmniConfig, is_uuid};
use crate::error::{Endpoint, OmniError};
use crate::http::{CALL_BUDGET, Http};

/// Largest models or topics listing Tiles reads.
pub const LIST_CAP: u64 = 4 * 1024 * 1024;
/// Largest topic description Tiles reads.
pub const TOPIC_CAP: u64 = 8 * 1024 * 1024;
/// Largest query result Tiles reads: 10,000 rows of 16 short columns fit.
pub const QUERY_CAP: u64 = 16 * 1024 * 1024;
/// Largest `GET /v1/query/wait` response Tiles reads.
const WAIT_CAP: u64 = 1024 * 1024;
/// Most model pages read: 20 pages of 100 models.
const MAX_MODEL_PAGES: usize = 20;
/// Most `GET /v1/query/wait` rounds for one query.
const MAX_WAITS: usize = 10;
/// Most jobs a `408` may leave running.
const MAX_JOBS: usize = 16;

/// A shared Omni model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// The model id.
    pub id: ModelId,
    /// The model name as Omni sends it, not yet made plain.
    pub name: Option<String>,
}

/// A topic of a shared model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    /// The model that holds the topic.
    pub model: ModelId,
    /// The topic name; always passes [`check_topic_name`].
    pub name: String,
    /// The label as Omni sends it, not yet made plain.
    pub label: Option<String>,
    /// The description as Omni sends it, not yet made plain.
    pub description: Option<String>,
}

/// The topics Tiles found, and how many it left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicListing {
    /// Visible topics with names Tiles can query, sorted by name.
    pub topics: Vec<Topic>,
    /// Topics left out because their names hold characters Tiles refuses.
    pub skipped: usize,
}

/// Whether a field groups rows or aggregates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// A column to group by, such as a date or a region.
    Dimension,
    /// An aggregate, such as a count or a sum.
    Measure,
}

/// A field a topic exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The fully qualified name; always passes [`check_field_name`].
    pub name: String,
    /// Dimension or measure.
    pub kind: FieldKind,
    /// The label as Omni sends it, not yet made plain.
    pub label: Option<String>,
    /// Omni's data type, such as `STRING`, `TIMESTAMP` or `NUMBER`, not yet made plain.
    pub data_type: Option<String>,
    /// The aggregate of a measure, such as `SUM`, not yet made plain.
    pub aggregate: Option<String>,
    /// The description as Omni sends it, not yet made plain.
    pub description: Option<String>,
    /// Timeframes a date dimension offers, lowercased: `name[week]` asks for one.
    pub time_frames: Vec<String>,
}

/// A topic's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicFields {
    /// The model that holds the topic.
    pub model: ModelId,
    /// The topic name.
    pub topic: String,
    /// The topic label as Omni sends it.
    pub label: Option<String>,
    /// The topic's base view, which the query names as its table.
    pub base_view: Option<String>,
    /// Visible fields with names Tiles can query, dimensions first.
    pub fields: Vec<Field>,
    /// Fields left out because their names hold characters Tiles refuses.
    pub skipped: usize,
}

impl TopicFields {
    /// Whether `name` is a field of the topic, or a lowercase timeframe of one
    /// of its date dimensions (`orders.created_at[week]`).
    #[must_use]
    pub fn offers(&self, name: &str) -> bool {
        let (base, frame) = match name.split_once('[') {
            Some((base, rest)) => (base, rest.strip_suffix(']')),
            None => (name, None),
        };
        self.fields.iter().any(|field| match frame {
            None => field.name == name,
            Some(frame) => {
                field.name == name
                    || (field.name == base && field.time_frames.iter().any(|f| f == frame))
            }
        })
    }
}

/// Talks to one Omni instance. Every public call finishes, retries and polling
/// included, within 60 s (ADR-008).
pub struct OmniClient {
    http: Http,
    model: Option<ModelId>,
    budget: Duration,
}

impl fmt::Debug for OmniClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OmniClient")
            .field("base", self.http.base())
            .field("key", self.http.key())
            .field("model", &self.model)
            .field("budget", &self.budget)
            .finish()
    }
}

impl OmniClient {
    /// A client for the instance `config` names. Opens no connection yet.
    #[must_use]
    pub fn new(config: OmniConfig) -> Self {
        Self {
            http: Http::new(config.base, config.key),
            model: config.model,
            budget: CALL_BUDGET,
        }
    }

    /// The same client with a shorter time budget per call; never longer than
    /// the 60 s ADR-008 sets.
    #[must_use]
    pub fn with_budget(mut self, budget: Duration) -> Self {
        self.budget = budget.min(CALL_BUDGET);
        self
    }

    /// The shared models, or only the configured one.
    ///
    /// # Errors
    ///
    /// When Omni cannot be reached, refuses the key, or answers in a shape
    /// Omni's docs do not describe.
    pub fn models(&self) -> Result<Vec<Model>, OmniError> {
        self.models_by(self.deadline())
    }

    /// Every visible topic of the models [`OmniClient::models`] returns.
    ///
    /// # Errors
    ///
    /// As [`OmniClient::models`].
    pub fn topics(&self) -> Result<TopicListing, OmniError> {
        let deadline = self.deadline();
        let mut listing = TopicListing {
            topics: Vec::new(),
            skipped: 0,
        };
        for model in self.models_by(deadline)? {
            let part = self.topics_in(&model.id, deadline)?;
            listing.topics.extend(part.topics);
            listing.skipped += part.skipped;
        }
        listing.topics.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then(a.model.as_str().cmp(b.model.as_str()))
        });
        Ok(listing)
    }

    /// The fields of `topic`.
    ///
    /// # Errors
    ///
    /// When the name is not a plain topic name, no model or more than one has
    /// the topic, or as [`OmniClient::models`].
    pub fn fields(&self, topic: &str) -> Result<TopicFields, OmniError> {
        let deadline = self.deadline();
        check_topic(topic)?;
        let model = self.model_of(topic, deadline)?;
        self.topic_fields(&model, topic, deadline)
    }

    /// Runs the query `source` describes and returns its rows as a [`Table`]
    /// whose columns are `source.fields`, in order.
    ///
    /// The query carries the topic's own default filters and no others
    /// (filters are stage 2, ADR-001).
    ///
    /// # Errors
    ///
    /// When `source` is invalid, names a field the topic lacks, Omni fails the
    /// query, the rows break a limit or a plain text rule, or as
    /// [`OmniClient::models`].
    pub fn query(&self, source: &OmniSource) -> Result<Table, OmniError> {
        let deadline = self.deadline();
        source.validate()?;
        let model = self.model_of(&source.topic, deadline)?;
        let topic = self.topic_fields(&model, &source.topic, deadline)?;
        if let Some(field) = source.fields.iter().find(|f| !topic.offers(f)) {
            return Err(OmniError::UnknownField {
                topic: topic.topic,
                field: field.clone(),
            });
        }
        let body = query_body(&model, &topic, source);
        let body = serde_json::to_vec(&body).unwrap_or_default();
        // A slow query answers 408 with the jobs still running. Tiles waits
        // for them, then runs the query once more, which reads the finished
        // result (https://docs.omni.co/api/queries/run-query.md).
        for run in 0..2 {
            let reply = self.http.post_json(
                Endpoint::RunQuery,
                "/v1/query/run",
                &body,
                QUERY_CAP,
                deadline,
            )?;
            if reply.status != 408 {
                return rows(&reply.body, source);
            }
            if run == 1 {
                break;
            }
            let timeout: api::QueryTimeout = api::parse(Endpoint::RunQuery, &reply.body)?;
            self.wait_for(timeout.remaining_job_ids, deadline)?;
        }
        Err(OmniError::StillRunning)
    }

    fn deadline(&self) -> Instant {
        Instant::now() + self.budget
    }

    fn models_by(&self, deadline: Instant) -> Result<Vec<Model>, OmniError> {
        if let Some(model) = &self.model {
            return Ok(vec![Model {
                id: model.clone(),
                name: None,
            }]);
        }
        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_MODEL_PAGES {
            // modelKind=SHARED: the governed models people build on.
            let mut path = "/v1/models?modelKind=SHARED&pageSize=100".to_owned();
            if let Some(cursor) = &cursor {
                path.push_str("&cursor=");
                path.push_str(&encode_component(cursor));
            }
            let reply = self
                .http
                .get(Endpoint::ListModels, &path, LIST_CAP, deadline)?;
            let page: api::ModelsPage = api::parse(Endpoint::ListModels, &reply.body)?;
            for (i, record) in page.records.into_iter().enumerate() {
                if record.deleted_at.is_some() {
                    continue;
                }
                if !is_uuid(&record.id) {
                    let path = format!("records[{i}].id");
                    return Err(schema(Endpoint::ListModels, &path, "not a UUID".to_owned()));
                }
                let id = ModelId::parse(&record.id)?;
                models.push(Model {
                    id,
                    name: record.name,
                });
            }
            match (page.page_info.has_next_page, page.page_info.next_cursor) {
                (true, Some(next)) if !next.is_empty() && next.len() <= 1_024 => {
                    cursor = Some(next);
                }
                (true, _) => {
                    let path = "pageInfo.nextCursor";
                    return Err(schema(
                        Endpoint::ListModels,
                        path,
                        "missing or too long".to_owned(),
                    ));
                }
                (false, _) => return Ok(models),
            }
        }
        Ok(models)
    }

    fn topics_in(&self, model: &ModelId, deadline: Instant) -> Result<TopicListing, OmniError> {
        let path = format!("/v1/models/{model}/topic");
        let reply = self
            .http
            .get(Endpoint::ListTopics, &path, LIST_CAP, deadline)?;
        let list: api::TopicList = api::parse(Endpoint::ListTopics, &reply.body)?;
        let mut listing = TopicListing {
            topics: Vec::new(),
            skipped: 0,
        };
        for entry in list.topics {
            if entry.hidden == Some(true) {
                continue;
            }
            if check_topic_name(&entry.name).is_err() {
                listing.skipped += 1;
                continue;
            }
            listing.topics.push(Topic {
                model: model.clone(),
                name: entry.name,
                label: entry.label,
                description: entry.description,
            });
        }
        Ok(listing)
    }

    /// The one model that has `topic`.
    fn model_of(&self, topic: &str, deadline: Instant) -> Result<ModelId, OmniError> {
        if let Some(model) = &self.model {
            return Ok(model.clone());
        }
        let mut holders = Vec::new();
        for model in self.models_by(deadline)? {
            if self
                .topics_in(&model.id, deadline)?
                .topics
                .iter()
                .any(|t| t.name == topic)
            {
                holders.push(model.id);
            }
        }
        match holders.len() {
            0 => Err(OmniError::UnknownTopic {
                topic: topic.to_owned(),
            }),
            1 => Ok(holders.remove(0)),
            _ => Err(OmniError::AmbiguousTopic {
                topic: topic.to_owned(),
                models: holders.iter().map(ToString::to_string).collect(),
            }),
        }
    }

    fn topic_fields(
        &self,
        model: &ModelId,
        topic: &str,
        deadline: Instant,
    ) -> Result<TopicFields, OmniError> {
        // `topic` passed check_topic_name, so it is safe in a path as is.
        let path = format!("/v1/models/{model}/topic/{topic}");
        let reply = match self
            .http
            .get(Endpoint::GetTopic, &path, TOPIC_CAP, deadline)
        {
            Err(OmniError::Status { status: 404, .. }) => {
                return Err(OmniError::UnknownTopic {
                    topic: topic.to_owned(),
                });
            }
            other => other?,
        };
        let body: api::TopicResponse = api::parse(Endpoint::GetTopic, &reply.body)?;
        if body.topic.name != topic {
            let problem = "names a different topic than the one asked for".to_owned();
            return Err(schema(Endpoint::GetTopic, "topic.name", problem));
        }
        let mut dimensions = Vec::new();
        let mut measures = Vec::new();
        let mut seen = HashSet::new();
        let mut skipped = 0;
        for view in body.topic.views {
            for (kind, entries) in [
                (FieldKind::Dimension, view.dimensions),
                (FieldKind::Measure, view.measures),
            ] {
                for entry in entries {
                    if entry.hidden == Some(true) || entry.ignored == Some(true) {
                        continue;
                    }
                    let name = entry.fully_qualified_name.clone().or_else(|| {
                        let field = entry.field_name.as_deref()?;
                        let view = entry.view_name.as_deref().unwrap_or(&view.name);
                        Some(format!("{view}.{field}"))
                    });
                    let Some(name) = name.filter(|n| check_field_name(n).is_ok()) else {
                        skipped += 1;
                        continue;
                    };
                    if !seen.insert(name.clone()) {
                        continue;
                    }
                    let time_frames = entry
                        .time_frames
                        .iter()
                        .map(|f| f.to_ascii_lowercase())
                        .filter(|f| check_field_name(&format!("v.f[{f}]")).is_ok())
                        .collect();
                    let field = Field {
                        name,
                        kind,
                        label: entry.label,
                        data_type: entry.data_type,
                        aggregate: entry.aggregate_type,
                        description: entry.description,
                        time_frames,
                    };
                    match kind {
                        FieldKind::Dimension => dimensions.push(field),
                        FieldKind::Measure => measures.push(field),
                    }
                }
            }
        }
        dimensions.extend(measures);
        Ok(TopicFields {
            model: model.clone(),
            topic: body.topic.name,
            label: body.topic.label,
            base_view: body.topic.base_view_name,
            fields: dimensions,
            skipped,
        })
    }

    /// Polls `GET /v1/query/wait` until Omni reports no job still running.
    fn wait_for(&self, mut jobs: Vec<String>, deadline: Instant) -> Result<(), OmniError> {
        for _ in 0..MAX_WAITS {
            if jobs.is_empty() {
                return Ok(());
            }
            if jobs.len() > MAX_JOBS || !jobs.iter().all(|j| is_uuid(j)) {
                let problem = format!("not up to {MAX_JOBS} job UUIDs");
                return Err(schema(Endpoint::WaitForQuery, "remaining_job_ids", problem));
            }
            // completionOnly=true: completed jobs come back without their
            // Arrow result, which Tiles does not read.
            let path = format!(
                "/v1/query/wait?jobIds={}&completionOnly=true",
                jobs.join(",")
            );
            let reply = self
                .http
                .get(Endpoint::WaitForQuery, &path, WAIT_CAP, deadline)?;
            let text = std::str::from_utf8(&reply.body)
                .map_err(|_| schema(Endpoint::WaitForQuery, ".", "not UTF-8 text".to_owned()))?;
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            let Some((footer, job_lines)) = lines.split_last() else {
                return Err(schema(Endpoint::WaitForQuery, ".", "empty".to_owned()));
            };
            for line in job_lines {
                let job: api::WaitJob = api::parse(Endpoint::WaitForQuery, line.as_bytes())?;
                if matches!(job.status.as_str(), "ERROR" | "FAILED" | "MISSING") {
                    let kind = self
                        .http
                        .key()
                        .redact(&job.error_type.unwrap_or(job.status));
                    return Err(OmniError::QueryFailed {
                        kind: sanitize_truncated(&kind, 40),
                    });
                }
            }
            let footer: api::WaitFooter = api::parse(Endpoint::WaitForQuery, footer.as_bytes())?;
            if !footer.timed_out.is_true() && footer.remaining_job_ids.is_empty() {
                return Ok(());
            }
            jobs = footer.remaining_job_ids;
        }
        Err(OmniError::StillRunning)
    }
}

fn check_topic(topic: &str) -> Result<(), OmniError> {
    check_topic_name(topic).map_err(|problem| {
        OmniError::Data(tiles_intent::source::DataError::new(
            "topic",
            problem,
            "use a topic name exactly as `tiles omni topics` lists it",
        ))
    })
}

/// The `POST /v1/query/run` body (<https://docs.omni.co/api/queries/run-query.md>):
/// `table` is the topic's base view and `join_paths_from_topic_name` the topic,
/// as in the documented example; `resultType: json` returns one array of row
/// objects keyed by field name; `formatResults: false` keeps numbers as numbers.
fn query_body(model: &ModelId, topic: &TopicFields, source: &OmniSource) -> Value {
    let sorts: Vec<Value> = source
        .sorts
        .iter()
        .map(|s| json!({ "column_name": s.field, "sort_descending": s.desc }))
        .collect();
    json!({
        "query": {
            "modelId": model.as_str(),
            "table": topic.base_view.as_deref().unwrap_or(&topic.topic),
            "join_paths_from_topic_name": topic.topic,
            "fields": source.fields,
            "sorts": sorts,
            "limit": source.effective_limit(),
        },
        "resultType": "json",
        "formatResults": false,
    })
}

/// Reads a `resultType: json` body into a table whose columns are the fields
/// asked for. Tabs and line breaks in text become spaces; any other control
/// character refuses the rows (ADR-009).
fn rows(body: &[u8], source: &OmniSource) -> Result<Table, OmniError> {
    let mut rows: Vec<Map<String, Value>> = api::parse(Endpoint::RunQuery, body)?;
    if rows.len() > source.effective_limit() as usize {
        let problem = format!("more rows than the limit of {}", source.effective_limit());
        return Err(schema(Endpoint::RunQuery, ".", problem));
    }
    for row in &mut rows {
        for value in row.values_mut() {
            if let Value::String(text) = value
                && text.contains(['\t', '\n', '\r'])
            {
                *text = text.replace(['\t', '\n', '\r'], " ");
            }
        }
    }
    Ok(Table::from_json_rows(
        rows,
        Some(&source.fields),
        "omni rows",
    )?)
}

/// Percent-encodes everything but the URL-unreserved characters.
fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Field, FieldKind, TopicFields, encode_component};
    use crate::config::ModelId;

    #[test]
    fn cursors_are_percent_encoded() {
        assert_eq!(encode_component("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(encode_component("abc-_.~"), "abc-_.~");
    }

    #[test]
    fn timeframes_of_date_dimensions_are_offered() {
        let topic = TopicFields {
            model: ModelId::parse("4f8c2a1e-0b7d-4c55-9a3e-2d1f6b8e9c01").expect("id"),
            topic: "orders".into(),
            label: None,
            base_view: None,
            fields: vec![Field {
                name: "orders.created_at".into(),
                kind: FieldKind::Dimension,
                label: None,
                data_type: None,
                aggregate: None,
                description: None,
                time_frames: vec!["week".into()],
            }],
            skipped: 0,
        };
        assert!(topic.offers("orders.created_at"));
        assert!(topic.offers("orders.created_at[week]"));
        assert!(!topic.offers("orders.created_at[WEEK]"));
        assert!(!topic.offers("orders.created_at[year]"));
        assert!(!topic.offers("orders.count"));
    }
}
