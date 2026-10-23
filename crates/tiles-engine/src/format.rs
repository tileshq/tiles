//! Number formats (playbook T6): thousands separators, a true minus sign,
//! one decimal count per chart, compact millions and billions (and
//! thousands on a headline), and the intent's unit (ADR-012).

use tiles_intent::Unit;

/// How every number on one chart is written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberFormat {
    decimals: usize,
    divisor: f64,
    suffix: &'static str,
    unit: Option<Unit>,
    /// Each value to three significant figures with no trailing zeros, in
    /// place of `decimals`: a headline and its change.
    significant: bool,
}

impl NumberFormat {
    /// The format that writes every value in `values` with the fewest
    /// decimals (at most two) that keeps them exact to the shown precision.
    #[must_use]
    pub fn for_values(values: &[f64]) -> Self {
        Self::stepped(values, false)
    }

    /// For a headline and its change: a thousands step as well (262000
    /// reads 262K, as a title writes it), and each value to three
    /// significant figures with no trailing zeros, so 23600 reads 23.6K
    /// and a change of 700 beside it +0.7K. Axes and mark labels keep the
    /// full number. The step is picked after rounding, so 999600 reads 1M.
    #[must_use]
    pub fn for_headline(values: &[f64]) -> Self {
        let rounded: Vec<f64> = values.iter().map(|v| round_significant(*v)).collect();
        Self {
            significant: true,
            ..Self::stepped(&rounded, true)
        }
    }

    fn stepped(values: &[f64], thousands: bool) -> Self {
        let max = values.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let (divisor, suffix) = if max >= 1e9 {
            (1e9, "B")
        } else if max >= 1e6 {
            (1e6, "M")
        } else if thousands && max >= 1e3 {
            (1e3, "K")
        } else {
            (1.0, "")
        };
        let limit = 2;
        let decimals = (0..=limit)
            .find(|&d| {
                let k = 10_f64.powi(i32::try_from(d).unwrap_or(0));
                values.iter().all(|v| {
                    let x = v / divisor * k;
                    (x - x.round()).abs() < 1e-6 * k.max(1.0)
                })
            })
            .unwrap_or(limit);
        Self {
            decimals,
            divisor,
            suffix,
            unit: None,
            significant: false,
        }
    }

    /// The same format, writing `unit` with every value: `31%`, `€8.2M`.
    #[must_use]
    pub fn with_unit(self, unit: Option<Unit>) -> Self {
        Self { unit, ..self }
    }

    /// For axis ticks: thousands written `3k` when every tick is a whole
    /// thousand (and no larger compact applies), as the gallery's axes do.
    #[must_use]
    pub fn for_ticks(ticks: &[f64]) -> Self {
        let plain = Self::for_values(ticks);
        let max = ticks.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let thousands = ticks.iter().all(|v| (v / 1000.0).fract() == 0.0);
        if plain.suffix.is_empty() && max >= 1000.0 && thousands {
            Self {
                decimals: 0,
                divisor: 1000.0,
                suffix: "k",
                unit: None,
                significant: false,
            }
        } else {
            plain
        }
    }

    /// Writes one value.
    #[must_use]
    pub fn format(&self, v: f64) -> String {
        // A value far smaller than the chart's step takes its own, so no
        // number people compare reads as zero: 1,284 beside 1.28M, not
        // 0.00M, and a headline's change +$27K beside $1.03M, not +$0.027M.
        if self.divisor > 1.0 && v != 0.0 && (v / self.divisor).abs() < 0.1 {
            let own = if self.significant {
                Self::for_headline(&[v])
            } else {
                Self::for_values(&[v])
            };
            return own.with_unit(self.unit).format(v);
        }
        // Zero needs no compact suffix: an axis reads 0, 1k, 2k.
        let zero = self.divisor > 1.0 && v == 0.0;
        let x = v / self.divisor;
        let decimals = if zero {
            0
        } else if self.significant {
            significant_decimals(x)
        } else {
            self.decimals
        };
        let mut text = format!("{:.*}", decimals, x.abs());
        if self.significant && text.contains('.') {
            text = text.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        let (int, frac) = match text.split_once('.') {
            Some((i, f)) => (i.to_string(), Some(f.to_string())),
            None => (text, None),
        };
        let mut grouped = String::new();
        for (i, c) in int.chars().enumerate() {
            if i > 0 && (int.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        let negative = x < 0.0
            && grouped
                .chars()
                .chain(frac.iter().flat_map(|f| f.chars()))
                .any(|c| c != '0' && c != ',');
        let mut out = String::new();
        if negative {
            out.push('\u{2212}');
        }
        out.push_str(match self.unit {
            Some(Unit::Eur) => "€",
            Some(Unit::Usd) => "$",
            Some(Unit::Gbp) => "£",
            Some(Unit::Percent) | None => "",
        });
        out.push_str(&grouped);
        if let Some(f) = frac {
            out.push('.');
            out.push_str(&f);
        }
        if !zero {
            out.push_str(self.suffix);
        }
        if self.unit == Some(Unit::Percent) {
            out.push('%');
        }
        out
    }

    /// Writes a change with its sign: `+12`, `−8`, `0`.
    #[must_use]
    pub fn signed(&self, v: f64) -> String {
        let s = self.format(v);
        if s.starts_with('\u{2212}') || s.chars().filter(char::is_ascii_digit).all(|c| c == '0') {
            s
        } else {
            format!("+{s}")
        }
    }
}

/// `x` rounded to three significant figures: 999600 is 1000000.
fn round_significant(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    #[allow(clippy::cast_possible_truncation)] // A power of ten.
    let k = 10_f64.powi(2 - x.abs().log10().floor() as i32);
    (x * k).round() / k
}

/// Decimals that show `x` to three significant figures: 23.6 takes one,
/// 0.7 two (then trimmed), 262 none.
fn significant_decimals(x: f64) -> usize {
    if x == 0.0 || !x.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)] // A power of ten.
    let magnitude = x.abs().log10().floor() as i32;
    usize::try_from(2 - magnitude).unwrap_or(0).min(6)
}

#[cfg(test)]
mod tests {
    use super::NumberFormat;

    fn f(values: &[f64], v: f64) -> String {
        NumberFormat::for_values(values).format(v)
    }

    #[test]
    fn whole_numbers_get_separators_and_no_decimals() {
        assert_eq!(f(&[412.0, 431.0, 298.0], 431.0), "431");
        assert_eq!(f(&[1234.0, 98_765.0], 98_765.0), "98,765");
        assert_eq!(f(&[999_999.0], 999_999.0), "999,999");
    }

    #[test]
    fn decimals_are_shared_across_the_chart() {
        assert_eq!(f(&[1.5, 2.0], 2.0), "2.0");
        assert_eq!(f(&[0.125, 0.5], 0.5), "0.50");
    }

    #[test]
    fn millions_and_billions_are_compact() {
        assert_eq!(f(&[1_200_000.0, 3_000_000.0], 1_200_000.0), "1.2M");
        assert_eq!(f(&[4_820_000.0], 4_820_000.0), "4.82M");
        assert_eq!(f(&[2e9, 4e9], 4e9), "4B");
    }

    #[test]
    fn a_value_far_below_the_step_takes_its_own() {
        let values = [1_284_000.5, 1_000_000.0, 1284.0, 1.0, -42.5, 0.0];
        assert_eq!(f(&values, 1_284_000.5), "1.28M");
        assert_eq!(f(&values, 1284.0), "1,284");
        assert_eq!(f(&values, 1.0), "1");
        assert_eq!(f(&values, -42.5), "\u{2212}42.5");
        assert_eq!(f(&values, 0.0), "0");
        assert_eq!(f(&[3e9, 5e7], 5e7), "50M");
    }

    #[test]
    fn a_headline_steps_thousands_but_axes_do_not() {
        use tiles_intent::Unit;
        let usd = NumberFormat::for_headline(&[249_000.0, 262_000.0]).with_unit(Some(Unit::Usd));
        assert_eq!(usd.format(262_000.0), "$262K");
        assert_eq!(usd.signed(13_000.0), "+$13K");
        let users = NumberFormat::for_headline(&[79_400.0, 84_200.0]);
        assert_eq!(users.format(84_200.0), "84.2K");
        assert_eq!(
            NumberFormat::for_headline(&[412.0, 431.0]).format(431.0),
            "431"
        );
        assert_eq!(
            NumberFormat::for_headline(&[4_820_000.0]).format(4_820_000.0),
            "4.82M"
        );
        assert_eq!(f(&[262_000.0], 262_000.0), "262,000");
    }

    #[test]
    fn a_headline_has_three_significant_figures_and_no_trailing_zeros() {
        let users = NumberFormat::for_headline(&[22_900.0, 23_600.0]);
        assert_eq!(users.format(23_600.0), "23.6K");
        assert_eq!(users.signed(700.0), "+0.7K");
        let arr = NumberFormat::for_headline(&[4_460_000.0, 4_820_000.0, 5_000_000.0]);
        assert_eq!(arr.format(5_000_000.0), "5M");
        assert_eq!(arr.signed(360_000.0), "+0.36M");
        assert_eq!(arr.format(4_823_456.0), "4.82M");
        let usd = NumberFormat::for_headline(&[1_005_000.0, 1_032_000.0])
            .with_unit(Some(tiles_intent::Unit::Usd));
        assert_eq!(usd.format(1_032_000.0), "$1.03M");
        assert_eq!(usd.signed(27_000.0), "+$27K");
        assert_eq!(usd.signed(-27_000.0), "\u{2212}$27K");
        let edge = NumberFormat::for_headline(&[950_000.0, 999_600.0]);
        assert_eq!(edge.format(999_600.0), "1M");
        let usd = NumberFormat::for_headline(&[999_600.0]).with_unit(Some(tiles_intent::Unit::Usd));
        assert_eq!(usd.format(999_600.0), "$1M");
        assert_eq!(
            NumberFormat::for_headline(&[999_960_000.0]).format(999_960_000.0),
            "1B"
        );
        assert_eq!(
            NumberFormat::for_headline(&[998_000.0]).format(998_000.0),
            "998K"
        );
        let small = NumberFormat::for_headline(&[12.345, 3.0]);
        assert_eq!(small.format(12.345), "12.3");
        assert_eq!(small.signed(-0.5), "\u{2212}0.5");
    }

    #[test]
    fn units_wrap_the_number_and_its_sign() {
        use tiles_intent::Unit;
        let pct = NumberFormat::for_values(&[31.0, 45.0]).with_unit(Some(Unit::Percent));
        assert_eq!(pct.format(31.0), "31%");
        let eur = NumberFormat::for_values(&[8_200_000.0, -200_000.0]).with_unit(Some(Unit::Eur));
        assert_eq!(eur.format(8_200_000.0), "€8.2M");
        assert_eq!(eur.signed(-200_000.0), "\u{2212}€0.2M");
        assert_eq!(eur.signed(1_100_000.0), "+€1.1M");
        assert_eq!(eur.signed(0.0), "€0");
    }

    #[test]
    fn whole_thousand_ticks_are_compact() {
        let t = NumberFormat::for_ticks(&[0.0, 1000.0, 2000.0, 3000.0]);
        assert_eq!(t.format(3000.0), "3k");
        assert_eq!(t.format(0.0), "0");
        let t = NumberFormat::for_ticks(&[1200.0, 1300.0]);
        assert_eq!(t.format(1200.0), "1,200");
    }

    #[test]
    fn negatives_use_a_true_minus_and_signed_adds_plus() {
        let nf = NumberFormat::for_values(&[-1200.0, 300.0]);
        assert_eq!(nf.format(-1200.0), "\u{2212}1,200");
        assert_eq!(nf.signed(300.0), "+300");
        assert_eq!(nf.signed(-1200.0), "\u{2212}1,200");
        assert_eq!(nf.signed(0.0), "0");
        assert_eq!(nf.format(-0.0001), "0");
    }
}
