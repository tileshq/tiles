//! The Omni API key (ADR-008): no `Display`, no `Clone`, no `Serialize`, and a
//! `Debug` that prints `[redacted]`.

use std::fmt;

/// Longest API key Tiles accepts, in bytes.
pub const MAX_KEY_BYTES: usize = 4_096;

/// An Omni organization API key or personal access token.
///
/// Its bytes are overwritten when it is dropped, and so are the copies Tiles
/// makes itself: the text read from the Keychain, the environment or the
/// terminal ([`wipe`]) and the `Authorization` header value it builds. Copies
/// Tiles does not own are not: the process environment, the Keychain
/// framework's buffers, standard input's read buffer and the HTTP client's
/// own header copy. This narrows how long the key sits in memory; it does not
/// remove every copy.
pub struct ApiKey(Vec<u8>);

impl ApiKey {
    /// Wraps `raw`, trimmed of surrounding whitespace.
    ///
    /// # Errors
    ///
    /// When the key is empty, longer than [`MAX_KEY_BYTES`], or holds anything
    /// but visible ASCII (a line break would let a key inject an HTTP header).
    /// The error never holds the key.
    pub fn new(raw: &str) -> Result<Self, KeyProblem> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(KeyProblem::Empty);
        }
        if trimmed.len() > MAX_KEY_BYTES {
            return Err(KeyProblem::TooLong);
        }
        if !trimmed.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(KeyProblem::NotVisibleAscii);
        }
        Ok(Self(trimmed.as_bytes().to_vec()))
    }

    /// The key text, for the `Authorization` header only.
    pub(crate) fn expose(&self) -> &str {
        // `new` admits visible ASCII only, so this never falls back.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// Replaces every copy of the key in `text` with `[redacted]`.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        text.replace(self.expose(), "[redacted]")
    }
}

impl Drop for ApiKey {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&self.0);
    }
}

/// Overwrites `text`'s bytes before freeing them, for a copy of the key
/// Tiles made itself.
pub fn wipe(text: String) {
    let mut bytes = text.into_bytes();
    bytes.fill(0);
    std::hint::black_box(&bytes);
}

/// The `Authorization` header value for `key`, overwritten when dropped.
pub(crate) struct Bearer(String);

impl Bearer {
    pub(crate) fn new(key: &ApiKey) -> Self {
        // Sized once, so building it leaves no partial copy behind.
        let mut header = String::with_capacity("Bearer ".len() + key.0.len());
        header.push_str("Bearer ");
        header.push_str(key.expose());
        Self(header)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for Bearer {
    fn drop(&mut self) {
        wipe(std::mem::take(&mut self.0));
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey([redacted])")
    }
}

/// Why a key was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyProblem {
    /// Nothing but whitespace.
    Empty,
    /// Over [`MAX_KEY_BYTES`].
    TooLong,
    /// A control character, a space or non-ASCII text.
    NotVisibleAscii,
}

impl fmt::Display for KeyProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "is empty",
            Self::TooLong => "is longer than any Omni key",
            Self::NotVisibleAscii => "holds spaces, control characters or non-ASCII text",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiKey, KeyProblem};

    const PLANTED: &str = "omni_planted_key_7Yq2xL9vR4";

    #[test]
    fn debug_never_prints_the_key() {
        let key = ApiKey::new(PLANTED).expect("key");
        assert_eq!(format!("{key:?}"), "ApiKey([redacted])");
        assert_eq!(format!("{key:#?}"), "ApiKey([redacted])");
    }

    #[test]
    fn keys_are_trimmed() {
        let key = ApiKey::new(&format!("  {PLANTED}\n")).expect("key");
        assert_eq!(key.expose(), PLANTED);
    }

    #[test]
    fn header_injection_and_junk_are_refused() {
        assert_eq!(
            ApiKey::new("a\r\nX-Evil: 1").err(),
            Some(KeyProblem::NotVisibleAscii)
        );
        assert_eq!(ApiKey::new("a b").err(), Some(KeyProblem::NotVisibleAscii));
        assert_eq!(ApiKey::new("ключ").err(), Some(KeyProblem::NotVisibleAscii));
        assert_eq!(ApiKey::new("   ").err(), Some(KeyProblem::Empty));
        assert_eq!(
            ApiKey::new(&"k".repeat(4_097)).err(),
            Some(KeyProblem::TooLong)
        );
    }

    #[test]
    fn redact_removes_every_copy() {
        let key = ApiKey::new(PLANTED).expect("key");
        let text = format!("{PLANTED} and again {PLANTED}");
        assert_eq!(key.redact(&text), "[redacted] and again [redacted]");
    }
}
