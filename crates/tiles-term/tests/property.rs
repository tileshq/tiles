//! Property tests ADR-009 promises: random text from a hostile character
//! pool through every text path this crate owns. Nothing panics and nothing
//! that can drive a terminal or hide text comes out. A small deterministic
//! generator keeps the crate free of new dependencies and names the seed of
//! any failure.

use tiles_intent::text::is_unsafe_char;
use tiles_term::{block, detect, fallback, sanitize};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }
}

/// Characters chosen to break parsers: controls, escapes, bidi and tag
/// characters, wide and combining characters, and the block's own syntax.
const POOL: &[char] = &[
    'a',
    'Z',
    '0',
    '9',
    ' ',
    '[',
    ']',
    ':',
    '/',
    't',
    'i',
    'l',
    'e',
    'f',
    '\u{1b}',
    '\u{7}',
    '\u{0}',
    '\u{7f}',
    '\u{9b}',
    '\u{9c}',
    '\u{9d}',
    '\u{90}',
    '\n',
    '\r',
    '\t',
    '\u{202e}',
    '\u{2066}',
    '\u{2028}',
    '\u{2029}',
    '\u{200f}',
    '\u{61c}',
    '\u{E0049}',
    '\u{E007F}',
    '\u{feff}',
    '\u{301}',
    '本',
    '✅',
    '\u{10EEEE}',
    '\u{fffd}',
    '…',
];

fn text(rng: &mut XorShift, max: usize) -> String {
    (0..rng.below(max))
        .map(|_| POOL[rng.below(POOL.len())])
        .collect()
}

fn plain(s: &str) -> bool {
    !s.chars().any(is_unsafe_char)
}

#[test]
fn random_text_through_every_path_comes_out_plain() {
    for seed in 1..=3_000u64 {
        let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let s = text(&mut rng, 120);
        let max = rng.below(200);
        let line = sanitize::line(&s, max);
        assert!(plain(&line), "seed {seed}: line {line:?}");
        assert!(line.chars().count() <= max.max(1), "seed {seed}");
        let lines = sanitize::lines(&s);
        assert!(lines.lines().all(plain), "seed {seed}: lines {lines:?}");
        let values: Vec<String> = (0..rng.below(20)).map(|_| text(&mut rng, 40)).collect();
        for l in fallback::summary(&s, &values, &text(&mut rng, 40)) {
            assert!(plain(&l), "seed {seed}: summary {l:?}");
        }
        let rows = u16::try_from(rng.below(120)).unwrap();
        let cols = rng.below(300);
        for l in block::lines("a1b2c3", rows, &s, cols) {
            assert!(plain(&l), "seed {seed}: block {l:?}");
        }
    }
}

#[test]
fn random_screens_never_panic_the_block_parser() {
    for seed in 1..=3_000u64 {
        let mut rng = XorShift(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
        let mut row: Vec<char> = text(&mut rng, 160).chars().collect();
        if rng.below(2) == 0 {
            let at = rng.below(row.len() + 1);
            let token = format!("[tile:a1b2c3 {:02}/{:02}]", rng.below(120), rng.below(120));
            let cut = rng.below(token.len() + 1);
            row.splice(at..at, token.chars().take(cut));
        }
        for t in block::find(&row) {
            assert!(t.col < row.len(), "seed {seed}");
            assert!(block::is_card_id(&t.id), "seed {seed}");
        }
        if let Some(at) = block::partial(&row) {
            assert!(at < row.len(), "seed {seed}");
        }
    }
}

#[test]
fn random_replies_never_panic_the_reply_parser() {
    for seed in 1..=3_000u64 {
        let mut rng = XorShift(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let pieces: [&[u8]; 9] = [
            b"\x1b_Gi=31;OK\x1b\\",
            b"\x1bP>|WarpTerminal\x1b\\",
            b"\x1b[6;32;16t",
            b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\",
            b"\x1b[?62;4c",
            b"\x1b",
            b"\xff\xfe",
            b"\x1bP>|\x1b]52;c;x\x07\x1b\\",
            b"9999999999999999999;",
        ];
        let bytes: Vec<u8> = (0..rng.below(12))
            .flat_map(|_| {
                let p = pieces[rng.below(pieces.len())];
                p[..rng.below(p.len() + 1)].to_vec()
            })
            .collect();
        let replies = detect::Replies::parse(&bytes);
        if let Some(name) = &replies.xtversion {
            assert!(plain(name), "seed {seed}: {name:?}");
        }
    }
}
