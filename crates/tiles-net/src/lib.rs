//! The one HTTP client Tiles' data sources share (ADR-008, ADR-031): HTTPS
//! only with the system trust store, a 10 s connect timeout inside one
//! deadline per call, at most three attempts for connect errors, rate limits
//! and gateway errors, a size cap on every response, and no redirects.
//!
//! It knows no host, key or response shape: each source builds the URL and
//! headers, names the endpoint with its own label type, and reads the body.
//! `tiles-omni` and `tiles-warehouse` are its users and, with it, the only
//! crates allowed a network client (`scripts/check-network-owners.py`).

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Time to open a connection (ADR-008).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Most attempts at one request.
pub const MAX_ATTEMPTS: u32 = 3;
/// Most bytes read from an error response, or from a response with the
/// status the caller polls on.
pub const ERROR_BODY_CAP: u64 = 64 * 1024;
/// Longest wait Tiles honours from a `Retry-After` header.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(5);

/// What failed below HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// The host name did not resolve.
    HostNotFound,
    /// No connection could be made.
    Connect,
    /// The TLS handshake or certificate check failed.
    Tls,
    /// The request ran out of time.
    Timeout,
    /// The response was larger than Tiles accepts from this endpoint.
    TooLarge,
    /// The server spoke broken HTTP.
    Protocol,
    /// Any other failure of the connection.
    Other,
}

/// One request: the endpoint's label, for errors, and what to send.
pub struct Request<'a, E> {
    /// The endpoint, by the source's own label.
    pub endpoint: E,
    /// The full URL.
    pub url: &'a str,
    /// Headers beyond `Accept: application/json`, such as `Authorization`.
    pub headers: &'a [(&'a str, &'a str)],
    /// The content type and body of a `POST`; `None` sends a `GET`.
    pub body: Option<(&'a str, &'a [u8])>,
    /// Most bytes read from a successful response.
    pub cap: u64,
    /// A non-success status the caller polls on, such as Omni's `408`,
    /// returned as a [`Reply`] with its body read under [`ERROR_BODY_CAP`].
    pub poll_status: Option<u16>,
}

/// A response the client accepted: a success, or the status the caller
/// polls on.
#[derive(Debug)]
pub struct Reply {
    /// The HTTP status.
    pub status: u16,
    /// The body, at most the request's cap.
    pub body: Vec<u8>,
}

/// Why a request got no [`Reply`]. None holds a header value; a status
/// error's body is the server's, for the caller to read and clean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure<E> {
    /// The request failed below HTTP.
    Transport {
        /// The endpoint called.
        endpoint: E,
        /// What failed.
        kind: TransportKind,
    },
    /// The server answered with an error status.
    Status {
        /// The endpoint called.
        endpoint: E,
        /// The HTTP status.
        status: u16,
        /// The error body, at most [`ERROR_BODY_CAP`] bytes, when it could
        /// be read.
        body: Option<Vec<u8>>,
    },
    /// The server answered with a redirect, which the client never follows.
    Redirect {
        /// The endpoint called.
        endpoint: E,
        /// Whether the redirect pointed at another host.
        other_host: bool,
    },
    /// The call ran past its deadline.
    Deadline {
        /// The endpoint being called when time ran out.
        endpoint: E,
    },
}

/// An HTTP client with Tiles' transport rules.
pub struct Client {
    agent: Agent,
}

impl Client {
    /// A client that sends `user_agent`. With `loopback_http` it also
    /// speaks plain `http://`, for a source's loopback replay server in
    /// tests, and ignores the environment's proxy.
    #[must_use]
    pub fn new(user_agent: &str, loopback_http: bool) -> Self {
        // ring, not aws-lc-rs: no C build step and a licence deny.toml allows.
        // The platform verifier trusts what macOS trusts, so a company's own
        // root certificate works the way it does in the browser.
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        let config = Agent::config_builder()
            .https_only(!loopback_http)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .max_response_header_size(64 * 1024)
            .user_agent(user_agent)
            .tls_config(tls)
            .proxy(if loopback_http {
                None
            } else {
                ureq::Proxy::try_from_env()
            })
            .build();
        Self {
            agent: Agent::new_with_config(config),
        }
    }

    /// Sends `request`, retrying a connect error or a `429`, `502`, `503`
    /// or `504` while time is left before `deadline`.
    ///
    /// # Errors
    ///
    /// A [`Failure`] naming the request's endpoint.
    pub fn send<E: Copy>(
        &self,
        request: &Request<'_, E>,
        deadline: Instant,
    ) -> Result<Reply, Failure<E>> {
        let endpoint = request.endpoint;
        let mut attempt = 1;
        loop {
            let left = remaining(deadline).ok_or(Failure::Deadline { endpoint })?;
            let sent = match request.body {
                None => {
                    let mut builder = self.agent.get(request.url);
                    for (name, value) in request.headers {
                        builder = builder.header(*name, *value);
                    }
                    builder
                        .header("Accept", "application/json")
                        .config()
                        .timeout_global(Some(left))
                        .build()
                        .call()
                }
                Some((content_type, body)) => {
                    let mut builder = self.agent.post(request.url);
                    for (name, value) in request.headers {
                        builder = builder.header(*name, *value);
                    }
                    builder
                        .header("Accept", "application/json")
                        .header("Content-Type", content_type)
                        .config()
                        .timeout_global(Some(left))
                        .build()
                        .send(body)
                }
            };
            let mut response = match sent {
                Ok(response) => response,
                Err(error) => {
                    let kind = transport_kind(&error);
                    if kind == TransportKind::Timeout && remaining(deadline).is_none() {
                        return Err(Failure::Deadline { endpoint });
                    }
                    if kind == TransportKind::Connect && attempt < MAX_ATTEMPTS {
                        if !pause(backoff(attempt), deadline) {
                            return Err(Failure::Deadline { endpoint });
                        }
                        attempt += 1;
                        continue;
                    }
                    return Err(Failure::Transport { endpoint, kind });
                }
            };
            let status = response.status().as_u16();
            if (300..400).contains(&status) {
                let other_host = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .is_none_or(|location| redirects_off_host(location, authority(request.url)));
                return Err(Failure::Redirect {
                    endpoint,
                    other_host,
                });
            }
            let polled = request.poll_status == Some(status);
            if (200..300).contains(&status) || polled {
                let limit = if polled { ERROR_BODY_CAP } else { request.cap };
                let body = response
                    .body_mut()
                    .with_config()
                    .limit(limit)
                    .read_to_vec()
                    .map_err(|error| read_error(endpoint, &error, deadline))?;
                return Ok(Reply { status, body });
            }
            if matches!(status, 429 | 502 | 503 | 504) && attempt < MAX_ATTEMPTS {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map_or_else(
                        || backoff(attempt),
                        |s| Duration::from_secs(s).min(MAX_RETRY_AFTER),
                    );
                if pause(wait, deadline) {
                    attempt += 1;
                    continue;
                }
            }
            let body = response
                .body_mut()
                .with_config()
                .limit(ERROR_BODY_CAP)
                .read_to_vec()
                .ok();
            return Err(Failure::Status {
                endpoint,
                status,
                body,
            });
        }
    }
}

fn remaining(deadline: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt.saturating_sub(1)))
}

/// Sleeps for `wait` unless that would end past `deadline`.
fn pause(wait: Duration, deadline: Instant) -> bool {
    match remaining(deadline) {
        Some(left) if left > wait => {
            thread::sleep(wait);
            true
        }
        _ => false,
    }
}

/// The host and port of `url`, which the caller built as `scheme://authority/...`.
fn authority(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or_default()
}

/// Whether a `Location` header points anywhere but `authority`. A relative
/// location stays on the host.
fn redirects_off_host(location: &str, authority: &str) -> bool {
    let Some((_, rest)) = location.split_once("//") else {
        return false;
    };
    let target = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !target.eq_ignore_ascii_case(authority)
}

fn transport_kind(error: &ureq::Error) -> TransportKind {
    match error {
        ureq::Error::HostNotFound => TransportKind::HostNotFound,
        // rustls reports a refused handshake, such as an untrusted
        // certificate, as an I/O error wrapping its own error.
        ureq::Error::Io(io)
            if io.kind() == std::io::ErrorKind::InvalidData
                && io
                    .get_ref()
                    .is_some_and(<dyn std::error::Error + Send + Sync>::is::<rustls::Error>) =>
        {
            TransportKind::Tls
        }
        ureq::Error::ConnectionFailed | ureq::Error::Io(_) => TransportKind::Connect,
        ureq::Error::Timeout(_) => TransportKind::Timeout,
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) | ureq::Error::TlsRequired => {
            TransportKind::Tls
        }
        ureq::Error::BodyExceedsLimit(_) => TransportKind::TooLarge,
        ureq::Error::Protocol(_) | ureq::Error::LargeResponseHeader(..) => TransportKind::Protocol,
        _ => TransportKind::Other,
    }
}

fn read_error<E>(endpoint: E, error: &ureq::Error, deadline: Instant) -> Failure<E> {
    let kind = transport_kind(error);
    if kind == TransportKind::Timeout && remaining(deadline).is_none() {
        Failure::Deadline { endpoint }
    } else {
        Failure::Transport { endpoint, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::{authority, backoff, redirects_off_host};
    use std::time::Duration;

    #[test]
    fn redirects_are_judged_by_host() {
        assert!(!redirects_off_host(
            "/api/v1/models?page=2",
            "acme.omniapp.co"
        ));
        assert!(!redirects_off_host(
            "https://ACME.omniapp.co/api",
            "acme.omniapp.co"
        ));
        assert!(redirects_off_host(
            "https://evil.test/api",
            "acme.omniapp.co"
        ));
        assert!(redirects_off_host(
            "https://acme.omniapp.co.evil.test/",
            "acme.omniapp.co"
        ));
        assert!(redirects_off_host("//evil.test/x", "acme.omniapp.co"));
    }

    #[test]
    fn the_authority_is_the_host_and_port_of_the_url() {
        assert_eq!(
            authority("https://acme.omniapp.co/api/v1/models?page=2"),
            "acme.omniapp.co"
        );
        assert_eq!(
            authority("http://127.0.0.1:52011/api/v1/query/run"),
            "127.0.0.1:52011"
        );
        assert_eq!(
            authority("https://oauth2.googleapis.com?x"),
            "oauth2.googleapis.com"
        );
    }

    #[test]
    fn backoff_doubles() {
        assert_eq!(backoff(1), Duration::from_millis(500));
        assert_eq!(backoff(2), Duration::from_millis(1_000));
    }

    /// The update check's record (ADR-032) names every header a `GET`
    /// sends; this holds it to that list.
    #[test]
    fn a_get_sends_the_host_user_agent_and_accept_only() {
        use std::io::{BufRead as _, BufReader, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim_end().is_empty() {
                    break;
                }
                head.push(line.trim_end().to_string());
            }
            let mut stream = stream;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}")
                .unwrap();
            head
        });
        let client = super::Client::new("tiles/9.9.9", true);
        let url = format!("http://127.0.0.1:{port}/repos/o/r/releases/latest");
        let reply = client
            .send(
                &super::Request {
                    endpoint: (),
                    url: &url,
                    headers: &[],
                    body: None,
                    cap: 1024,
                    poll_status: None,
                },
                std::time::Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(reply.body, b"{}");
        let head = server.join().unwrap();
        assert_eq!(head[0], "GET /repos/o/r/releases/latest HTTP/1.1");
        let mut names: Vec<String> = head[1..]
            .iter()
            .map(|l| l.split(':').next().unwrap().to_ascii_lowercase())
            .collect();
        names.sort();
        assert_eq!(names, ["accept", "host", "user-agent"], "{head:?}");
        assert!(
            head.contains(&"user-agent: tiles/9.9.9".to_string()),
            "{head:?}"
        );
        assert!(
            head.contains(&"accept: application/json".to_string()),
            "{head:?}"
        );
    }
}
