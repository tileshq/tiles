//! The one HTTP path to Omni (ADR-008), over the client every source shares
//! (`tiles-net`, ADR-031): HTTPS only with the system trust store, a 10 s
//! connect timeout inside one 60 s budget per call, at most three attempts
//! for rate limits and gateway errors, a size cap on every response, and no
//! redirects.

use std::time::{Duration, Instant};

use tiles_intent::text::sanitize_truncated;
pub(crate) use tiles_net::Reply;
use tiles_net::{Client, Failure, Request};

use crate::config::BaseUrl;
use crate::error::{Endpoint, OmniError};
use crate::secret::ApiKey;

pub use tiles_net::{CONNECT_TIMEOUT, MAX_ATTEMPTS};
/// Time for one whole client call, retries and `408` polling included (ADR-008).
pub const CALL_BUDGET: Duration = Duration::from_secs(60);
/// Longest Omni error detail Tiles repeats, in characters.
const DETAIL_CHARS: usize = 200;

pub(crate) struct Http {
    client: Client,
    base: BaseUrl,
    key: ApiKey,
}

impl Http {
    pub fn new(base: BaseUrl, key: ApiKey) -> Self {
        let user_agent = format!("tiles/{}", tiles_intent::TILES_VERSION);
        Self {
            client: Client::new(&user_agent, base.is_loopback_http()),
            base,
            key,
        }
    }

    pub fn base(&self) -> &BaseUrl {
        &self.base
    }

    pub fn key(&self) -> &ApiKey {
        &self.key
    }

    pub fn get(
        &self,
        endpoint: Endpoint,
        path: &str,
        cap: u64,
        deadline: Instant,
    ) -> Result<Reply, OmniError> {
        self.send(endpoint, path, None, cap, deadline)
    }

    pub fn post_json(
        &self,
        endpoint: Endpoint,
        path: &str,
        body: &[u8],
        cap: u64,
        deadline: Instant,
    ) -> Result<Reply, OmniError> {
        self.send(endpoint, path, Some(body), cap, deadline)
    }

    fn send(
        &self,
        endpoint: Endpoint,
        path: &str,
        body: Option<&[u8]>,
        cap: u64,
        deadline: Instant,
    ) -> Result<Reply, OmniError> {
        let url = self.base.join(path);
        let auth = crate::secret::Bearer::new(&self.key);
        let headers = [("Authorization", auth.as_str())];
        let request = Request {
            endpoint,
            url: &url,
            headers: &headers,
            body: body.map(|body| ("application/json", body)),
            cap,
            poll_status: (endpoint == Endpoint::RunQuery).then_some(408),
        };
        self.client
            .send(&request, deadline)
            .map_err(|failure| match failure {
                Failure::Transport { endpoint, kind } => OmniError::Transport { endpoint, kind },
                Failure::Status {
                    endpoint,
                    status,
                    body,
                } => OmniError::Status {
                    endpoint,
                    status,
                    detail: body.and_then(|body| self.detail(&body)),
                },
                Failure::Redirect {
                    endpoint,
                    other_host,
                } => OmniError::Redirect {
                    endpoint,
                    other_host,
                },
                Failure::Deadline { endpoint } => OmniError::Deadline { endpoint },
            })
    }

    /// Omni's error text from a body like `{"detail": "..."}` or
    /// `{"error": "...", "message": "..."}`, made plain, cut short and redacted.
    fn detail(&self, body: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(body).ok()?;
        let text = ["detail", "message"]
            .iter()
            .find_map(|k| value.get(k)?.as_str())?;
        let plain = sanitize_truncated(&self.key.redact(text), DETAIL_CHARS);
        (!plain.trim().is_empty()).then_some(plain)
    }
}
