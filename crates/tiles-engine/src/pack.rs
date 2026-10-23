//! The design pack (ADR-036): the design tokens sealed once into one file a
//! build takes from outside the source tree. The forms are public, so a
//! pack carries `tokens.json` alone and the build seals the tree's forms.
//!
//! A pack is a short text manifest and the sealed bytes after it:
//!
//! ```text
//! tiles-design-pack 1
//! number 1
//! seed-a <64 hex digits>
//! seed-b <64 hex digits>
//! asset tokens.json 3102
//! end
//! <the sealed bytes of each asset, in manifest order>
//! ```
//!
//! The seed comes from the SHA-256 of the plaintext and a fixed constant,
//! so sealing the same files twice gives the same bytes, and a build from
//! source, which seals `assets/` the same way, reproduces byte for byte.
//! The build script includes this file, as it includes `chacha.rs`.

use std::fmt::Write as _;
use std::path::Path;

use super::{chacha, sha256};

/// The manifest's first line.
const MAGIC: &str = "tiles-design-pack 1";
/// Mixed into the seed's digest, so a seed says which derivation made it.
const SEED_CONSTANT: &[u8] = b"tiles-seal-seed/1";
/// The design tokens' path, the one asset a pack carries.
pub const TOKENS: &str = "tokens.json";

/// One asset, by its path under `assets/` (as `tokens.json` or
/// `forms/bar.json`), with its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// The path under `assets/`.
    pub path: String,
    /// Plain or sealed, as the holder says.
    pub bytes: Vec<u8>,
}

/// A sealed design pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// The pack's number, `n` in `tiles-design-<n>.pack`.
    pub number: u32,
    /// The two seed halves the key is derived from (`chacha::derive`).
    pub seed: ([u32; 8], [u32; 8]),
    /// The sealed assets, ordered by path.
    pub assets: Vec<Asset>,
}

/// Whether `path` is one an asset may have: `tokens.json` or
/// `forms/<name>.json` with a name of lowercase letters, digits and dashes.
/// The build writes each asset under its path, so no other path is taken.
#[must_use]
pub fn valid_path(path: &str) -> bool {
    path == TOKENS
        || path
            .strip_prefix("forms/")
            .and_then(|p| p.strip_suffix(".json"))
            .is_some_and(|name| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
}

/// Reads the plain assets: `tokens` as `tokens.json`, and every
/// `forms/*.json` file in `forms` when given, ordered by path.
///
/// # Errors
///
/// When a file is unreadable or a form's file name is not a valid path.
pub fn read(tokens: &Path, forms: Option<&Path>) -> Result<Vec<Asset>, String> {
    let readable = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let mut assets = vec![Asset {
        path: TOKENS.into(),
        bytes: readable(tokens)?,
    }];
    if let Some(dir) = forms {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".json") {
                continue;
            }
            let path = format!("forms/{name}");
            if !valid_path(&path) {
                return Err(format!("{}: not a form file name", entry.path().display()));
            }
            assets.push(Asset {
                path,
                bytes: readable(&entry.path())?,
            });
        }
    }
    assets.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(assets)
}

/// The seed for these plain assets: two SHA-256 digests over one digest of
/// the constant and every asset's path and bytes, each length-prefixed.
#[must_use]
pub fn seed(plain: &[Asset]) -> ([u32; 8], [u32; 8]) {
    let mut framed = SEED_CONSTANT.to_vec();
    for asset in plain {
        for part in [asset.path.as_bytes(), &asset.bytes] {
            framed.extend_from_slice(&(part.len() as u64).to_le_bytes());
            framed.extend_from_slice(part);
        }
    }
    let all = sha256::digest(&framed);
    let half = |tag: u8| {
        let mut input = all.to_vec();
        input.push(tag);
        words(&sha256::digest(&input))
    };
    (half(b'a'), half(b'b'))
}

fn words(bytes: &[u8; 32]) -> [u32; 8] {
    let mut out = [0u32; 8];
    for (w, c) in out.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *w = u32::from_le_bytes(*c);
    }
    out
}

fn bytes(words: &[u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (c, w) in out.as_chunks_mut::<4>().0.iter_mut().zip(words) {
        *c = w.to_le_bytes();
    }
    out
}

/// Seals plain assets into pack `number`, under the seed [`seed`] derives
/// from them.
#[must_use]
pub fn seal(number: u32, mut plain: Vec<Asset>) -> Pack {
    plain.sort_by(|a, b| a.path.cmp(&b.path));
    let seed = seed(&plain);
    let key = chacha::derive(&seed.0, &seed.1);
    for asset in &mut plain {
        chacha::apply(&key, &chacha::nonce(&asset.path), &mut asset.bytes);
    }
    Pack {
        number,
        seed,
        assets: plain,
    }
}

impl Pack {
    /// The pack's file name, `tiles-design-<n>.pack`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("tiles-design-{}.pack", self.number)
    }

    /// The asset at `path`, opened: its plain text.
    ///
    /// # Errors
    ///
    /// When the pack has no such asset, or its bytes do not open to UTF-8
    /// text, which means they were not sealed under this pack's seed.
    pub fn open(&self, path: &str) -> Result<String, String> {
        let asset = self
            .assets
            .iter()
            .find(|a| a.path == path)
            .ok_or_else(|| format!("the pack has no {path}"))?;
        let mut data = asset.bytes.clone();
        let key = chacha::derive(&self.seed.0, &self.seed.1);
        chacha::apply(&key, &chacha::nonce(path), &mut data);
        String::from_utf8(data).map_err(|_| format!("{path}: the sealed asset does not open"))
    }

    /// Seals every plain asset but `tokens.json`, the source tree's forms,
    /// under the pack's own seed, so a build that takes the pack embeds the
    /// pack's tokens and the tree's forms.
    pub fn fill(&mut self, plain: Vec<Asset>) {
        let key = chacha::derive(&self.seed.0, &self.seed.1);
        for mut asset in plain.into_iter().filter(|a| a.path != TOKENS) {
            chacha::apply(&key, &chacha::nonce(&asset.path), &mut asset.bytes);
            self.assets.push(asset);
        }
        self.assets.sort_by(|a, b| a.path.cmp(&b.path));
    }

    /// The pack file's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!(
            "{MAGIC}\nnumber {}\nseed-a {}\nseed-b {}\n",
            self.number,
            sha256::hex(&bytes(&self.seed.0)),
            sha256::hex(&bytes(&self.seed.1)),
        );
        for asset in &self.assets {
            let _ = writeln!(out, "asset {} {}", asset.path, asset.bytes.len());
        }
        out.push_str("end\n");
        let mut out = out.into_bytes();
        for asset in &self.assets {
            out.extend_from_slice(&asset.bytes);
        }
        out
    }

    /// Reads a pack file and checks that every asset opens.
    ///
    /// # Errors
    ///
    /// A one-line reason when the manifest is malformed, names an asset
    /// other than `tokens.json` or names it twice, the sizes do not add up,
    /// the pack has no `tokens.json`, or it does not open.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        const END: &[u8] = b"\nend\n";
        let split = data
            .windows(END.len())
            .position(|w| w == END)
            .ok_or("no manifest: the pack has no end line")?;
        let head = std::str::from_utf8(&data[..split]).map_err(|_| "the manifest is not text")?;
        let mut body = &data[split + END.len()..];
        let mut lines = head.lines();
        if lines.next() != Some(MAGIC) {
            return Err(format!(
                "not a design pack: the first line is not \"{MAGIC}\""
            ));
        }
        let mut field = |name: &str| {
            lines
                .next()
                .and_then(|l| l.strip_prefix(name))
                .and_then(|l| l.strip_prefix(' '))
                .ok_or(format!("the manifest has no {name} line where expected"))
        };
        let number = field("number")?
            .parse()
            .map_err(|_| "the pack number is not a whole number")?;
        let seed_a = parse_seed(field("seed-a")?)?;
        let seed_b = parse_seed(field("seed-b")?)?;
        let mut assets: Vec<Asset> = Vec::new();
        for line in lines {
            let (path, len) = line
                .strip_prefix("asset ")
                .and_then(|l| l.split_once(' '))
                .ok_or_else(|| format!("manifest line \"{line}\" is not an asset line"))?;
            if path != TOKENS || !assets.is_empty() {
                return Err(format!(
                    "manifest asset \"{path}\" is not {TOKENS} or is listed twice"
                ));
            }
            let len: usize = len
                .parse()
                .map_err(|_| format!("{path}: the size is not a whole number"))?;
            if len > body.len() {
                return Err(format!("{path}: the pack ends before its {len} bytes"));
            }
            let (bytes, rest) = body.split_at(len);
            body = rest;
            assets.push(Asset {
                path: path.into(),
                bytes: bytes.to_vec(),
            });
        }
        if !body.is_empty() {
            return Err(format!("{} bytes follow the last asset", body.len()));
        }
        let pack = Pack {
            number,
            seed: (seed_a, seed_b),
            assets,
        };
        pack.open(TOKENS)?;
        Ok(pack)
    }
}

fn parse_seed(text: &str) -> Result<[u32; 8], String> {
    let bad = || "a seed is not 64 hex digits".to_string();
    if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|_| bad())?;
    }
    Ok(words(&out))
}

/// The SHA-256 of `data` as hex, as `shasum -a 256` prints it.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    sha256::hex(&sha256::digest(data))
}

/// Checks a pack file's bytes against the pin for its file `name` in
/// `sums`, the text of `scripts/design-pack.sha256` (`<sha256>  <name>`
/// per line, `#` starting a comment).
///
/// # Errors
///
/// When `sums` pins no pack of that name, or pins a different hash.
pub fn check_pin(pack: &[u8], name: &str, sums: &str) -> Result<(), String> {
    let got = sha256_hex(pack);
    let pinned = sums
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once("  "))
        .find(|(_, n)| n.trim() == name)
        .map(|(h, _)| h.trim());
    match pinned {
        None => Err(format!(
            "{name} is not pinned in scripts/design-pack.sha256; a build takes only a pinned pack"
        )),
        Some(h) if h.eq_ignore_ascii_case(&got) => Ok(()),
        Some(h) => Err(format!(
            "{name} has SHA-256 {got}, but scripts/design-pack.sha256 pins {h}; this is not the pinned pack"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Asset, Pack, check_pin, seal, sha256_hex, valid_path};

    fn plain() -> Vec<Asset> {
        vec![
            Asset {
                path: "tokens.json".into(),
                bytes: br##"{"themes":[{"name":"plain","accent":"#123456"}]}"##.to_vec(),
            },
            Asset {
                path: "forms/bar.json".into(),
                bytes: br#"{"name":"bar"}"#.to_vec(),
            },
        ]
    }

    #[test]
    fn sealing_the_same_files_twice_gives_the_same_pack_and_new_files_a_new_seed() {
        let one = seal(1, plain());
        assert_eq!(one.to_bytes(), seal(1, plain()).to_bytes());
        assert_eq!(one.assets[0].path, "forms/bar.json", "ordered by path");
        assert!(!one.to_bytes().windows(7).any(|w| w == b"#123456"));
        let mut changed = plain();
        changed[0].bytes[20] = b'q';
        assert_ne!(seal(1, changed).seed, one.seed);
    }

    fn tokens() -> Vec<Asset> {
        plain()[..1].to_vec()
    }

    #[test]
    fn a_pack_reads_back_and_opens() {
        let one = seal(7, tokens());
        let back = Pack::from_bytes(&one.to_bytes()).unwrap();
        assert_eq!(back, one);
        assert_eq!(back.file_name(), "tiles-design-7.pack");
        assert_eq!(
            back.open("tokens.json").unwrap(),
            r##"{"themes":[{"name":"plain","accent":"#123456"}]}"##
        );
        assert!(back.open("forms/line.json").is_err());
    }

    #[test]
    fn fill_seals_the_tree_forms_under_the_packs_own_seed() {
        let mut tokens_only = seal(1, tokens());
        let theirs = tokens_only.assets[0].clone();
        let mut local = plain();
        local[0].bytes = b"{\"other\":1}".to_vec();
        tokens_only.fill(local);
        assert_eq!(tokens_only.assets.len(), 2);
        assert!(
            tokens_only.assets.contains(&theirs),
            "the pack's tokens win"
        );
        assert_eq!(
            tokens_only.open("forms/bar.json").unwrap(),
            r#"{"name":"bar"}"#
        );
    }

    #[test]
    fn a_malformed_or_foreign_pack_is_refused() {
        let good = seal(1, tokens()).to_bytes();
        let text = String::from_utf8_lossy(&good).into_owned();
        let refused = |bytes: &[u8], why: &str| {
            let e = Pack::from_bytes(bytes).unwrap_err();
            assert!(e.contains(why), "{e}");
        };
        refused(b"hello", "no end line");
        refused(&good[..good.len() - 1], "ends before");
        refused(&[&good[..], b"x"].concat(), "follow the last asset");
        refused(&seal(1, plain()).to_bytes(), "is not tokens.json");
        refused(
            text.replacen("asset tokens.json", "asset forms/bar.json", 1)
                .as_bytes(),
            "is not tokens.json",
        );
        refused(
            text.replacen("tiles-design-pack 1", "tiles-design-pack 9", 1)
                .as_bytes(),
            "not a design pack",
        );
        // Another seed: the bytes no longer open.
        let seed_at = text.find("seed-a ").unwrap() + 7;
        let mut other = good.clone();
        other[seed_at] = if other[seed_at] == b'0' { b'1' } else { b'0' };
        refused(&other, "does not open");
        let head = &text[..text.find("asset ").unwrap()];
        refused(format!("{head}end\n").as_bytes(), "no tokens.json");
    }

    #[test]
    fn a_pack_whose_hash_is_not_pinned_is_refused() {
        let pack = seal(1, plain()).to_bytes();
        let hash = sha256_hex(&pack);
        let sums = format!("# comment\n{hash}  tiles-design-1.pack\n");
        assert_eq!(check_pin(&pack, "tiles-design-1.pack", &sums), Ok(()));
        let mut tampered = pack.clone();
        *tampered.last_mut().unwrap() ^= 1;
        let e = check_pin(&tampered, "tiles-design-1.pack", &sums).unwrap_err();
        assert!(e.contains(&format!("pins {hash}")), "{e}");
        let e = check_pin(&pack, "tiles-design-2.pack", &sums).unwrap_err();
        assert!(e.contains("is not pinned"), "{e}");
    }

    #[test]
    fn only_the_token_file_and_form_files_are_asset_paths() {
        for ok in [
            "tokens.json",
            "forms/bar.json",
            "forms/stacked-100-bar.json",
        ] {
            assert!(valid_path(ok), "{ok}");
        }
        for bad in [
            "",
            "forms/.json",
            "forms/../tokens.json",
            "forms/Bar.json",
            "/tokens.json",
            "forms/a b.json",
            "other.json",
        ] {
            assert!(!valid_path(bad), "{bad}");
        }
    }
}
