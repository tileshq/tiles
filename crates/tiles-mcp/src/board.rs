//! Several cards on one picture: a row of cards, or a dashboard of rows.
//! [`plan`] decides which cards share a line and how wide each is; [`place`]
//! puts the drawn cards on the picture, line under line.

use crate::card::{CARD_COLS, Fit, Look, MAX_CARDS};

/// A size or a position in cells: columns, rows.
pub type Cells = (u16, u16);

/// Columns between two cards in a row. Each card already keeps a margin
/// around its box, so one column reads as a gutter.
pub const GAP_COLS: u16 = 1;

/// The cards' lines, top to bottom, each card as its index (counting every
/// asked row's cards in order) and its width in columns, and the asked rows
/// that did not fit on one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The lines, top to bottom.
    pub rows: Vec<Vec<(usize, u16)>>,
    /// Each asked row split over several lines: its index, the columns its
    /// cards need side by side at their narrowest, margin included, and the
    /// lines it took.
    pub split: Vec<Split>,
    /// Whether scorecards were moved out of a row they shared with charts,
    /// or up from a lower row, into the top rows.
    pub moved: bool,
}

/// One card as asked for: its widths, and whether it is a scorecard (one
/// headline number), which a dashboard puts in its top rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ask {
    /// Its form's widths for its rows.
    pub fit: Fit,
    /// Whether it is a scorecard.
    pub scorecard: bool,
}

/// An asked row the terminal was too narrow for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split {
    /// Which row of the dashboard as arranged ([`plan`]), from 0.
    pub row: usize,
    /// Its cards.
    pub cards: usize,
    /// Terminal columns its cards need side by side.
    pub need: u16,
    /// Lines it took.
    pub lines: usize,
}

/// One card's widths in columns: the narrowest that reads, what its
/// content asks for, the widest it stretches to.
type Widths = (u16, u16, u16);

/// Columns a row of `widths` covers, gaps included.
#[must_use]
pub fn span(widths: &[u16]) -> u16 {
    let gaps = u16::try_from(widths.len().saturating_sub(1)).unwrap_or(u16::MAX);
    widths
        .iter()
        .fold(0u16, |a, w| a.saturating_add(*w))
        .saturating_add(gaps.saturating_mul(GAP_COLS))
}

/// One card's widths in this terminal: the narrowest that still reads, what
/// its content asks for, and the widest it stretches to so a line reaches
/// the dashboard's width. A form whose range tops out under [`CARD_COLS`]
/// (a scorecard, a slope chart) never passes its own maximum; the others,
/// charts that read better wide, stretch as far as the line needs.
fn widths(fit: Fit, look: &Look) -> Widths {
    let ideal = look.card_cols(fit);
    let min = fit.min.max(look.min_cols()).min(ideal);
    let cap = if fit.max >= CARD_COLS.1 {
        u16::MAX
    } else {
        fit.max.max(ideal)
    };
    (min, ideal, cap)
}

/// The way to split one asked row of cards over lines, in order, that
/// fits the room at the cards' narrowest: the fewest lines, then the lines
/// whose widths (as asked for) differ least, so three scorecards over 80
/// columns take two and one and a line chart beside two scorecards puts
/// the scorecards together; then the fuller lines first.
fn split_row(cards: &[Widths], room: u16) -> Vec<std::ops::Range<usize>> {
    let n = cards.len();
    let cuts = u32::try_from(n.saturating_sub(1)).unwrap_or(0).min(15);
    let lines = |mask: u32| {
        let mut out = Vec::new();
        let mut at = 0;
        for i in 0..n {
            if i + 1 == n || mask & (1 << i) != 0 {
                out.push(at..i + 1);
                at = i + 1;
            }
        }
        out
    };
    let widths = |r: &std::ops::Range<usize>, pick: fn(&Widths) -> u16| {
        span(&cards[r.clone()].iter().map(pick).collect::<Vec<_>>())
    };
    (0..1u32 << cuts)
        .map(lines)
        .filter(|ls| ls.iter().all(|r| widths(r, |c| c.0) <= room))
        .min_by_key(|ls| {
            let spans: Vec<u16> = ls.iter().map(|r| widths(r, |c| c.1)).collect();
            let spread = spans.iter().max().unwrap_or(&0) - spans.iter().min().unwrap_or(&0);
            let lens: Vec<std::cmp::Reverse<usize>> =
                ls.iter().map(|r| std::cmp::Reverse(r.len())).collect();
            (ls.len(), spread, lens)
        })
        .unwrap_or_else(|| (0..n).map(|i| i..i + 1).collect())
}

/// Widths for one line of cards, `(min, ideal, cap)` each, as close to
/// `target` columns as their ranges allow: what each asks for, narrowed
/// alike when the line is too wide, widened alike when it is short. Columns
/// left over by the even step go one each to the first cards that can take
/// them.
fn fill(cards: &[Widths], target: u16) -> Vec<u16> {
    let gaps = span(&vec![0; cards.len()]);
    let avail = u32::from(target.saturating_sub(gaps));
    let sum = |w: &[u16]| w.iter().map(|w| u32::from(*w)).sum::<u32>();
    let ideal: Vec<u16> = cards.iter().map(|c| c.1).collect();
    let mut out = if sum(&ideal) > avail {
        // The widest cap whose line still fits, never under a card's min.
        let at = |cap: u16| -> Vec<u16> { cards.iter().map(|c| c.1.min(cap.max(c.0))).collect() };
        let top = ideal.iter().copied().max().unwrap_or(0);
        (0..=top)
            .rev()
            .map(at)
            .find(|w| sum(w) <= avail)
            .unwrap_or_else(|| cards.iter().map(|c| c.0).collect())
    } else {
        // The highest floor the line has room for, never over a card's cap.
        let at =
            |floor: u16| -> Vec<u16> { cards.iter().map(|c| c.1.max(floor.min(c.2))).collect() };
        let mut lo = 0u16;
        let mut hi = target;
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if sum(&at(mid)) <= avail {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        at(lo)
    };
    let shrunk = sum(&out) < sum(&ideal);
    let mut left = avail.saturating_sub(sum(&out));
    for (w, c) in out.iter_mut().zip(cards) {
        let room = if shrunk { c.1 } else { c.2 };
        if left > 0 && *w < room {
            *w += 1;
            left -= 1;
        }
    }
    out
}

/// The rows a dashboard is drawn in, each card as its index in the order
/// asked: every scorecard first, in order, sharing the top rows (as even as
/// can be, at most [`MAX_CARDS`] to a row: five take three and two), then
/// each asked row's other cards in a row of their own. A scorecard beside
/// a chart would be stretched to the chart's height, a tall card with one
/// number in it.
fn arrange(rows: &[Vec<Ask>]) -> Vec<Vec<usize>> {
    let mut at = 0;
    let mut cards = Vec::new();
    let mut charts = Vec::new();
    for row in rows {
        let mut others = Vec::new();
        for ask in row {
            if ask.scorecard {
                cards.push(at);
            } else {
                others.push(at);
            }
            at += 1;
        }
        if !others.is_empty() {
            charts.push(others);
        }
    }
    let mut out = Vec::new();
    if !cards.is_empty() {
        let k = cards.len().div_ceil(MAX_CARDS);
        let (each, extra) = (cards.len() / k, cards.len() % k);
        let mut from = 0;
        for i in 0..k {
            let len = each + usize::from(i < extra);
            out.push(cards[from..from + len].to_vec());
            from += len;
        }
    }
    out.extend(charts);
    out
}

/// The lines for `rows` of cards in this terminal, as [`arrange`] orders
/// them. Each row stays one line when its cards fit side by side at their
/// narrowest; otherwise it splits over the fewest lines that fit, in
/// order. The dashboard is as wide as its widest line asks for, within the
/// room, and every line is fitted to that width ([`fill`]), so its edges
/// line up where the forms allow.
#[must_use]
pub fn plan(rows: &[Vec<Ask>], look: &Look) -> Plan {
    let room = look.room();
    let asks: Vec<Ask> = rows.iter().flatten().copied().collect();
    let arranged = arrange(rows);
    let mut asked = Vec::new();
    let mut at = 0;
    for row in rows {
        asked.push((at..at + row.len()).collect::<Vec<_>>());
        at += row.len();
    }
    let moved = arranged != asked;
    let mut lines: Vec<Vec<(usize, Widths)>> = Vec::new();
    let mut split = Vec::new();
    for (r, row) in arranged.iter().enumerate() {
        let cards: Vec<(usize, Widths)> = row
            .iter()
            .map(|i| (*i, widths(asks[*i].fit, look)))
            .collect();
        let parts = split_row(&cards.iter().map(|(_, w)| *w).collect::<Vec<_>>(), room);
        if parts.len() > 1 {
            let mins: Vec<u16> = cards.iter().map(|(_, w)| w.0).collect();
            split.push(Split {
                row: r,
                cards: cards.len(),
                need: span(&mins).saturating_add(look.term_cols - room),
                lines: parts.len(),
            });
        }
        for part in parts {
            lines.push(cards[part].to_vec());
        }
    }
    let ideal = |line: &[(usize, Widths)]| span(&line.iter().map(|(_, w)| w.1).collect::<Vec<_>>());
    let width = lines.iter().map(|l| ideal(l)).max().unwrap_or(0).min(room);
    Plan {
        rows: lines
            .iter()
            .map(|line| {
                let cards: Vec<Widths> = line.iter().map(|(_, w)| *w).collect();
                line.iter()
                    .map(|(i, _)| *i)
                    .zip(fill(&cards, width.max(ideal(line).min(room))))
                    .collect()
            })
            .collect(),
        split,
        moved,
    }
}

/// Where each drawn card lands, in cells from the picture's top left, for
/// rows of card sizes in cells; and the picture's size in cells. Rows sit
/// directly under one another: each card's own margin keeps them apart.
#[must_use]
pub fn place(sizes: &[Vec<Cells>]) -> (Vec<Vec<Cells>>, Cells) {
    let mut at = Vec::with_capacity(sizes.len());
    let (mut width, mut top) = (0u16, 0u16);
    for row in sizes {
        let mut left = 0u16;
        let mut line = Vec::with_capacity(row.len());
        for (cols, _) in row {
            line.push((left, top));
            left = left.saturating_add(*cols).saturating_add(GAP_COLS);
        }
        let widths: Vec<u16> = row.iter().map(|(c, _)| *c).collect();
        width = width.max(span(&widths));
        top = top.saturating_add(row.iter().map(|(_, r)| *r).max().unwrap_or(0));
        at.push(line);
    }
    (at, (width, top))
}

#[cfg(test)]
mod tests {
    use super::{Ask, GAP_COLS, Plan, Split, place};
    use crate::card::{Fit, Look};

    fn look(cols: u16) -> Look {
        Look {
            term_cols: cols,
            ..Look::default()
        }
    }

    /// A scorecard among others: compact, its form's narrowest width.
    const KPI: Fit = Fit {
        min: 35,
        ideal: 35,
        max: 35,
    };
    /// A line chart over 12 months.
    const LINE: Fit = Fit {
        min: 48,
        ideal: 72,
        max: 72,
    };
    /// A bar chart with short labels.
    const BAR: Fit = Fit {
        min: 44,
        ideal: 52,
        max: 72,
    };
    /// A slope chart, whose range stops short of the widest card.
    const SLOPE: Fit = Fit {
        min: 44,
        ideal: 50,
        max: 56,
    };

    /// Plans rows of fits, each a scorecard when it is [`KPI`].
    fn plan(rows: &[Vec<Fit>], look: &Look) -> Plan {
        let asks: Vec<Vec<Ask>> = rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|f| Ask {
                        fit: *f,
                        scorecard: *f == KPI,
                    })
                    .collect()
            })
            .collect();
        super::plan(&asks, look)
    }

    fn widths(p: &Plan) -> Vec<Vec<u16>> {
        p.rows
            .iter()
            .map(|l| l.iter().map(|(_, w)| *w).collect())
            .collect()
    }

    fn order(p: &Plan) -> Vec<usize> {
        p.rows.iter().flatten().map(|(i, _)| *i).collect()
    }

    /// Three scorecards over a line chart, the dashboard a model sends for
    /// "revenue, active users and churn, and revenue by month".
    fn kpis_over_a_line() -> Vec<Vec<Fit>> {
        vec![vec![KPI, KPI, KPI], vec![LINE]]
    }

    #[test]
    fn at_200_columns_the_line_chart_stretches_under_the_three_scorecards() {
        let p = plan(&kpis_over_a_line(), &look(200));
        assert_eq!(widths(&p), [vec![35, 35, 35], vec![107]]);
        assert_eq!(order(&p), [0, 1, 2, 3]);
        assert_eq!(p.split, []);
    }

    #[test]
    fn at_120_columns_the_dashboard_keeps_its_rows() {
        let p = plan(&kpis_over_a_line(), &look(120));
        assert_eq!(widths(&p), [vec![35, 35, 35], vec![107]]);
        assert_eq!(p.split, []);
    }

    #[test]
    fn at_80_columns_three_scorecards_split_two_and_one() {
        let p = plan(&kpis_over_a_line(), &look(80));
        assert_eq!(widths(&p), [vec![35, 35], vec![35], vec![72]]);
        assert_eq!(order(&p), [0, 1, 2, 3]);
        assert_eq!(
            p.split,
            [Split {
                row: 0,
                cards: 3,
                need: 115,
                lines: 2
            }]
        );
        for line in widths(&p) {
            assert!(super::span(&line) <= look(80).room());
        }
    }

    #[test]
    fn scorecards_leave_a_row_they_share_with_a_chart_for_the_top_row() {
        // The live ask as the model sent it: all four in one row.
        let row = vec![vec![KPI, KPI, KPI, LINE]];
        for cols in [200, 120] {
            let p = plan(&row, &look(cols));
            assert_eq!(widths(&p), [vec![35, 35, 35], vec![107]], "{cols}");
            assert_eq!(order(&p), [0, 1, 2, 3]);
            assert!(p.moved);
        }
        // A chart first, scorecards after, and scorecards in a lower row
        // all move up.
        let p = plan(&[vec![LINE, KPI], vec![BAR], vec![KPI]], &look(200));
        assert_eq!(order(&p), [1, 3, 0, 2]);
        assert_eq!(widths(&p), [vec![35, 35], vec![72], vec![72]]);
        // Five scorecards take three and two.
        let p = plan(&[vec![KPI; 4], vec![KPI]], &look(200));
        assert_eq!(widths(&p), [vec![35, 35, 35], vec![35, 35]]);
        // Rows already laid out this way do not move.
        assert!(!plan(&kpis_over_a_line(), &look(200)).moved);
        assert!(!plan(&[vec![BAR, LINE]], &look(200)).moved);
    }

    #[test]
    fn mixed_rows_line_up_on_the_widest_line() {
        // A bar chart and a line chart over two bars: the lines fill to the
        // widest line's 125 columns at 200.
        let rows = vec![vec![BAR, LINE], vec![BAR, BAR]];
        let p = plan(&rows, &look(200));
        assert_eq!(widths(&p), [vec![52, 72], vec![62, 62]]);
        // A slope chart stops at its own maximum.
        let p = plan(&[vec![LINE, LINE], vec![SLOPE]], &look(200));
        assert_eq!(widths(&p), [vec![72, 72], vec![56]]);
    }

    #[test]
    fn a_tight_row_narrows_its_widest_cards_first() {
        // 106 columns less the margin leave 98: both narrow to 48, the line
        // chart's narrowest, and the bar chart takes the column left over.
        let p = plan(&[vec![BAR, LINE]], &look(106));
        assert_eq!(widths(&p), [vec![49, 48]]);
        // Two scorecards in 80 columns fit; at 78 they split.
        assert_eq!(widths(&plan(&[vec![KPI, KPI]], &look(80))), [vec![35, 35]]);
        let p = plan(&[vec![KPI, KPI]], &look(78));
        assert_eq!(widths(&p), [vec![35], vec![35]]);
        assert_eq!(p.split[0].need, 79);
    }

    #[test]
    fn one_card_never_splits() {
        let p = plan(&[vec![LINE]], &look(60));
        assert_eq!(p.split, []);
        assert_eq!(widths(&p), [vec![52]]);
    }

    #[test]
    fn cards_land_left_to_right_then_row_under_row() {
        let (at, size) = place(&[vec![(42, 15), (40, 17)], vec![(72, 20)]]);
        assert_eq!(at, [vec![(0, 0), (42 + GAP_COLS, 0)], vec![(0, 17)]]);
        assert_eq!(size, (82 + GAP_COLS, 37));
    }
}
