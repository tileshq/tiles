//! Errors from the Omni source. Each names the endpoint and what went wrong, so
//! a user can report a mismatch between Omni's docs and its real API without
//! sharing data (ADR-008). None holds the key, a row value or raw response text.

use std::fmt;

use tiles_intent::source::DataError;

use crate::config::ConfigError;

/// An Omni API endpoint, by its documented method and path template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// `GET /v1/models` (<https://docs.omni.co/api/models/list-models.md>).
    ListModels,
    /// `GET /v1/models/{modelId}/topic` (<https://docs.omni.co/api/topics/list-topics.md>).
    ListTopics,
    /// `GET /v1/models/{modelId}/topic/{topicName}` (<https://docs.omni.co/api/topics/retrieve-a-topic.md>).
    GetTopic,
    /// `POST /v1/query/run` (<https://docs.omni.co/api/queries/run-query.md>).
    RunQuery,
    /// `GET /v1/query/wait` (<https://docs.omni.co/api/queries/wait-for-query-results.md>).
    WaitForQuery,
}

impl Endpoint {
    /// The method and path template, as Omni's docs write it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ListModels => "GET /v1/models",
            Self::ListTopics => "GET /v1/models/{modelId}/topic",
            Self::GetTopic => "GET /v1/models/{modelId}/topic/{topicName}",
            Self::RunQuery => "POST /v1/query/run",
            Self::WaitForQuery => "GET /v1/query/wait",
        }
    }
}

pub use tiles_net::TransportKind;

fn describe(kind: TransportKind) -> &'static str {
    match kind {
        TransportKind::HostNotFound => "the Omni host name did not resolve; check TILES_OMNI_URL",
        TransportKind::Connect => "could not connect to Omni; check the network or VPN",
        TransportKind::Tls => {
            "the TLS certificate check failed; Tiles verifies certificates with the system trust store"
        }
        TransportKind::Timeout => "Omni did not answer in time",
        TransportKind::TooLarge => {
            "the response is larger than Tiles accepts; lower the limit or fetch fewer fields"
        }
        TransportKind::Protocol => "Omni sent a malformed HTTP response",
        TransportKind::Other => "the connection to Omni failed",
    }
}

/// An error from the Omni source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmniError {
    /// The URL, key or model id could not be read.
    Config(ConfigError),
    /// The request failed below HTTP.
    Transport {
        /// The endpoint called.
        endpoint: Endpoint,
        /// What failed.
        kind: TransportKind,
    },
    /// Omni answered with an error status.
    Status {
        /// The endpoint called.
        endpoint: Endpoint,
        /// The HTTP status.
        status: u16,
        /// Omni's `detail` or `message`, made plain, cut short and redacted.
        detail: Option<String>,
    },
    /// Omni answered with a redirect, which Tiles never follows.
    Redirect {
        /// The endpoint called.
        endpoint: Endpoint,
        /// Whether the redirect pointed at another host.
        other_host: bool,
    },
    /// The response did not match the documented shape.
    Schema {
        /// The endpoint called.
        endpoint: Endpoint,
        /// Where in the response, such as `records[3].id`.
        path: String,
        /// What was wrong there, such as `wrong type`.
        problem: String,
    },
    /// The whole call took longer than its time budget.
    Deadline {
        /// The endpoint being called when time ran out.
        endpoint: Endpoint,
    },
    /// A topic named by the user does not exist in the models Tiles reads.
    UnknownTopic {
        /// The topic name, already checked to be plain.
        topic: String,
    },
    /// The topic exists in more than one model.
    AmbiguousTopic {
        /// The topic name.
        topic: String,
        /// The ids of the models that have it.
        models: Vec<String>,
    },
    /// The query names a field the topic does not expose.
    UnknownField {
        /// The topic name.
        topic: String,
        /// The field name, already checked to be plain.
        field: String,
    },
    /// Omni still had the query running after Tiles waited for it, with time
    /// left in the budget.
    StillRunning,
    /// Omni reported that the query failed.
    QueryFailed {
        /// Omni's error type, such as `SQL`, made plain.
        kind: String,
    },
    /// The source or the returned rows were refused.
    Data(DataError),
}

impl fmt::Display for OmniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(f),
            Self::Transport { endpoint, kind } => {
                write!(f, "{} failed: {}", endpoint.label(), describe(*kind))
            }
            Self::Status {
                endpoint,
                status,
                detail,
            } => {
                let hint = match status {
                    401 => {
                        "; Omni refused the API key, check TILES_OMNI_API_KEY or run tiles omni set-key again"
                    }
                    403 => "; the key lacks permission for this model or topic",
                    404 => "; the model or topic does not exist or the key cannot see it",
                    429 => "; Omni's rate limit is 60 requests a minute, wait and retry",
                    _ => "",
                };
                write!(f, "{} answered {status}{hint}", endpoint.label())?;
                if let Some(detail) = detail {
                    write!(f, " (Omni said: {detail})")?;
                }
                Ok(())
            }
            Self::Redirect {
                endpoint,
                other_host,
            } => {
                let target = if *other_host {
                    "another host"
                } else {
                    "another path"
                };
                write!(
                    f,
                    "{} redirected to {target}; Tiles never follows redirects, check TILES_OMNI_URL",
                    endpoint.label()
                )
            }
            Self::Schema {
                endpoint,
                path,
                problem,
            } => write!(
                f,
                "{} returned a response Tiles cannot read: {problem} at `{path}`; please report the endpoint and path",
                endpoint.label()
            ),
            Self::Deadline { endpoint } => write!(
                f,
                "{} ran past Tiles' 60 s budget; lower the limit or fetch fewer fields",
                endpoint.label()
            ),
            Self::UnknownTopic { topic } => {
                write!(
                    f,
                    "no topic `{topic}` in the Omni models Tiles reads; run `tiles omni topics`"
                )
            }
            Self::AmbiguousTopic { topic, models } => write!(
                f,
                "topic `{topic}` is in {} models ({}); set TILES_OMNI_MODEL_ID to one of them",
                models.len(),
                models.join(", ")
            ),
            Self::UnknownField { topic, field } => write!(
                f,
                "topic `{topic}` has no field `{field}`; run `tiles omni fields {topic}`"
            ),
            Self::StillRunning => write!(
                f,
                "Omni was still running the query after Tiles waited for it; run it again in a minute or lower the limit"
            ),
            Self::QueryFailed { kind } => {
                write!(
                    f,
                    "Omni could not run the query ({kind} error); try it in Omni first"
                )
            }
            Self::Data(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for OmniError {}

impl From<ConfigError> for OmniError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<DataError> for OmniError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}
