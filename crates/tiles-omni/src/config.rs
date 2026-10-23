//! Where Tiles finds the Omni instance and key (ADR-008, ADR-013): both from
//! the environment, or both from the macOS Keychain, never one from each.
//! Never a file Tiles writes, the intent, or a command-line argument.
//!
//! In the Keychain the key sits under an account that names its host,
//! `api-key@acme.omniapp.co`, so a URL item rewritten to another host finds
//! no key to send there. `tiles omni set-key` writes both items through
//! [`SecretStore::put`]; an item this binary creates trusts only this binary,
//! so another process reading it gets a macOS prompt.

use std::fmt;

use crate::secret::{ApiKey, KeyProblem};

/// Environment variable holding the Omni base URL, such as `https://acme.omniapp.co`.
pub const ENV_URL: &str = "TILES_OMNI_URL";
/// Environment variable holding the Omni API key.
pub const ENV_API_KEY: &str = "TILES_OMNI_API_KEY";
/// Optional environment variable naming the one Omni model to read topics from.
pub const ENV_MODEL_ID: &str = "TILES_OMNI_MODEL_ID";
/// The Keychain service holding Tiles' Omni items.
pub const KEYCHAIN_SERVICE: &str = "tiles.omni";
/// Keychain account of the base URL item.
pub const KEYCHAIN_URL: &str = "url";
/// Keychain account prefix of the API key item; the account is
/// `api-key@<host>` ([`keychain_key_account`]).
pub const KEYCHAIN_API_KEY: &str = "api-key";
/// Keychain account of the optional model id item.
pub const KEYCHAIN_MODEL_ID: &str = "model-id";

/// The Omni API root, always `https://` (except the test-only loopback).
#[derive(Clone, PartialEq, Eq)]
pub struct BaseUrl {
    scheme: &'static str,
    authority: String,
    path: String,
}

impl BaseUrl {
    /// Parses an Omni base URL. `https://acme.omniapp.co` and
    /// `https://acme.omniapp.co/api` both give the documented API root
    /// `https://{instance}.omniapp.co/api` (<https://docs.omni.co/api/authentication.md>).
    ///
    /// # Errors
    ///
    /// When the URL is not `https://`, carries a user name, password, query or
    /// fragment, or its host or path holds anything but plain URL characters.
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        let raw = raw.trim();
        let bad = |problem: &'static str| ConfigError::BadUrl(problem);
        let rest = match raw.get(..8) {
            Some(scheme) if scheme.eq_ignore_ascii_case("https://") => &raw[8..],
            _ => return Err(bad("must start with https://")),
        };
        if rest.contains(['?', '#']) {
            return Err(bad("must not carry a query or fragment"));
        }
        let (authority, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, ""),
        };
        if authority.contains('@') {
            return Err(bad("must not carry a user name or password"));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        let host_ok = !host.is_empty()
            && host.len() <= 253
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            && !host.starts_with(['.', '-'])
            && !host.ends_with(['.', '-']);
        if !host_ok {
            return Err(bad("must name a plain host such as acme.omniapp.co"));
        }
        if let Some(port) = port
            && !(!port.is_empty()
                && port.len() <= 5
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|p| p > 0))
        {
            return Err(bad("has a port that is not a number from 1 to 65535"));
        }
        let path = path.trim_end_matches('/');
        let path_ok = path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
            && !path
                .split('/')
                .any(|segment| segment == "." || segment == "..");
        if !path_ok {
            return Err(bad(
                "has a path with characters other than letters, digits, - _ . /",
            ));
        }
        let path = if path.is_empty() {
            "/api".to_owned()
        } else {
            path.to_owned()
        };
        Ok(Self {
            scheme: "https",
            authority: authority.to_ascii_lowercase(),
            path,
        })
    }

    /// A plain `http://127.0.0.1:{port}/api` root for the replay server. Only the
    /// `replay` feature, which tests turn on, has it.
    #[cfg(feature = "replay")]
    #[must_use]
    pub fn loopback(port: u16) -> Self {
        Self {
            scheme: "http",
            authority: format!("127.0.0.1:{port}"),
            path: "/api".to_owned(),
        }
    }

    /// Whether this is the test-only loopback root.
    pub(crate) fn is_loopback_http(&self) -> bool {
        self.scheme == "http"
    }

    /// The host and port requests go to.
    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// The full URL of `path_and_query`, which starts with `/v1/`.
    pub(crate) fn join(&self, path_and_query: &str) -> String {
        format!(
            "{}://{}{}{}",
            self.scheme, self.authority, self.path, path_and_query
        )
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}{}", self.scheme, self.authority, self.path)
    }
}

impl fmt::Debug for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BaseUrl({}://{}{})",
            self.scheme, self.authority, self.path
        )
    }
}

/// An Omni model id: a UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelId(String);

impl ModelId {
    /// Parses a UUID such as `4f8c2a1e-0b7d-4c55-9a3e-2d1f6b8e9c01`.
    ///
    /// # Errors
    ///
    /// When `raw` is not a UUID.
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        let raw = raw.trim();
        if is_uuid(raw) {
            Ok(Self(raw.to_ascii_lowercase()))
        } else {
            Err(ConfigError::BadModelId)
        }
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `text` is a UUID in its usual 8-4-4-4-12 hex form.
pub(crate) fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The Keychain account holding the key for `base`: `api-key@<host[:port]>`.
#[must_use]
pub fn keychain_key_account(base: &BaseUrl) -> String {
    format!("{KEYCHAIN_API_KEY}@{}", base.authority())
}

/// Where the URL and key came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `TILES_OMNI_URL` and `TILES_OMNI_API_KEY`.
    Environment,
    /// The login Keychain.
    Keychain,
}

/// Everything the client needs to reach one Omni instance.
#[derive(Debug)]
pub struct OmniConfig {
    /// The API root.
    pub base: BaseUrl,
    /// The key sent as `Authorization: Bearer`.
    pub key: ApiKey,
    /// The one model to read topics from; every shared model when absent.
    pub model: Option<ModelId>,
    /// Where the URL and key came from.
    pub source: Source,
}

/// A store of named secrets: the macOS Keychain, or a fake in tests.
pub trait SecretStore {
    /// The value stored under `account` in the [`KEYCHAIN_SERVICE`] service.
    ///
    /// # Errors
    ///
    /// When the store exists but refuses the read.
    fn get(&self, account: &str) -> Result<Option<String>, ConfigError>;

    /// Whether an item is stored under `account`. The Keychain's asks
    /// without reading the item's value; this default, for stores in
    /// tests, reads it.
    ///
    /// # Errors
    ///
    /// When the store exists but refuses the search.
    fn has(&self, account: &str) -> Result<bool, ConfigError> {
        Ok(self.get(account)?.is_some())
    }

    /// Stores `value` under `account`, replacing any item there.
    ///
    /// # Errors
    ///
    /// When the store is read-only or refuses the write.
    fn put(&self, account: &str, value: &str) -> Result<(), ConfigError> {
        let _ = value;
        Err(ConfigError::KeychainWrite {
            account: account.to_owned(),
            code: None,
        })
    }
}

/// Stores the Omni URL and key for `tiles omni set-key`: the URL under
/// [`KEYCHAIN_URL`] and the key under [`keychain_key_account`].
///
/// # Errors
///
/// When the store refuses a write. The error never holds the key.
pub fn store_settings(
    store: &dyn SecretStore,
    base: &BaseUrl,
    key: &ApiKey,
) -> Result<(), ConfigError> {
    store.put(KEYCHAIN_URL, &base.to_string())?;
    store.put(&keychain_key_account(base), key.expose())
}

/// The login Keychain on macOS; empty everywhere else.
#[derive(Debug, Default, Clone, Copy)]
pub struct Keychain;

impl SecretStore for Keychain {
    #[cfg(target_os = "macos")]
    fn get(&self, account: &str) -> Result<Option<String>, ConfigError> {
        // errSecItemNotFound: the item does not exist.
        const NOT_FOUND: i32 = -25_300;
        match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| ConfigError::Keychain {
                    account: account.to_owned(),
                    code: None,
                }),
            Err(error) if error.code() == NOT_FOUND => Ok(None),
            Err(error) => Err(ConfigError::Keychain {
                account: account.to_owned(),
                code: Some(error.code()),
            }),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn get(&self, _account: &str) -> Result<Option<String>, ConfigError> {
        Ok(None)
    }

    /// Searches by service and account and asks for no attribute and no
    /// data, so the item's value is never read.
    #[cfg(target_os = "macos")]
    fn has(&self, account: &str) -> Result<bool, ConfigError> {
        use security_framework::item::{ItemClass, ItemSearchOptions};
        const NOT_FOUND: i32 = -25_300;
        match ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(KEYCHAIN_SERVICE)
            .account(account)
            .search()
        {
            Ok(_) => Ok(true),
            Err(error) if error.code() == NOT_FOUND => Ok(false),
            Err(error) => Err(ConfigError::Keychain {
                account: account.to_owned(),
                code: Some(error.code()),
            }),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn has(&self, _account: &str) -> Result<bool, ConfigError> {
        Ok(false)
    }

    /// Deletes any item under `account`, then adds a new one. An item this
    /// binary adds trusts only this binary, so another process that reads it,
    /// `/usr/bin/security` included, gets a macOS prompt. Updating an item in
    /// place would keep the access list of whoever made it.
    #[cfg(target_os = "macos")]
    fn put(&self, account: &str, value: &str) -> Result<(), ConfigError> {
        const NOT_FOUND: i32 = -25_300;
        let failed = |error: security_framework::base::Error| ConfigError::KeychainWrite {
            account: account.to_owned(),
            code: Some(error.code()),
        };
        match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account) {
            Err(error) if error.code() != NOT_FOUND => return Err(failed(error)),
            _ => {}
        }
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            account,
            value.as_bytes(),
        )
        .map_err(failed)
    }
}

impl OmniConfig {
    /// Reads the configuration from the process environment and the Keychain.
    ///
    /// # Errors
    ///
    /// As [`OmniConfig::resolve`].
    pub fn from_system() -> Result<Self, ConfigError> {
        Self::resolve(|name| std::env::var(name).ok(), &Keychain)
    }

    /// Reads the URL and key together: both from `env`, or, when neither
    /// is set there, both from `store`, the key from the account naming the
    /// URL's host. The model id is read from `env` first and `store` second.
    ///
    /// # Errors
    ///
    /// When only one of the URL and key is in `env`, either is missing, or a
    /// value is malformed.
    pub fn resolve(
        env: impl Fn(&str) -> Option<String>,
        store: &dyn SecretStore,
    ) -> Result<Self, ConfigError> {
        let set = |var: &str| env(var).filter(|v| !v.trim().is_empty());
        let stored = |account: &str| -> Result<Option<String>, ConfigError> {
            Ok(store.get(account)?.filter(|v| !v.trim().is_empty()))
        };
        let (base, key, source) = match (set(ENV_URL), set(ENV_API_KEY)) {
            (Some(url), Some(key)) => match BaseUrl::parse(&url) {
                Ok(base) => (base, key, Source::Environment),
                Err(e) => {
                    crate::secret::wipe(key);
                    return Err(e);
                }
            },
            (Some(_), None) => return Err(ConfigError::MixedSources(ENV_URL)),
            (None, Some(key)) => {
                crate::secret::wipe(key);
                return Err(ConfigError::MixedSources(ENV_API_KEY));
            }
            (None, None) => {
                let url = stored(KEYCHAIN_URL)?.ok_or(ConfigError::Missing("URL"))?;
                let base = BaseUrl::parse(&url)?;
                let key = stored(&keychain_key_account(&base))?
                    .ok_or_else(|| ConfigError::NoKeyFor(base.authority().to_owned()))?;
                (base, key, Source::Keychain)
            }
        };
        let parsed = ApiKey::new(&key);
        crate::secret::wipe(key);
        let key = parsed.map_err(ConfigError::BadKey)?;
        let model = match set(ENV_MODEL_ID) {
            Some(m) => Some(m),
            None => stored(KEYCHAIN_MODEL_ID)?,
        }
        .map(|m| ModelId::parse(&m))
        .transpose()?;
        Ok(Self {
            base,
            key,
            model,
            source,
        })
    }
}

/// Why the configuration could not be read. Never holds the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// Neither the environment nor the Keychain has this setting.
    Missing(&'static str),
    /// Only this one of the URL and key variables is set; the other would
    /// come from the Keychain, which could send its key to another host.
    MixedSources(&'static str),
    /// The Keychain has a URL but no key stored for its host.
    NoKeyFor(String),
    /// The Keychain refused a write, with its status code when it gave one.
    KeychainWrite {
        /// The Keychain account written.
        account: String,
        /// The Security framework status code.
        code: Option<i32>,
    },
    /// The base URL was refused.
    BadUrl(&'static str),
    /// The key was refused.
    BadKey(KeyProblem),
    /// The model id is not a UUID.
    BadModelId,
    /// The Keychain refused a read, with its status code when it gave one.
    Keychain {
        /// The Keychain account read.
        account: String,
        /// The Security framework status code.
        code: Option<i32>,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing("URL") => write!(
                f,
                "no Omni URL: set {ENV_URL} or the Keychain item {KEYCHAIN_SERVICE}/{KEYCHAIN_URL} (docs/omni.md)"
            ),
            Self::Missing(_) => write!(
                f,
                "no Omni API key: set {ENV_API_KEY} or run tiles omni set-key (docs/omni.md)"
            ),
            Self::MixedSources(var) => write!(
                f,
                "{var} is set but not {}; set both, or unset it to use the Keychain (docs/omni.md)",
                if *var == ENV_URL {
                    ENV_API_KEY
                } else {
                    ENV_URL
                }
            ),
            Self::NoKeyFor(host) => write!(
                f,
                "the Keychain holds no Omni API key for {host}; run tiles omni set-key https://{host} (docs/omni.md)"
            ),
            Self::KeychainWrite {
                account,
                code: Some(code),
            } => write!(
                f,
                "the Keychain refused to store {KEYCHAIN_SERVICE}/{account} (status {code})"
            ),
            Self::KeychainWrite {
                account,
                code: None,
            } => write!(
                f,
                "there is no Keychain to store {KEYCHAIN_SERVICE}/{account} in; use {ENV_URL} and {ENV_API_KEY}"
            ),
            Self::BadUrl(problem) => write!(f, "the Omni URL {problem}"),
            Self::BadKey(problem) => write!(f, "the Omni API key {problem}"),
            Self::BadModelId => write!(f, "{ENV_MODEL_ID} is not a model UUID"),
            Self::Keychain {
                account,
                code: Some(code),
            } => write!(
                f,
                "the Keychain refused to read {KEYCHAIN_SERVICE}/{account} (status {code}); allow tiles in Keychain Access"
            ),
            Self::Keychain {
                account,
                code: None,
            } => {
                write!(
                    f,
                    "the Keychain item {KEYCHAIN_SERVICE}/{account} is not text"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use std::cell::RefCell;

    use super::{
        BaseUrl, ConfigError, ModelId, OmniConfig, SecretStore, Source, keychain_key_account,
        store_settings,
    };
    use crate::ApiKey;

    const PLANTED: &str = "omni_planted_key_7Yq2xL9vR4";

    struct Fake(HashMap<&'static str, &'static str>);

    impl SecretStore for Fake {
        fn get(&self, account: &str) -> Result<Option<String>, ConfigError> {
            Ok(self.0.get(account).map(|v| (*v).to_owned()))
        }
    }

    fn env(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        move |name| map.get(name).map(|v| (*v).to_owned())
    }

    #[test]
    fn base_urls_normalise_to_the_api_root() {
        let url = BaseUrl::parse("https://Acme.omniapp.co").expect("url");
        assert_eq!(
            url.join("/v1/models"),
            "https://acme.omniapp.co/api/v1/models"
        );
        let url = BaseUrl::parse(" https://acme.omniapp.co/api/ ").expect("url");
        assert_eq!(
            url.join("/v1/models"),
            "https://acme.omniapp.co/api/v1/models"
        );
        let url = BaseUrl::parse("HTTPS://acme.example:8443/omni/api").expect("url");
        assert_eq!(
            url.join("/v1/models"),
            "https://acme.example:8443/omni/api/v1/models"
        );
    }

    #[test]
    fn unsafe_base_urls_are_refused() {
        for raw in [
            "http://acme.omniapp.co",
            "acme.omniapp.co",
            "ftp://acme.omniapp.co",
            "https://user:pass@acme.omniapp.co",
            "https://acme.omniapp.co/api?x=1",
            "https://acme.omniapp.co/#frag",
            "https://acme.omniapp.co/../etc",
            "https://acme.omni app.co",
            "https://acme.omniapp.co:0",
            "https://acme.omniapp.co:99999",
            "https://acme.omniapp.co\u{1b}[2J",
            "https://",
            "https://.acme",
        ] {
            assert!(
                matches!(BaseUrl::parse(raw), Err(ConfigError::BadUrl(_))),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn the_environment_wins_over_the_keychain() {
        let store = Fake(HashMap::from([
            ("url", "https://keychain.omniapp.co"),
            ("api-key@keychain.omniapp.co", "kc"),
        ]));
        let config = OmniConfig::resolve(
            env(&[
                ("TILES_OMNI_URL", "https://env.omniapp.co"),
                ("TILES_OMNI_API_KEY", PLANTED),
            ]),
            &store,
        )
        .expect("config");
        assert_eq!(config.base.authority(), "env.omniapp.co");
        assert_eq!(config.key.expose(), PLANTED);
        assert_eq!(config.model, None);
        assert_eq!(config.source, Source::Environment);
    }

    #[test]
    fn the_keychain_supplies_both_when_the_environment_has_neither() {
        let store = Fake(HashMap::from([
            ("url", "https://keychain.omniapp.co"),
            ("api-key@keychain.omniapp.co", PLANTED),
            ("model-id", "4F8C2A1E-0B7D-4C55-9A3E-2D1F6B8E9C01"),
        ]));
        let config =
            OmniConfig::resolve(env(&[("TILES_OMNI_API_KEY", "  ")]), &store).expect("config");
        assert_eq!(config.base.authority(), "keychain.omniapp.co");
        assert_eq!(config.key.expose(), PLANTED);
        assert_eq!(config.source, Source::Keychain);
        assert_eq!(
            config.model.as_ref().map(ModelId::as_str),
            Some("4f8c2a1e-0b7d-4c55-9a3e-2d1f6b8e9c01")
        );
    }

    // The security review's F3: an environment URL took the Keychain key to
    // any HTTPS host.
    #[test]
    fn an_environment_url_never_takes_the_keychain_key() {
        let store = Fake(HashMap::from([
            ("url", "https://acme.omniapp.co"),
            ("api-key@acme.omniapp.co", PLANTED),
            ("api-key", PLANTED),
        ]));
        let err = OmniConfig::resolve(
            env(&[("TILES_OMNI_URL", "https://collector.evil.test")]),
            &store,
        )
        .expect_err("a mix is refused");
        assert_eq!(err, ConfigError::MixedSources("TILES_OMNI_URL"));
        assert!(err.to_string().contains("TILES_OMNI_API_KEY"));
        let err = OmniConfig::resolve(env(&[("TILES_OMNI_API_KEY", PLANTED)]), &store)
            .expect_err("a mix is refused");
        assert_eq!(err, ConfigError::MixedSources("TILES_OMNI_API_KEY"));
    }

    #[test]
    fn a_rewritten_keychain_url_finds_no_key_for_its_host() {
        let store = Fake(HashMap::from([
            ("url", "https://collector.evil.test"),
            ("api-key@acme.omniapp.co", PLANTED),
            ("api-key", PLANTED),
        ]));
        let err = OmniConfig::resolve(env(&[]), &store).expect_err("no key for that host");
        assert_eq!(err, ConfigError::NoKeyFor("collector.evil.test".into()));
        assert!(!err.to_string().contains(PLANTED));
    }

    struct Recorder(RefCell<Vec<(String, String)>>);

    impl SecretStore for Recorder {
        fn get(&self, account: &str) -> Result<Option<String>, ConfigError> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|(a, _)| a == account)
                .map(|(_, v)| v.clone()))
        }
        fn put(&self, account: &str, value: &str) -> Result<(), ConfigError> {
            self.0.borrow_mut().push((account.into(), value.into()));
            Ok(())
        }
    }

    #[test]
    fn stored_settings_resolve_to_the_same_host_and_key() {
        let store = Recorder(RefCell::new(Vec::new()));
        let base = BaseUrl::parse("https://Acme.omniapp.co").expect("url");
        store_settings(&store, &base, &ApiKey::new(PLANTED).expect("key")).expect("stored");
        assert_eq!(keychain_key_account(&base), "api-key@acme.omniapp.co");
        assert_eq!(
            store.0.borrow()[0],
            ("url".to_string(), "https://acme.omniapp.co/api".to_string())
        );
        let config = OmniConfig::resolve(env(&[]), &store).expect("config");
        assert_eq!(config.base.authority(), "acme.omniapp.co");
        assert_eq!(config.key.expose(), PLANTED);
        let refused = store_settings(
            &Fake(HashMap::new()),
            &base,
            &ApiKey::new(PLANTED).expect("key"),
        )
        .expect_err("a read-only store");
        assert!(!refused.to_string().contains(PLANTED));
    }

    #[test]
    fn missing_and_malformed_settings_name_the_fix_without_the_key() {
        let empty = Fake(HashMap::new());
        let missing = OmniConfig::resolve(env(&[]), &empty).expect_err("missing");
        assert!(missing.to_string().contains("TILES_OMNI_URL"));
        let no_key = OmniConfig::resolve(
            env(&[]),
            &Fake(HashMap::from([("url", "https://a.omniapp.co")])),
        )
        .expect_err("no key");
        assert!(
            no_key
                .to_string()
                .contains("tiles omni set-key https://a.omniapp.co")
        );
        let bad_key = OmniConfig::resolve(
            env(&[
                ("TILES_OMNI_URL", "https://a.omniapp.co"),
                ("TILES_OMNI_API_KEY", "omni secret\n"),
            ]),
            &empty,
        )
        .expect_err("bad key");
        assert!(!bad_key.to_string().contains("secret"));
        assert!(!format!("{bad_key:?}").contains("secret"));
        let bad_model = OmniConfig::resolve(
            env(&[
                ("TILES_OMNI_URL", "https://a.omniapp.co"),
                ("TILES_OMNI_API_KEY", PLANTED),
                ("TILES_OMNI_MODEL_ID", "../x"),
            ]),
            &empty,
        )
        .expect_err("bad model");
        assert_eq!(bad_model, ConfigError::BadModelId);
    }

    #[test]
    fn config_debug_redacts_the_key() {
        let config = OmniConfig::resolve(
            env(&[
                ("TILES_OMNI_URL", "https://a.omniapp.co"),
                ("TILES_OMNI_API_KEY", PLANTED),
            ]),
            &Fake(HashMap::new()),
        )
        .expect("config");
        let debug = format!("{config:?} {config:#?}");
        assert!(!debug.contains(PLANTED));
        assert!(debug.contains("[redacted]"));
    }
}
