//! Omni's documented response shapes (docs.omni.co, fetched 2026-10-17), and
//! the strict reader that turns a response into them or into an error naming
//! the endpoint and the JSON path.
//!
//! Keys Omni documents but Tiles does not use are ignored, so additions on
//! Omni's side do not break Tiles; a key Tiles uses with the wrong type does.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use tiles_intent::text::sanitize_truncated;

use crate::error::{Endpoint, OmniError};

/// Reads `body` as exactly one JSON document of type `T`.
pub(crate) fn parse<T: DeserializeOwned>(endpoint: Endpoint, body: &[u8]) -> Result<T, OmniError> {
    let mut reader = serde_json::Deserializer::from_slice(body);
    let value = serde_path_to_error::deserialize(&mut reader).map_err(|error| {
        let path = error.path().to_string();
        schema(endpoint, &path, problem(error.inner()))
    })?;
    reader.end().map_err(|_| {
        schema(
            endpoint,
            ".",
            "trailing data after the JSON document".to_owned(),
        )
    })?;
    Ok(value)
}

pub(crate) fn schema(endpoint: Endpoint, path: &str, problem: String) -> OmniError {
    OmniError::Schema {
        endpoint,
        path: sanitize_truncated(path, 120),
        problem,
    }
}

/// What a serde error says, without the value it quotes: serde's
/// `invalid type: string "..."` would repeat response data.
fn problem(error: &serde_json::Error) -> String {
    use serde_json::error::Category;
    match error.classify() {
        Category::Syntax => "not valid JSON".to_owned(),
        Category::Eof => "the JSON is cut short".to_owned(),
        Category::Io => "unreadable bytes".to_owned(),
        Category::Data => {
            let message = error.to_string();
            if let Some(rest) = message.strip_prefix("missing field `") {
                let field = rest.split('`').next().unwrap_or_default();
                format!("missing field `{}`", sanitize_truncated(field, 60))
            } else if message.starts_with("invalid type") {
                "wrong type".to_owned()
            } else if message.starts_with("invalid length") {
                "wrong number of items".to_owned()
            } else if message.starts_with("recursion limit") {
                "nested too deeply".to_owned()
            } else {
                "unexpected value".to_owned()
            }
        }
    }
}

/// A list that may come as `null`.
fn nullable_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// `GET /v1/models` (<https://docs.omni.co/api/models/list-models.md>).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelsPage {
    pub page_info: PageInfo,
    #[serde(deserialize_with = "nullable_vec")]
    pub records: Vec<ModelRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageInfo {
    pub has_next_page: bool,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelRecord {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

/// `GET /v1/models/{modelId}/topic` (<https://docs.omni.co/api/topics/list-topics.md>).
#[derive(Debug, Deserialize)]
pub(crate) struct TopicList {
    #[serde(deserialize_with = "nullable_vec")]
    pub topics: Vec<TopicEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TopicEntry {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

/// `GET /v1/models/{modelId}/topic/{topicName}` (<https://docs.omni.co/api/topics/retrieve-a-topic.md>).
#[derive(Debug, Deserialize)]
pub(crate) struct TopicResponse {
    pub topic: TopicBody,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TopicBody {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub base_view_name: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub views: Vec<ViewEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ViewEntry {
    pub name: String,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub dimensions: Vec<FieldEntry>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub measures: Vec<FieldEntry>,
}

/// A dimension or a measure of a view.
#[derive(Debug, Deserialize)]
pub(crate) struct FieldEntry {
    #[serde(default)]
    pub field_name: Option<String>,
    #[serde(default)]
    pub view_name: Option<String>,
    #[serde(default)]
    pub fully_qualified_name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub data_type: Option<String>,
    #[serde(default)]
    pub aggregate_type: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub time_frames: Vec<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub ignored: Option<bool>,
}

/// The `408` body of `POST /v1/query/run` (`QueryTimeoutError`,
/// <https://docs.omni.co/api/queries/run-query.md>).
#[derive(Debug, Deserialize)]
pub(crate) struct QueryTimeout {
    #[serde(deserialize_with = "nullable_vec")]
    pub remaining_job_ids: Vec<String>,
}

/// A job line of `GET /v1/query/wait`
/// (<https://docs.omni.co/api/queries/wait-for-query-results.md>).
#[derive(Debug, Deserialize)]
pub(crate) struct WaitJob {
    pub status: String,
    #[serde(default)]
    pub error_type: Option<String>,
}

/// The footer line of `GET /v1/query/wait`. The docs type `timed_out` as the
/// string `"true"` or `"false"`; a boolean is read too.
#[derive(Debug, Deserialize)]
pub(crate) struct WaitFooter {
    #[serde(deserialize_with = "nullable_vec")]
    pub remaining_job_ids: Vec<String>,
    pub timed_out: Flag,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Flag {
    Bool(bool),
    Text(String),
}

impl Flag {
    pub fn is_true(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Text(t) => t.eq_ignore_ascii_case("true"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelsPage, TopicList, WaitFooter, parse};
    use crate::error::{Endpoint, OmniError};

    #[test]
    fn a_wrong_type_names_the_path_but_not_the_value() {
        let body = br#"{"pageInfo":{"hasNextPage":false},"records":[{"id":"a"},{"id":"secret-row-value-9"}]}"#;
        let ok: ModelsPage = parse(Endpoint::ListModels, body).expect("models");
        assert_eq!(ok.records.len(), 2);
        let body = br#"{"pageInfo":{"hasNextPage":"secret-row-value-9"},"records":[]}"#;
        let error = parse::<ModelsPage>(Endpoint::ListModels, body).expect_err("wrong type");
        let text = error.to_string();
        assert!(text.contains("pageInfo.hasNextPage"), "{text}");
        assert!(text.contains("wrong type"), "{text}");
        assert!(!text.contains("secret-row-value-9"), "{text}");
    }

    #[test]
    fn a_missing_field_is_named() {
        let error = parse::<TopicList>(Endpoint::ListTopics, br#"{"topics":[{"label":"x"}]}"#)
            .expect_err("missing");
        assert!(error.to_string().contains("missing field `name`"));
        assert!(error.to_string().contains("topics[0]"));
    }

    #[test]
    fn broken_json_trailing_data_and_deep_nesting_are_errors_not_panics() {
        for body in [&b"{"[..], b"[]", b"{\"topics\":[]} {}", b"\xff\xfe", b""] {
            assert!(matches!(
                parse::<TopicList>(Endpoint::ListTopics, body),
                Err(OmniError::Schema { .. })
            ));
        }
        let deep = format!(
            "{{\"topics\":{}{}}}",
            "[".repeat(10_000),
            "]".repeat(10_000)
        );
        assert!(parse::<TopicList>(Endpoint::ListTopics, deep.as_bytes()).is_err());
    }

    #[test]
    fn the_wait_footer_reads_string_and_boolean_flags() {
        let text: WaitFooter = parse(
            Endpoint::WaitForQuery,
            br#"{"remaining_job_ids":[],"timed_out":"false"}"#,
        )
        .expect("footer");
        assert!(!text.timed_out.is_true());
        let flag: WaitFooter = parse(
            Endpoint::WaitForQuery,
            br#"{"remaining_job_ids":null,"timed_out":true}"#,
        )
        .expect("footer");
        assert!(flag.timed_out.is_true());
    }

    #[test]
    fn hostile_paths_are_made_plain() {
        let body = b"{\"topics\":[{\"name\":\"a\",\"label\":{\"\x1b]52;c;eA==\x07\":1}}]}";
        let error = parse::<TopicList>(Endpoint::ListTopics, body).expect_err("object label");
        assert!(!error.to_string().contains('\u{1b}'));
    }
}
