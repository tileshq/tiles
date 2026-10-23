//! Chart forms as data (ADR-002, ADR-003). Each file in `assets/forms/`
//! names a form, the roles a model fills and the layout that draws it. The
//! release binary only uses the embedded copies, sealed at build time and
//! opened in memory at load (ADR-022); a debug build reads them
//! from `$TILES_DEV_ASSETS/forms/` when that variable is set and the
//! directory exists, so a form can be tried without a recompile.

use serde::Deserialize;
use tiles_intent::Intent;

/// The embedded form files, name and sealed JSON.
pub const EMBEDDED: &[(&str, &[u8])] = &[
    ("column", crate::seal::sealed!("forms/column.json")),
    ("bar", crate::seal::sealed!("forms/bar.json")),
    ("line", crate::seal::sealed!("forms/line.json")),
    ("area", crate::seal::sealed!("forms/area.json")),
    (
        "area-labelled",
        crate::seal::sealed!("forms/area-labelled.json"),
    ),
    ("big-number", crate::seal::sealed!("forms/big-number.json")),
    ("multi-line", crate::seal::sealed!("forms/multi-line.json")),
    (
        "grouped-bar",
        crate::seal::sealed!("forms/grouped-bar.json"),
    ),
    (
        "stacked-bar",
        crate::seal::sealed!("forms/stacked-bar.json"),
    ),
    (
        "stacked-100-bar",
        crate::seal::sealed!("forms/stacked-100-bar.json"),
    ),
    ("lollipop", crate::seal::sealed!("forms/lollipop.json")),
    ("dot-plot", crate::seal::sealed!("forms/dot-plot.json")),
    ("slope", crate::seal::sealed!("forms/slope.json")),
    ("dumbbell", crate::seal::sealed!("forms/dumbbell.json")),
    ("bullet", crate::seal::sealed!("forms/bullet.json")),
    ("waterfall", crate::seal::sealed!("forms/waterfall.json")),
    ("scatter", crate::seal::sealed!("forms/scatter.json")),
];

/// The mark layouts the compiled core provides. A form picks one; a form
/// that needs a new kind of mark is Rust work (ADR-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// Vertical bars from zero.
    Columns,
    /// Horizontal bars from zero.
    Bars,
    /// Lines with points, optionally filled, optionally several series.
    Line,
    /// One big number.
    Kpi,
    /// Side-by-side columns per category.
    Grouped,
    /// Stacked columns.
    Stacked,
    /// Horizontal bars of shares stacked to 100%.
    Shares,
    /// Horizontal stems with a dot.
    Lollipop,
    /// Horizontal dots on tracks.
    Dots,
    /// Two-period slopes.
    Slope,
    /// Two dots joined per row.
    Dumbbell,
    /// A bar on a track with a target tick.
    Bullet,
    /// Floating signed steps.
    Waterfall,
    /// Points on two measures.
    Scatter,
}

impl Layout {
    /// The roles this layout reads, with kind and whether a form must
    /// require them.
    #[must_use]
    pub fn roles(self) -> &'static [(&'static str, RoleKind, bool)] {
        use RoleKind::{Category, Number};
        match self {
            Layout::Columns | Layout::Bars | Layout::Dots | Layout::Waterfall => {
                &[("x", Category, true), ("y", Number, true)]
            }
            Layout::Lollipop => &[
                ("x", Category, true),
                ("y", Number, true),
                ("average", Number, false),
            ],
            Layout::Line => &[
                ("x", Category, true),
                ("y", Number, true),
                ("series", Category, false),
            ],
            Layout::Grouped | Layout::Stacked | Layout::Shares | Layout::Slope => &[
                ("x", Category, true),
                ("y", Number, true),
                ("series", Category, true),
            ],
            Layout::Kpi => &[
                ("y", Number, true),
                ("x", Category, false),
                ("target", Number, false),
            ],
            Layout::Dumbbell => &[
                ("x", Category, true),
                ("lo", Number, true),
                ("hi", Number, true),
            ],
            Layout::Bullet => &[
                ("x", Category, true),
                ("y", Number, true),
                ("target", Number, true),
            ],
            Layout::Scatter => &[
                ("x", Number, true),
                ("y", Number, true),
                ("label", Category, false),
            ],
        }
    }
}

/// What a role's column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoleKind {
    /// Labels: text, or numbers read as labels.
    Category,
    /// Numbers.
    Number,
}

/// One role a model fills with a column name.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    /// The key under `fields`.
    pub name: String,
    /// What the column must hold.
    pub kind: RoleKind,
    /// Whether the intent must name it.
    pub required: bool,
    /// What it means, for the form list a model reads.
    pub doc: String,
}

/// How rows are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    /// As given: periods stay in time order.
    #[default]
    Data,
    /// Largest first, for rankings.
    ValueDesc,
}

/// Form options a layout reads.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    /// Row order.
    #[serde(default)]
    pub sort: Sort,
    /// Fill under the line from zero.
    #[serde(default)]
    pub area: bool,
    /// The named variant: edge the fill with its line and write the value
    /// at each highlighted period, which the approved card leaves unwritten.
    #[serde(default)]
    pub label_highlight: bool,
}

/// How wide a card of the form wants to be, in terminal columns: `min`, plus
/// `per_point` for each distinct `x` value and `per_label_char` for each
/// character of the longest `x` label, up to `max`. A big number needs few
/// columns whatever its rows; a column chart widens with its periods, a bar
/// chart with its labels.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Width {
    /// Narrowest card that still reads.
    pub min: u16,
    /// Widest card worth drawing.
    pub max: u16,
    /// Columns each distinct `x` value adds.
    #[serde(default)]
    pub per_point: f32,
    /// Columns each character of the longest `x` label adds.
    #[serde(default)]
    pub per_label_char: f32,
}

impl Default for Width {
    /// The width every card had before forms stated one.
    fn default() -> Self {
        Self {
            min: 40,
            max: 72,
            per_point: 0.0,
            per_label_char: 0.0,
        }
    }
}

/// A form's width for one intent: the narrowest card that reads and the one
/// its content asks for, in columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    /// The form's minimum.
    pub min: u16,
    /// What this intent's content asks for, within the form's range.
    pub ideal: u16,
    /// The form's maximum.
    pub max: u16,
}

/// A chart form, loaded from its data file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Form {
    /// The name a model puts in `form`.
    pub name: String,
    /// One sentence on when to use it.
    pub summary: String,
    /// The layout that draws it.
    pub layout: Layout,
    /// How wide its card wants to be.
    #[serde(default)]
    pub width: Width,
    /// The roles in `fields`.
    pub roles: Vec<Role>,
    /// Layout options.
    #[serde(default)]
    pub options: Options,
}

impl Form {
    /// Whether the form is a scorecard (one headline number), which a row
    /// of cards draws compact.
    #[must_use]
    pub fn is_scorecard(&self) -> bool {
        self.layout == Layout::Kpi
    }

    /// Whether the form colours the values `highlight` names; a big number
    /// ignores a highlight rather than refusing it.
    #[must_use]
    pub fn takes_highlight(&self) -> bool {
        self.layout != Layout::Kpi
    }

    /// The width a card of this form wants for `intent`'s rows.
    #[must_use]
    pub fn fit(&self, intent: &Intent) -> Fit {
        let w = self.width;
        let (points, longest) = intent.column("x").map_or((0, 0), |x| {
            let labels: std::collections::BTreeSet<String> =
                intent.table.rows.iter().map(|r| r[x].label()).collect();
            let longest = labels.iter().map(|l| l.chars().count()).max();
            (labels.len(), longest.unwrap_or(0))
        });
        #[allow(clippy::cast_precision_loss)] // Row and label counts are small.
        let want =
            f32::from(w.min) + w.per_point * points as f32 + w.per_label_char * longest as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped to u16 range.
        let ideal = (want.ceil().clamp(0.0, f32::from(u16::MAX)) as u16).clamp(w.min, w.max);
        Fit {
            min: w.min,
            ideal,
            max: w.max,
        }
    }

    /// Parses a form file and checks it against its layout.
    ///
    /// # Errors
    ///
    /// A one-line reason naming the file.
    pub fn from_json(file: &str, json: &str) -> Result<Self, String> {
        let form: Form = serde_json::from_str(json).map_err(|e| format!("{file}: {e}"))?;
        if form.width.min == 0
            || form.width.min > form.width.max
            || !(form.width.per_point >= 0.0 && form.width.per_label_char >= 0.0)
        {
            return Err(format!(
                "{file}: width needs 0 < min <= max and steps of 0 or more"
            ));
        }
        if form.name.is_empty()
            || !form
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(format!(
                "{file}: name must be lowercase letters, digits and dashes"
            ));
        }
        for (role, kind, required) in form.layout.roles() {
            match form.roles.iter().find(|r| r.name == *role) {
                Some(r) if r.kind != *kind => {
                    return Err(format!("{file}: role {role} must be {kind:?}"));
                }
                Some(r) if *required && !r.required => {
                    return Err(format!(
                        "{file}: role {role} must be required for this layout"
                    ));
                }
                None if *required => return Err(format!("{file}: layout needs role {role}")),
                _ => {}
            }
        }
        for r in &form.roles {
            if !form
                .layout
                .roles()
                .iter()
                .any(|(name, _, _)| *name == r.name)
            {
                return Err(format!(
                    "{file}: layout {:?} has no role {}",
                    form.layout, r.name
                ));
            }
        }
        Ok(form)
    }
}

/// Loads the forms: from the dev directory's `forms/` in a debug build
/// when it has one, else embedded.
///
/// # Errors
///
/// The first form file that fails to parse or check, or a duplicate name.
pub fn load(dev_dir: Option<&std::path::Path>) -> Result<Vec<Form>, String> {
    let mut forms = Vec::new();
    match dev_dir.map(|d| d.join("forms")).filter(|d| d.is_dir()) {
        Some(dir) => {
            let mut paths: Vec<_> = std::fs::read_dir(&dir)
                .map_err(|e| format!("{}: {e}", dir.display()))?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect();
            // By form name, so `area` comes before `area-labelled` as in EMBEDDED order checks.
            paths.sort_by(|a, b| a.file_stem().cmp(&b.file_stem()));
            for p in paths {
                let text =
                    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                forms.push(Form::from_json(&p.display().to_string(), &text)?);
            }
        }
        None => {
            for (name, sealed) in EMBEDDED {
                let path = format!("forms/{name}.json");
                forms.push(Form::from_json(&path, &crate::seal::open(&path, sealed)?)?);
            }
        }
    }
    for (i, f) in forms.iter().enumerate() {
        if forms[..i].iter().any(|g| g.name == f.name) {
            return Err(format!("form {} is defined twice", f.name));
        }
    }
    Ok(forms)
}

#[cfg(test)]
mod tests {
    use super::{EMBEDDED, Form, load};

    #[test]
    fn every_embedded_form_loads_and_is_named_after_its_file() {
        let forms = load(None).unwrap();
        assert_eq!(forms.len(), EMBEDDED.len());
        for (f, (file, _)) in forms.iter().zip(EMBEDDED) {
            assert_eq!(f.name, *file);
        }
    }

    #[test]
    fn a_card_is_as_wide_as_its_content_within_the_form_range() {
        let forms = load(None).unwrap();
        let form = |name: &str| forms.iter().find(|f| f.name == name).unwrap();
        let intent = |name: &str| {
            let path = format!("{}/tests/intents/{name}.json", env!("CARGO_MANIFEST_DIR"));
            tiles_intent::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
        };
        // Seven quarters: 35 + 0.5 x 7, rounded up.
        let big = form("big-number").fit(&intent("big-number"));
        assert_eq!((big.min, big.ideal), (35, 39));
        for name in ["column", "bar", "line", "bullet"] {
            let fit = form(name).fit(&intent(name));
            assert!(fit.ideal > big.ideal, "{name}: {fit:?}");
            assert!(fit.ideal <= form(name).width.max, "{name}: {fit:?}");
        }
    }

    #[test]
    fn a_form_width_out_of_order_is_refused() {
        let json = r#"{"name":"x","summary":"s","layout":"columns","width":{"min":50,"max":40},"roles":[
            {"name":"x","kind":"category","required":true,"doc":"d"},
            {"name":"y","kind":"number","required":true,"doc":"d"}]}"#;
        assert!(
            Form::from_json("x.json", json)
                .unwrap_err()
                .contains("width")
        );
    }

    #[test]
    fn a_form_missing_a_layout_role_is_refused() {
        let json = r#"{"name":"x","summary":"s","layout":"columns","roles":[
            {"name":"x","kind":"category","required":true,"doc":"d"}]}"#;
        assert!(
            Form::from_json("x.json", json)
                .unwrap_err()
                .contains("needs role y")
        );
    }

    #[test]
    fn a_form_with_an_unknown_key_or_role_is_refused() {
        let json = r#"{"name":"x","summary":"s","layout":"columns","roles":[],"extra":1}"#;
        assert!(
            Form::from_json("x.json", json)
                .unwrap_err()
                .contains("unknown field")
        );
        let json = r#"{"name":"x","summary":"s","layout":"columns","roles":[
            {"name":"x","kind":"category","required":true,"doc":"d"},
            {"name":"y","kind":"number","required":true,"doc":"d"},
            {"name":"z","kind":"number","required":false,"doc":"d"}]}"#;
        assert!(
            Form::from_json("x.json", json)
                .unwrap_err()
                .contains("no role z")
        );
    }
}
