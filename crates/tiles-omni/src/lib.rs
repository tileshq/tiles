//! The Omni data source (ADR-008): lists the topics of an Omni instance, reads
//! a topic's fields, and runs a topic query for chosen fields with simple sorts
//! and a limit, returning rows the engine draws.
//!
//! It is one of the crates allowed a network client (ADR-005, ADR-031,
//! `scripts/check-network-owners.py`) and sends through the one client in
//! `tiles-net`. It opens a connection only when a caller asks it for Omni
//! data, and only to the configured instance.
//!
//! Endpoints, from Omni's public API docs (fetched 2026-10-17):
//!
//! | Need | Endpoint |
//! |---|---|
//! | Shared models | `GET /v1/models` (<https://docs.omni.co/api/models/list-models.md>) |
//! | Topics of a model | `GET /v1/models/{modelId}/topic` (<https://docs.omni.co/api/topics/list-topics.md>) |
//! | A topic's views and fields | `GET /v1/models/{modelId}/topic/{topicName}` (<https://docs.omni.co/api/topics/retrieve-a-topic.md>) |
//! | Run a query | `POST /v1/query/run` (<https://docs.omni.co/api/queries/run-query.md>) |
//! | Wait for a slow query | `GET /v1/query/wait` (<https://docs.omni.co/api/queries/wait-for-query-results.md>) |
//!
//! All requests authenticate with `Authorization: Bearer <key>`
//! (<https://docs.omni.co/api/authentication.md>).

mod api;
mod client;
mod config;
mod error;
mod http;
#[cfg(feature = "replay")]
pub mod replay;
mod secret;

pub use client::{
    Field, FieldKind, LIST_CAP, Model, OmniClient, QUERY_CAP, TOPIC_CAP, Topic, TopicFields,
    TopicListing,
};
pub use config::{
    BaseUrl, ConfigError, ENV_API_KEY, ENV_MODEL_ID, ENV_URL, KEYCHAIN_API_KEY, KEYCHAIN_MODEL_ID,
    KEYCHAIN_SERVICE, KEYCHAIN_URL, Keychain, ModelId, OmniConfig, SecretStore, Source,
    keychain_key_account, store_settings,
};
pub use error::{Endpoint, OmniError, TransportKind};
pub use http::{CALL_BUDGET, CONNECT_TIMEOUT, MAX_ATTEMPTS};
pub use secret::{ApiKey, KeyProblem, MAX_KEY_BYTES, wipe};
