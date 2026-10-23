//! Property tests ADR-009 promises for the intent parser: valid intents cut,
//! spliced and flipped at random, and random JSON-shaped text, through
//! `parse_request` and `into_intent`. Nothing panics, and a refusal's text
//! never carries a character that can drive a terminal or hide text. A
//! warehouse query that parses renders its model references in place and
//! keeps every other byte of its SQL (ADR-031). A small
//! deterministic generator keeps the crate free of new dependencies and names
//! the seed of any failure.

use tiles_intent::text::is_unsafe_char;

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

const GOOD: &str = r#"{"v":1,"form":"stacked-bar","title":"Signups dipped in W3","subtitle":"weekly",
"claim":{"kind":"change","tone":"bad"},"fields":{"x":"week","y":"signups","series":"plan"},
"highlight":["W3",3],"unit":"eur","note":"launch",
"data":{"rows":[{"week":"W1","plan":"a","signups":412},{"week":"W2","plan":"b","signups":-0.5},
{"week":"W3","plan":"a","signups":null}]}}"#;

const OMNI: &str = r#"{"v":1,"form":"column","title":"Orders","claim":{"kind":"trend"},
"fields":{"x":"orders.created_at[week]","y":"orders.count"},
"data":{"omni":{"topic":"orders","fields":["orders.created_at[week]","orders.count"],"limit":20}}}"#;

const WAREHOUSE: &str = r#"{"v":1,"form":"column","title":"Paid volume","claim":{"kind":"trend"},
"fields":{"x":"month","y":"amount"},
"data":{"warehouse":{"sql":"WITH p AS (SELECT * FROM {{ ref('fct_payments') }}) SELECT month, SUM(a) AS amount FROM p JOIN {{ref('acme','stg_orgs',v=2)}} USING (id) -- {{ ref('x') }}","target":"mr:4521","limit":20}}}"#;

/// Pieces spliced into an intent: JSON syntax, hostile and invisible text,
/// numbers at and past every limit, and deep nesting.
const PIECES: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    ",",
    ":",
    "\"",
    "\\",
    "null",
    "true",
    "1e400",
    "-1e15",
    "1.7e308",
    "5e-324",
    "-0",
    "18446744073709551616",
    "\"\\u001b]52;c;eA==\\u0007\"",
    "\"\\u202e\\u2066\"",
    "\"\\udb40\\udc49\"",
    "\"\\u200b\\ufeff\\u00ad\"",
    "\"\\ud800\"",
    "\"日本語\"",
    "[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[",
    "{\"rows\":[]}",
    "\"form\":\"waterfall\"",
    "\"v\":2",
    "{{",
    "}}",
    "{{ ref('fct_x') }}",
    "{{- ref(\"a\", \"b\", v=3) -}}",
    "{{ source('crm', 'users') }}",
    "'",
    "v=2",
    "\"target\":\"prod\"",
];

fn mutate(rng: &mut XorShift, base: &str) -> String {
    let mut s = base.as_bytes().to_vec();
    for _ in 0..=rng.below(4) {
        let at = rng.below(s.len() + 1);
        match rng.below(4) {
            0 => s.truncate(at),
            1 => {
                let piece = PIECES[rng.below(PIECES.len())].as_bytes();
                s.splice(at..at, piece.iter().copied());
            }
            2 if at < s.len() => {
                let end = (at + rng.below(12)).min(s.len());
                s.drain(at..end);
            }
            _ if at < s.len() => s[at] ^= 1 << rng.below(8),
            _ => {}
        }
    }
    String::from_utf8_lossy(&s).into_owned()
}

fn check(seed: u64, json: &str) {
    match tiles_intent::parse_request(json) {
        Ok(request) => {
            if let Some(source) = request.warehouse() {
                renders_in_place(seed, source);
            }
            if let Err(e) = request.into_intent() {
                let text = e.to_string();
                assert!(!text.chars().any(is_unsafe_char), "seed {seed}: {text:?}");
            }
        }
        Err(e) => {
            let text = e.to_string();
            assert!(!text.chars().any(is_unsafe_char), "seed {seed}: {text:?}");
        }
    }
}

/// Each reference of a source that parsed is a whole `{{ ... }}` on
/// character boundaries, in order, and rendering replaces exactly those
/// bytes.
fn renders_in_place(seed: u64, source: &tiles_intent::source::WarehouseSource) {
    let references = source
        .references()
        .expect("a parsed source reads its references");
    let mut last = 0;
    for r in &references {
        assert!(r.span.start >= last, "seed {seed}: references overlap");
        let text = &source.sql[r.span.clone()];
        assert!(
            text.starts_with("{{") && text.ends_with("}}"),
            "seed {seed}: {text:?}"
        );
        last = r.span.end;
    }
    let rendered = source
        .render(|_| Ok::<_, tiles_intent::source::DataError>("`T`".to_string()))
        .expect("render");
    let mut rest = source.sql.clone();
    for r in references.iter().rev() {
        rest.replace_range(r.span.clone(), "`T`");
    }
    assert_eq!(rendered, rest, "seed {seed}");
    assert!(!rendered.contains("{{"), "seed {seed}: {rendered:?}");
}

#[test]
fn mutated_intents_never_panic_and_refusals_print_safely() {
    for seed in 1..=20_000u64 {
        let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let base = match seed % 5 {
            0 => OMNI,
            1 => WAREHOUSE,
            _ => GOOD,
        };
        check(seed, &mutate(&mut rng, base));
    }
}

#[test]
fn random_pieces_never_panic() {
    for seed in 1..=5_000u64 {
        let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let json: String = (0..rng.below(40))
            .map(|_| PIECES[rng.below(PIECES.len())])
            .collect();
        check(seed, &json);
    }
}

#[test]
fn the_seed_intents_parse() {
    for base in [GOOD, OMNI, WAREHOUSE] {
        assert!(tiles_intent::parse_request(base).is_ok(), "{base}");
    }
    let request = tiles_intent::parse_request(WAREHOUSE).unwrap();
    assert_eq!(request.warehouse().unwrap().references().unwrap().len(), 3);
}
