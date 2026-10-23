//! The placeholder block: one text token per row a card covers.
//!
//! A tool result carries the block and the model copies it into its reply
//! inside a ```` ```tiles ```` fence (ADR-052). The harness lays out one
//! line per token, which reserves the card's rows in its own scrollback;
//! `tiles run` finds the tokens on screen and draws the card over them. Without `tiles run` the
//! block reads as text, its first row holding the title.
//!
//! ```text
//! [tile:5fab86 01/12] Signups dipped in W3
//! [tile:5fab86 02/12]
//! ...
//! [tile:5fab86 12/12]
//! ```

/// Most rows a block can have: two digits.
pub const MAX_ROWS: u16 = 99;
/// Length of a token in characters: `[tile:` + 6 + space + `NN/NN` + `]`.
pub const TOKEN_LEN: usize = 19;

/// One token found on a screen row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The card id, six lowercase hex digits.
    pub id: String,
    /// The row of the card this line is, from 1.
    pub row: u16,
    /// How many rows the card has.
    pub rows: u16,
    /// The column the token starts at.
    pub col: usize,
}

/// Whether `id` is a card id: six lowercase hex digits.
#[must_use]
pub fn is_card_id(id: &str) -> bool {
    id.len() == 6
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The block's lines for card `id` covering `rows` rows, with the title on the
/// first line cut so the line fits `cols` columns. `rows` is clamped to 1 to
/// [`MAX_ROWS`].
#[must_use]
pub fn lines(id: &str, rows: u16, title: &str, cols: usize) -> Vec<String> {
    let rows = rows.clamp(1, MAX_ROWS);
    (1..=rows)
        .map(|r| {
            let token = format!("[tile:{id} {r:02}/{rows:02}]");
            if r == 1 {
                let room = cols.saturating_sub(TOKEN_LEN + 1).max(8);
                format!("{token} {}", crate::sanitize::line(title, room))
            } else {
                token
            }
        })
        .collect()
}

/// The tokens in a screen row, where `cells` holds one character per column.
#[must_use]
pub fn find(cells: &[char]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + TOKEN_LEN <= cells.len() {
        if let Some(token) = token_at(&cells[i..i + TOKEN_LEN], i) {
            out.push(token);
            i += TOKEN_LEN;
        } else {
            i += 1;
        }
    }
    out
}

fn token_at(c: &[char], col: usize) -> Option<Token> {
    let text: String = c.iter().collect();
    if c.len() != TOKEN_LEN || !could_become_token(&text) {
        return None;
    }
    let (row, rows) = (
        text[13..15].parse::<u16>().ok()?,
        text[16..18].parse::<u16>().ok()?,
    );
    (row >= 1 && row <= rows).then(|| Token {
        id: text[6..12].to_string(),
        row,
        rows,
        col,
    })
}

/// Where a token that is still being typed starts in a row, if one is: a
/// prefix of a token at the end of the row's text. The wrapper blanks it so
/// a half-typed token never flashes.
#[must_use]
pub fn partial(cells: &[char]) -> Option<usize> {
    let end = cells.iter().rposition(|c| *c != ' ' && *c != '\0')? + 1;
    let start = cells[..end].iter().rposition(|c| *c == '[')?;
    let piece: String = cells[start..end].iter().collect();
    if piece.len() >= TOKEN_LEN || !could_become_token(&piece) {
        return None;
    }
    Some(start)
}

fn could_become_token(piece: &str) -> bool {
    const SHAPE: &str = "[tile:hhhhhh dd/dd]";
    piece.chars().zip(SHAPE.chars()).all(|(c, s)| match s {
        'h' => c.is_ascii_digit() || ('a'..='f').contains(&c),
        'd' => c.is_ascii_digit(),
        s => c == s,
    }) && piece.len() >= 2
}

#[cfg(test)]
mod tests {
    use super::{Token, find, is_card_id, lines, partial};

    fn cells(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn lines_number_every_row_and_title_the_first() {
        let l = lines("5fab86", 3, "Signups dipped in W3", 80);
        assert_eq!(
            l,
            [
                "[tile:5fab86 01/03] Signups dipped in W3",
                "[tile:5fab86 02/03]",
                "[tile:5fab86 03/03]",
            ]
        );
        assert_eq!(lines("5fab86", 0, "t", 80).len(), 1);
        assert_eq!(lines("5fab86", 500, "t", 80).len(), 99);
    }

    #[test]
    fn a_hostile_title_reaches_the_block_as_plain_text() {
        let l = lines("5fab86", 1, "a\x1b]8;;https://x.test\x1b\\b\u{202e}c", 80);
        assert!(!l[0].contains('\x1b') && !l[0].contains('\u{202e}'));
    }

    #[test]
    fn find_reads_tokens_anywhere_on_a_row() {
        let row = cells("│ [tile:5fab86 02/12]    text [tile:00aa11 01/01] x");
        assert_eq!(
            find(&row),
            [
                Token {
                    id: "5fab86".into(),
                    row: 2,
                    rows: 12,
                    col: 2
                },
                Token {
                    id: "00aa11".into(),
                    row: 1,
                    rows: 1,
                    col: 30
                },
            ]
        );
    }

    #[test]
    fn malformed_tokens_are_not_found() {
        for s in [
            "[tile:5FAB86 01/02]",
            "[tile:5fab8g 01/02]",
            "[tile:5fab86 03/02]",
            "[tile:5fab86 00/02]",
            "[tile:5fab86 1/02] ",
            "[tile:5fab86 +1/02]",
            "tile:5fab86 01/02]",
            "[tile:5fab… 01/02]",
            "[tile:…5fab86 01/02]",
            "[tile:5fab86 0…/02]",
        ] {
            assert!(find(&cells(s)).is_empty(), "{s}");
        }
        assert!(is_card_id("0a1b2c") && !is_card_id("0a1b2"));
    }

    #[test]
    fn partial_tokens_at_the_end_of_a_row() {
        assert_eq!(partial(&cells("text [tile:5f   ")), Some(5));
        assert_eq!(partial(&cells("[t")), Some(0));
        assert_eq!(partial(&cells("[tile:5fab86 01/1")), Some(0));
        assert_eq!(partial(&cells("[tile:5fab86 01/12]")), None);
        assert_eq!(partial(&cells("an [array]")), None);
        assert_eq!(partial(&cells("[")), None);
        assert_eq!(partial(&cells("")), None);
    }
}
