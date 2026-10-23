//! A loopback replay server for tests (ADR-008): it serves recorded synthetic
//! exchanges and refuses every request it has no recording for, so no test can
//! reach a real Omni instance by accident.
//!
//! Only the `replay` feature builds this module, and only tests turn it on.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::config::{BaseUrl, OmniConfig};
use crate::secret::ApiKey;

/// The status the server answers an unrecorded request with.
pub const UNRECORDED: u16 = 501;

/// One recorded request and the response to give it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    /// `GET` or `POST`.
    pub method: String,
    /// The path and query after `/api`, such as `/v1/models?modelKind=SHARED`.
    pub path: String,
    /// The JSON body the request must carry, when it has one.
    #[serde(default)]
    pub request: Option<Value>,
    /// The response status.
    #[serde(default = "ok")]
    pub status: u16,
    /// Extra response headers.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// A fixture file under `fixtures/omni/` holding the response body.
    #[serde(default)]
    pub file: Option<String>,
    /// The response body, when no file holds it.
    #[serde(skip)]
    pub body: Vec<u8>,
    /// How long to wait before answering.
    #[serde(skip)]
    pub delay: Duration,
    /// Whether the exchange answers one request only; otherwise it answers
    /// every matching request.
    #[serde(default)]
    pub once: bool,
}

fn ok() -> u16 {
    200
}

impl Exchange {
    /// An exchange answering `method path` with `status` and `body`.
    #[must_use]
    pub fn new(method: &str, path: &str, status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            method: method.to_owned(),
            path: path.to_owned(),
            request: None,
            status,
            headers: Vec::new(),
            file: None,
            body: body.into(),
            delay: Duration::ZERO,
            once: false,
        }
    }

    /// The same exchange, answering one request only.
    #[must_use]
    pub fn once(mut self) -> Self {
        self.once = true;
        self
    }

    /// The same exchange, matching only a request whose JSON body equals `request`.
    #[must_use]
    pub fn expecting(mut self, request: Value) -> Self {
        self.request = Some(request);
        self
    }

    /// The same exchange with an extra response header.
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// The same exchange, answered after `delay`.
    #[must_use]
    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// The directory of recorded synthetic fixtures.
#[must_use]
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/omni")
}

/// The recorded exchanges in `fixtures/omni/recorded.json`, bodies loaded.
///
/// # Panics
///
/// When the fixtures are missing or malformed: a test setup error.
#[must_use]
pub fn recorded() -> Vec<Exchange> {
    let dir = fixtures_dir();
    let manifest = std::fs::read(dir.join("recorded.json")).expect("fixtures/omni/recorded.json");
    let mut exchanges: Vec<Exchange> = serde_json::from_slice(&manifest).expect("recorded.json");
    for exchange in &mut exchanges {
        if let Some(file) = &exchange.file {
            exchange.body = std::fs::read(dir.join(file)).expect("fixture body");
        }
    }
    exchanges
}

/// A request the server saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// The method.
    pub method: String,
    /// The path and query after `/api`.
    pub path: String,
    /// Whether it carried the expected `Authorization: Bearer` header.
    pub authorized: bool,
    /// Whether a recording answered it.
    pub recorded: bool,
}

struct State {
    exchanges: Vec<Option<Exchange>>,
    seen: Vec<Seen>,
}

/// A replay server on `127.0.0.1` with a free port, stopped when dropped.
pub struct ReplayServer {
    port: u16,
    key: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ReplayServer {
    /// Starts serving `exchanges` to requests that carry `key`. The first
    /// matching exchange answers; one marked `once` is then used up.
    ///
    /// # Panics
    ///
    /// When no loopback port can be bound.
    #[must_use]
    pub fn start(key: &str, exchanges: Vec<Exchange>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("local address").port();
        let state = Arc::new(Mutex::new(State {
            exchanges: exchanges.into_iter().map(Some).collect(),
            seen: Vec::new(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (state, stop, key) = (Arc::clone(&state), Arc::clone(&stop), key.to_owned());
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        serve(stream, &key, &state);
                    }
                }
            })
        };
        Self {
            port,
            key: key.to_owned(),
            state,
            stop,
            worker: Some(worker),
        }
    }

    /// The loopback API root of this server.
    #[must_use]
    pub fn base_url(&self) -> BaseUrl {
        BaseUrl::loopback(self.port)
    }

    /// A client configuration pointing at this server with its key.
    ///
    /// # Panics
    ///
    /// When the key given to [`ReplayServer::start`] is not a valid key.
    #[must_use]
    pub fn config(&self) -> OmniConfig {
        OmniConfig {
            base: self.base_url(),
            key: ApiKey::new(&self.key).expect("key"),
            model: None,
            source: crate::Source::Environment,
        }
    }

    /// Every request seen so far.
    #[must_use]
    pub fn seen(&self) -> Vec<Seen> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .seen
            .clone()
    }

    /// The requests no recording answered.
    #[must_use]
    pub fn unrecorded(&self) -> Vec<Seen> {
        self.seen().into_iter().filter(|s| !s.recorded).collect()
    }
}

impl Drop for ReplayServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(stream: TcpStream, key: &str, state: &Mutex<State>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream.take(2 * 1024 * 1024));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default();
    let path = target.strip_prefix("/api").unwrap_or(target).to_owned();
    let mut length = 0usize;
    let mut authorized = false;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("content-length") {
                length = value.parse().unwrap_or(0);
            } else if name.eq_ignore_ascii_case("authorization") {
                authorized = value == format!("Bearer {key}");
            }
        }
    }
    let mut body = vec![0; length.min(1024 * 1024)];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let request: Option<Value> = serde_json::from_slice(&body).ok();
    let answer = {
        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
        let found = state.exchanges.iter_mut().find(|slot| {
            slot.as_ref().is_some_and(|e| {
                e.method == method
                    && e.path == path
                    && e.request
                        .as_ref()
                        .is_none_or(|want| Some(want) == request.as_ref())
            })
        });
        let answer = match found {
            Some(slot) if authorized => {
                if slot.as_ref().is_some_and(|e| e.once) {
                    slot.take()
                } else {
                    slot.clone()
                }
            }
            _ => None,
        };
        let recorded = answer.is_some();
        state.seen.push(Seen {
            method,
            path,
            authorized,
            recorded,
        });
        match (authorized, answer) {
            (false, _) => Exchange::new("", "", 401, r#"{"detail":"unauthorized","status":401}"#),
            (true, Some(exchange)) => exchange,
            (true, None) => Exchange::new(
                "",
                "",
                UNRECORDED,
                r#"{"detail":"the replay server has no recording for this request","status":501}"#,
            ),
        }
    };
    thread::sleep(answer.delay);
    let mut head = format!(
        "HTTP/1.1 {} Replay\r\nContent-Length: {}\r\nConnection: close\r\n",
        answer.status,
        answer.body.len()
    );
    for (name, value) in &answer.headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    if !answer
        .headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
    {
        head.push_str("Content-Type: application/json\r\n");
    }
    head.push_str("\r\n");
    let _ = writer.write_all(head.as_bytes());
    let _ = writer.write_all(&answer.body);
    let _ = writer.flush();
    let _ = writer.shutdown(Shutdown::Both);
}
