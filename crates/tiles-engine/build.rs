//! Seals the forms and design tokens (ADR-022, ADR-036): each asset is
//! encrypted with `ChaCha20` into `$OUT_DIR/sealed/`, so a release binary
//! carries no readable JSON. The engine opens them in memory at load
//! (`src/seal.rs`).
//!
//! With `TILES_DESIGN_PACK=<file>` the build takes a sealed design pack
//! (`src/pack.rs`) from outside the source tree: it refuses a pack whose
//! SHA-256 is not the one `scripts/design-pack.sha256` pins for its name,
//! embeds the pack's seed and sealed tokens as they are, and seals the forms
//! in `assets/` under the pack's seed. Without it, the build seals the plain
//! tokens in `assets/plain/` and the forms in `assets/` under a seed derived
//! from the plaintext, so two builds of one tree embed the same bytes
//! (ADR-037).

use std::path::{Path, PathBuf};
use std::{env, fs};

#[path = "src/chacha.rs"]
mod chacha;
#[allow(dead_code)] // The engine also writes and names packs; the build only reads them.
#[path = "src/pack.rs"]
mod pack;
#[path = "src/sha256.rs"]
mod sha256;

const PACK_VAR: &str = "TILES_DESIGN_PACK";
/// The tokens a build without a design pack seals: the plain theme
/// (ADR-037).
const PLAIN_TOKENS: &str = "assets/plain/tokens.json";

fn main() {
    println!("cargo::rerun-if-changed={PLAIN_TOKENS}");
    println!("cargo::rerun-if-changed=assets/forms");
    for file in ["chacha.rs", "pack.rs", "sha256.rs"] {
        println!("cargo::rerun-if-changed=src/{file}");
    }
    println!("cargo::rerun-if-env-changed={PACK_VAR}");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let plain = pack::read(Path::new(PLAIN_TOKENS), Some(Path::new("assets/forms")))
        .unwrap_or_else(|e| fail(&e));
    let (sealed, source) = match env::var_os(PACK_VAR).filter(|v| !v.is_empty()) {
        None => (pack::seal(0, plain), "assets".to_string()),
        Some(file) => {
            let file = PathBuf::from(file);
            let mut sealed = taken(&file);
            sealed.fill(plain);
            let source = format!("pack {}", sealed.number);
            (sealed, source)
        }
    };
    println!("cargo::rustc-env=TILES_SEAL_SOURCE={source}");
    let words = |w: &[u32; 8]| {
        w.iter()
            .map(|x| format!("0x{:04x}_{:04x}", x >> 16, x & 0xffff))
            .collect::<Vec<_>>()
            .join(", ")
    };
    write(
        &out.join("seal_seed.rs"),
        format!(
            "pub(super) const SEED_A: [u32; 8] = [{}];\npub(super) const SEED_B: [u32; 8] = [{}];\n",
            words(&sealed.seed.0),
            words(&sealed.seed.1)
        )
        .as_bytes(),
    );
    let dir = out.join("sealed");
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap_or_else(|e| fail(&format!("{}: {e}", dir.display())));
    }
    for asset in &sealed.assets {
        let dest = dir.join(&asset.path);
        fs::create_dir_all(dest.parent().expect("a parent")).expect("sealed dir");
        write(&dest, &asset.bytes);
    }
}

/// The pack at `file`, checked against its pin and opened.
fn taken(file: &Path) -> pack::Pack {
    println!("cargo::rerun-if-changed={}", file.display());
    let sums = Path::new(&env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"))
        .join("../../scripts/design-pack.sha256");
    println!("cargo::rerun-if-changed={}", sums.display());
    let data = fs::read(file).unwrap_or_else(|e| refuse(file, &e.to_string()));
    let sums = fs::read_to_string(&sums)
        .unwrap_or_else(|e| refuse(file, &format!("{}: {e}", sums.display())));
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_else(|| refuse(file, "not a file name"));
    if let Err(e) = pack::check_pin(&data, name, &sums) {
        refuse(file, &e);
    }
    pack::Pack::from_bytes(&data).unwrap_or_else(|e| refuse(file, &e))
}

fn refuse(file: &Path, why: &str) -> ! {
    fail(&format!("{PACK_VAR}={}: {why}", file.display()))
}

fn write(path: &Path, data: &[u8]) {
    fs::write(path, data).unwrap_or_else(|e| fail(&format!("{}: {e}", path.display())));
}

/// Stops the build with one clear line.
fn fail(why: &str) -> ! {
    eprintln!("error: {why}");
    std::process::exit(1)
}
