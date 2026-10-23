//! Opens the sealed forms and design tokens in memory (ADR-022). The build
//! script sealed them, or took them sealed from a design pack (ADR-036),
//! under a key derived from the seed it wrote; the key is derived again here
//! at load and the plain JSON never touches disk.

use crate::chacha;

mod seed {
    include!(concat!(env!("OUT_DIR"), "/seal_seed.rs"));
}

/// The sealed bytes of one asset, by its path under `assets/`.
macro_rules! sealed {
    ($path:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/sealed/", $path))
    };
}
pub(crate) use sealed;

/// The sealed design tokens.
pub(crate) const TOKENS: &[u8] = sealed!("tokens.json");

/// Decrypts the asset at `path` (as `tokens.json` or `forms/bar.json`).
///
/// # Errors
///
/// When the bytes do not open to UTF-8 text, which means they were not
/// sealed by this build.
pub fn open(path: &str, sealed: &[u8]) -> Result<String, String> {
    let mut data = sealed.to_vec();
    chacha::apply(
        &chacha::derive(&seed::SEED_A, &seed::SEED_B),
        &chacha::nonce(path),
        &mut data,
    );
    String::from_utf8(data).map_err(|_| format!("{path}: the sealed asset does not open"))
}

#[cfg(test)]
mod tests {
    use crate::chacha;

    /// RFC 8439 section 2.3.2.
    #[test]
    fn the_block_function_matches_the_rfc_vector() {
        let key: [u32; 8] = std::array::from_fn(|i| {
            let b = u8::try_from(i * 4).unwrap();
            u32::from_le_bytes([b, b + 1, b + 2, b + 3])
        });
        let out = chacha::block(&key, 1, &[0x0900_0000, 0x4a00_0000, 0]);
        assert_eq!(
            out[..4],
            [0xe4e7_f110, 0x1559_3bd1, 0x1fdd_0f50, 0xc471_20a3]
        );
        assert_eq!(out[15], 0x4e3c_50a2);
    }

    #[test]
    fn sealing_twice_gives_back_the_text_and_nonces_differ_per_path() {
        let key = chacha::derive(&[1; 8], &[2; 8]);
        let text = br##"{"themes":{"obsidian":{"accent":"#123abc"}}}"##.repeat(3);
        let mut data = text.clone();
        chacha::apply(&key, &chacha::nonce("tokens.json"), &mut data);
        assert_ne!(data, text);
        assert!(!data.windows(7).any(|w| w == b"#123abc"));
        chacha::apply(&key, &chacha::nonce("tokens.json"), &mut data);
        assert_eq!(data, text);
        assert_ne!(
            chacha::nonce("forms/bar.json"),
            chacha::nonce("forms/line.json")
        );
    }

    #[test]
    fn the_embedded_tokens_are_sealed_and_open_to_json() {
        let text = super::open("tokens.json", super::TOKENS).unwrap();
        assert!(text.contains("\"themes\""));
        assert!(!super::TOKENS.windows(8).any(|w| w == b"\"themes\""));
        assert!(super::open("forms/bar.json", super::TOKENS).is_err());
    }

    /// ADR-036, ADR-037: a build from source seals the plain tokens and the
    /// forms under a seed derived from the plaintext, so sealing the tree
    /// again now gives exactly the bytes this build embedded, and a second
    /// build gives them too.
    #[test]
    fn a_build_from_source_embeds_what_sealing_the_tree_again_gives() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let read = || {
            crate::pack::read(
                &assets.join("plain/tokens.json"),
                Some(&assets.join("forms")),
            )
            .unwrap()
        };
        let again = crate::pack::seal(0, read());
        assert_eq!(again, crate::pack::seal(0, read()));
        if env!("TILES_SEAL_SOURCE") != "assets" {
            return; // Built from a design pack: its bytes are the pack's.
        }
        let sealed = |path: &str| {
            again
                .assets
                .iter()
                .find(|a| a.path == path)
                .map(|a| a.bytes.as_slice())
        };
        assert_eq!(sealed("tokens.json"), Some(super::TOKENS));
        for (name, bytes) in crate::forms::EMBEDDED {
            assert_eq!(
                sealed(&format!("forms/{name}.json")),
                Some(*bytes),
                "{name}"
            );
        }
        assert_eq!(
            (again.seed.0, again.seed.1),
            (super::seed::SEED_A, super::seed::SEED_B)
        );
    }
}
