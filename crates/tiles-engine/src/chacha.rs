//! `ChaCha20` (RFC 8439) and the asset key derivation (ADR-022). The build
//! script seals the forms and design tokens with this file and the engine
//! opens them with it, so both sides cannot drift.

const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];
/// The nonce of the block that turns the two seed halves into the key.
const DERIVE: [u32; 3] = [0x7469_6c65, 0x732d_6b65, 0x7900_0001];

#[allow(clippy::many_single_char_names)] // RFC 8439 names them a, b, c, d.
fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

/// One 64-byte keystream block, as sixteen little-endian words.
pub fn block(key: &[u32; 8], counter: u32, nonce: &[u32; 3]) -> [u32; 16] {
    let mut init = [0u32; 16];
    init[..4].copy_from_slice(&SIGMA);
    init[4..12].copy_from_slice(key);
    init[12] = counter;
    init[13..].copy_from_slice(nonce);
    let mut s = init;
    for _ in 0..10 {
        quarter(&mut s, 0, 4, 8, 12);
        quarter(&mut s, 1, 5, 9, 13);
        quarter(&mut s, 2, 6, 10, 14);
        quarter(&mut s, 3, 7, 11, 15);
        quarter(&mut s, 0, 5, 10, 15);
        quarter(&mut s, 1, 6, 11, 12);
        quarter(&mut s, 2, 7, 8, 13);
        quarter(&mut s, 3, 4, 9, 14);
    }
    for (w, i) in s.iter_mut().zip(init) {
        *w = w.wrapping_add(i);
    }
    s
}

/// XORs `data` with the keystream from block 1 on. Sealing and opening are
/// the same operation.
pub fn apply(key: &[u32; 8], nonce: &[u32; 3], data: &mut [u8]) {
    for (counter, chunk) in (1u32..).zip(data.chunks_mut(64)) {
        let stream = block(key, counter, nonce);
        for (i, byte) in chunk.iter_mut().enumerate() {
            *byte ^= stream[i / 4].to_le_bytes()[i % 4];
        }
    }
}

/// The nonce for an asset, from its path under `assets/` (FNV-1a), so no
/// two assets share a keystream.
pub fn nonce(path: &str) -> [u32; 3] {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    let len = u32::try_from(path.len()).unwrap_or(u32::MAX);
    let bytes = hash.to_le_bytes();
    let (halves, _) = bytes.as_chunks::<4>();
    [
        u32::from_le_bytes(halves[0]),
        u32::from_le_bytes(halves[1]),
        len,
    ]
}

/// The asset key, from the two seed halves the build script wrote. The key
/// itself is never stored: it is one `ChaCha20` block over the mixed halves.
pub fn derive(a: &[u32; 8], b: &[u32; 8]) -> [u32; 8] {
    let mut seed = [0u32; 8];
    for (i, (w, x)) in (0u32..).zip(seed.iter_mut().zip(a)) {
        *w = x ^ b[7 - i as usize].rotate_left(i * 3 + 1);
    }
    let out = block(&seed, 0, &DERIVE);
    let mut key = [0u32; 8];
    key.copy_from_slice(&out[..8]);
    key
}
