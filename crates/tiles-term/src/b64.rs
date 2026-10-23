//! Standard base64 with padding (RFC 4648 section 4), for graphics payloads.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` as base64 text.
///
/// Built as bytes and turned into a `String` once: every byte is ASCII, so
/// the conversion cannot fail, and a prover reads each output byte's length
/// as one (verify/formal/kitty/README.md).
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() {
                ALPHABET[((n >> shift) & 63) as usize]
            } else {
                b'='
            });
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn rfc_4648_test_vectors() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), coded);
        }
    }

    #[test]
    fn every_byte_value_uses_only_the_alphabet() {
        let all: Vec<u8> = (0..=255).collect();
        let coded = encode(&all);
        assert_eq!(coded.len(), 344);
        assert!(
            coded
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
        );
    }
}
