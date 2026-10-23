//! Kani proofs and their checker for the kitty encoder (ADR-009): nothing
//! from a chart reaches the terminal except base64 inside the encoder's own
//! framing.
//!
//! The proofs in `proofs.rs` cover the payload: [`encodes`] holds for
//! `tiles_term::b64::encode` of every short input. [`frames`], [`carries`]
//! and [`bare`] state the framing for every function in `tiles_term::kitty`
//! that writes a command: a run of `ESC _ G <keys> [; <payload>] ESC \`
//! frames, keys of digits, letters, `=`, `,` and `-` on the first frame
//! only, base64 payloads of at most `kitty::CHUNK` bytes, `m=1` on every
//! frame but the last and `m=0` on the last, and the payloads joined the
//! exact base64 of the PNG. Their harnesses did not finish on the
//! development Mac and are follow-ups; until one does, the framing property
//! is checked by the unit test below, which `check.sh` runs over the real
//! encoder's output and which also rejects an appended OSC 52 leak.
//!
//! Run with `sh verify/formal/kitty/check.sh`; see README.md for bounds.

#[cfg(kani)]
mod proofs;

/// One kitty command as the terminal reads it.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    /// The keys before the payload, `m=` included.
    pub control: &'a [u8],
    /// The payload after `;`, if there is one.
    pub payload: Option<&'a [u8]>,
}

/// The terminal's reading of `out`: the frames in order, or `None` when a
/// byte outside a frame, an unterminated frame or a byte the framing does
/// not allow shows up.
#[must_use]
pub fn frames(out: &[u8]) -> Option<Vec<Frame<'_>>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < out.len() {
        if out.get(at..at + 3)? != b"\x1b_G" {
            return None;
        }
        at += 3;
        let start = at;
        while *out.get(at)? != 0x1b {
            at += 1;
        }
        if *out.get(at + 1)? != b'\\' {
            return None;
        }
        let body = &out[start..at];
        at += 2;
        let (control, payload) = match body.iter().position(|b| *b == b';') {
            Some(semi) => (&body[..semi], Some(&body[semi + 1..])),
            None => (body, None),
        };
        if !control
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"=,-".contains(b))
        {
            return None;
        }
        if let Some(p) = payload
            && !p
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(b))
        {
            return None;
        }
        found.push(Frame { control, payload });
    }
    Some(found)
}

fn sextet(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Whether `text` is exactly the standard padded base64 of `bytes`
/// (RFC 4648 section 4): `4 * ceil(n / 3)` characters, the alphabet only,
/// `=` only as the last group's padding, and no stray bits under it.
///
/// It walks `bytes`, whose length the harness fixes, rather than decoding
/// `text`, so its loops have the same bounds on every path the solver takes.
#[must_use]
pub fn encodes(text: &[u8], bytes: &[u8]) -> bool {
    if text.len() != bytes.len().div_ceil(3) * 4 {
        return false;
    }
    text.chunks(4).zip(bytes.chunks(3)).all(|(quad, group)| {
        let mut n = 0u32;
        for (k, c) in quad.iter().enumerate() {
            n <<= 6;
            if k <= group.len() {
                let Some(s) = sextet(*c) else {
                    return false;
                };
                n |= s;
            } else if *c != b'=' {
                return false;
            }
        }
        let want = group
            .iter()
            .zip([16, 8, 0])
            .fold(0, |acc, (b, shift)| acc | (u32::from(*b) << shift));
        n == want
    })
}

/// Whether `out` is one command carrying `png` under keys starting with
/// `keys`: the property every payload-carrying encoder must keep.
#[must_use]
pub fn carries(out: &[u8], keys: &[u8], png: &[u8]) -> bool {
    let Some(frames) = frames(out) else {
        return false;
    };
    let Some(last) = frames.len().checked_sub(1) else {
        return false;
    };
    let mut data = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        let m: &[u8] = if i == last { b"m=0" } else { b"m=1" };
        let control_ok = if i == 0 {
            f.control.starts_with(keys)
                && f.control.len() > keys.len() + m.len()
                && f.control.ends_with(m)
                && f.control[f.control.len() - m.len() - 1] == b','
        } else {
            f.control == m
        };
        let Some(p) = f.payload else {
            return false;
        };
        if !control_ok || p.len() > tiles_term::kitty::CHUNK {
            return false;
        }
        data.extend_from_slice(p);
    }
    encodes(&data, png)
}

/// Whether `out` is one command with no payload: the property every other
/// encoder must keep.
#[must_use]
pub fn bare(out: &[u8]) -> bool {
    frames(out).is_some_and(|f| f.len() == 1 && f[0].payload.is_none())
}

#[cfg(test)]
mod tests {
    use super::{bare, carries, encodes, frames};
    use tiles_term::kitty;

    #[test]
    fn the_checker_accepts_the_encoder_and_rejects_a_leak() {
        let png = vec![7u8; kitty::CHUNK];
        assert!(carries(&kitty::transmit_png(4, &png), b"a=t,", &png));
        assert!(carries(&kitty::show_png(4, b"", 1, 1, None), b"a=T,", b""));
        assert!(bare(&kitty::place(4, 2, 40, 3, -1, None)));
        let mut leak = kitty::transmit_png(4, b"x");
        leak.extend_from_slice(b"\x1b]52;c;eA==\x07");
        assert!(frames(&leak).is_none());
        assert!(!carries(&kitty::transmit_png(4, b"x"), b"a=t,", b"y"));
    }

    #[test]
    fn encodes_reads_rfc_4648_and_refuses_the_rest() {
        assert!(encodes(b"Zm9vYg==", b"foob"));
        assert!(encodes(b"", b""));
        assert!(!encodes(b"Zg==Zg==", b"ff"));
        assert!(!encodes(b"Zh==", b"f"), "stray bits under the padding");
        assert!(!encodes(b"Zg=", b"f"));
        assert!(!encodes(b"Zm9w", b"foo"));
    }
}
