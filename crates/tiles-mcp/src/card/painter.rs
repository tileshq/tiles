//! [`EnginePainter`], the engine behind [`Painter`].

use tiles_intent::schema::{FormEntry, RoleEntry};
use tiles_intent::{Intent, IntentError};

use super::{Face, Fit, Heading, Look, Painted, Painter, Refusal, Tile, whole, within};
use crate::board;

/// The engine behind [`Painter`].
#[derive(Debug)]
pub struct EnginePainter {
    engine: tiles_engine::Engine,
}

impl EnginePainter {
    /// Loads the engine's tokens, forms and fonts.
    ///
    /// # Errors
    ///
    /// When an embedded asset fails to load.
    pub fn new() -> Result<Self, String> {
        tiles_engine::Engine::new()
            .map(|engine| Self { engine })
            .map_err(|e| e.to_string())
    }

    /// Lays out one card, lined up with its row by `align`, and describes
    /// it.
    fn face(
        &self,
        tile: &Tile<'_>,
        look: &Look,
        align: tiles_engine::Align,
    ) -> Result<(tiles_engine::Scene, Face), Refusal> {
        let opts = tiles_engine::Options {
            width: look
                .width_css(tile.cols)
                .clamp(tiles_engine::MIN_WIDTH, tiles_engine::MAX_WIDTH),
            theme: look.theme,
            cell_height: Some(look.cell_height_css().clamp(1.0, 200.0)),
        };
        let intent = tile.intent;
        let notes = intent.notes.clone();
        let refusal = |e| match e {
            tiles_engine::RenderError::Intent(i) => Refusal::Intent(within(tile.key, i)),
            tiles_engine::RenderError::Engine(e) => Refusal::Internal(e),
        };
        let mut notes = notes;
        let scene = match self.engine.scene_aligned(intent, &opts, align) {
            // A note with no room beside its mark is left out, not refused.
            Err(tiles_engine::RenderError::Intent(e)) if e.key == "note" => {
                notes.push("The note found no room beside its mark, so Tiles left it out.".into());
                let bare = Intent {
                    note: None,
                    ..intent.clone()
                };
                self.engine
                    .scene_aligned(&bare, &opts, align)
                    .map_err(refusal)?
            }
            other => other.map_err(refusal)?,
        };
        let mut seen = std::collections::HashSet::new();
        let mut values = Vec::new();
        let hover = |i: &&tiles_engine::scene::Item| {
            matches!(
                i.part,
                tiles_engine::scene::Part::Mark | tiles_engine::scene::Part::Hit
            )
        };
        for item in scene.items.iter().filter(hover) {
            if let (Some(row), Some(tip)) = (item.datum, &item.tip)
                && seen.insert((row, tip.clone()))
            {
                values.push(tip.replace('\n', " · "));
            }
        }
        values.dedup();
        let face = Face {
            title: intent.title.clone(),
            form: scene.form.clone(),
            values,
            notes,
            fetched: None,
        };
        Ok((scene, face))
    }
}

impl EnginePainter {
    /// Lays out one row's cards so their parts line up across it: alone
    /// first, then each with the row's most title and subtitle lines, then
    /// each as tall as the tallest. A card among others (`among`) has no
    /// default subtitle, and a scorecard among them is compact.
    pub(super) fn row_faces(
        &self,
        row: &[Tile<'_>],
        look: &Look,
        among: bool,
    ) -> Result<Vec<(tiles_engine::Scene, Face)>, Refusal> {
        let mut align: Vec<tiles_engine::Align> = row
            .iter()
            .map(|t| tiles_engine::Align {
                compact: among
                    && self
                        .engine
                        .form(&t.intent.form)
                        .is_ok_and(tiles_engine::Form::is_scorecard),
                among,
                ..tiles_engine::Align::default()
            })
            .collect();
        let lay = |align: &[tiles_engine::Align]| {
            row.iter()
                .zip(align)
                .map(|(t, a)| self.face(t, look, *a))
                .collect::<Result<Vec<_>, Refusal>>()
        };
        let faces = lay(&align)?;
        if row.len() < 2 {
            return Ok(faces);
        }
        let lines = |part: tiles_engine::scene::Part| {
            faces
                .iter()
                .map(|(s, _)| s.items.iter().filter(|i| i.part == part).count())
                .max()
                .unwrap_or(0)
        };
        let (title, subtitle) = (
            lines(tiles_engine::scene::Part::Title),
            lines(tiles_engine::scene::Part::Subtitle),
        );
        for a in &mut align {
            a.title_lines = title;
            a.subtitle_lines = subtitle;
        }
        let faces = lay(&align)?;
        let tallest = faces.iter().map(|(s, _)| s.height).fold(0.0, f32::max);
        if faces.iter().all(|(s, _)| tallest - s.height < 0.5) {
            return Ok(faces);
        }
        for a in &mut align {
            a.min_height = tallest;
        }
        lay(&align)
    }
}

impl Painter for EnginePainter {
    fn forms(&self) -> Vec<FormEntry> {
        self.engine
            .forms()
            .iter()
            .map(|f| FormEntry {
                name: f.name.clone(),
                summary: f.summary.clone(),
                roles: f
                    .roles
                    .iter()
                    .map(|r| RoleEntry {
                        name: r.name.clone(),
                        required: r.required,
                        doc: r.doc.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    fn mend(&self, intent: &mut Intent) {
        self.engine.mend(intent);
    }

    fn scorecard(&self, intent: &Intent) -> bool {
        self.engine
            .form(&intent.form)
            .is_ok_and(tiles_engine::Form::is_scorecard)
    }

    fn fit(&self, intent: &Intent, among: bool) -> Result<Fit, IntentError> {
        self.engine.form(&intent.form).map(|f| {
            let fit = f.fit(intent);
            if among && f.is_scorecard() {
                Fit {
                    ideal: fit.min,
                    max: fit.min,
                    ..fit
                }
            } else {
                fit
            }
        })
    }

    fn paint(
        &self,
        rows: &[Vec<Tile<'_>>],
        heading: Option<&Heading>,
        look: &Look,
    ) -> Result<Painted, Refusal> {
        let cell = (look.width_css(1), look.cell_height_css());
        let theme = look.theme;
        let mut scenes = Vec::new();
        let mut sizes = Vec::new();
        let mut faces = Vec::new();
        let among = rows.iter().map(Vec::len).sum::<usize>() > 1;
        // The engine snaps a height to whole cells.
        let cell_rows = |height: f32| {
            let rows = whole(
                f64::from(height / cell.1 - 0.01).ceil(),
                u32::from(u16::MAX),
            );
            u16::try_from(rows).unwrap_or(u16::MAX)
        };
        if let Some(h) = heading {
            let cols = rows
                .iter()
                .map(|r| board::span(&r.iter().map(|t| t.cols).collect::<Vec<_>>()))
                .max()
                .unwrap_or(0);
            let opts = tiles_engine::Options {
                width: look
                    .width_css(cols)
                    .clamp(tiles_engine::MIN_WIDTH, tiles_engine::MAX_WIDTH),
                theme,
                cell_height: Some(look.cell_height_css().clamp(1.0, 200.0)),
            };
            let scene = self.engine.heading(&h.title, h.note.as_deref(), &opts);
            sizes.push(vec![(cols, cell_rows(scene.height))]);
            scenes.push(scene);
        }
        for row in rows {
            let mut size = Vec::new();
            for (tile, (scene, face)) in row.iter().zip(self.row_faces(row, look, among)?) {
                size.push((tile.cols, cell_rows(scene.height)));
                scenes.push(scene);
                faces.push(face);
            }
            sizes.push(size);
        }
        let (at, cells) = board::place(&sizes);
        let tiles: Vec<tiles_engine::Placed> = scenes
            .into_iter()
            .zip(at.into_iter().flatten())
            .map(|(scene, (col, row))| tiles_engine::Placed {
                scene,
                x: f32::from(col) * cell.0,
                y: f32::from(row) * cell.1,
            })
            .collect();
        let board = tiles_engine::Board {
            width: tiles
                .iter()
                .map(|t| t.x + t.scene.width)
                .fold(0.0, f32::max),
            height: tiles
                .iter()
                .map(|t| t.y + t.scene.height)
                .fold(0.0, f32::max),
            tiles,
        };
        let png = self
            .engine
            .board_png(&board, look.scale)
            .map_err(|e| Refusal::Internal(e.to_string()))?;
        // The entrance is drawn at half the card's resolution, a quarter of
        // its bytes, so it reaches the terminal fast; it plays for under a
        // second before the crisp card takes over. Its last frame is the
        // settled picture at that resolution.
        let frames = if look.frames >= 2 {
            let scale = (look.scale / 2.0).max(0.5);
            let mut frames = self
                .engine
                .board_entrance(&board, scale, look.entrance)
                .map_err(|e| Refusal::Internal(e.to_string()))?;
            frames.push(
                self.engine
                    .board_png(&board, scale)
                    .map_err(|e| Refusal::Internal(e.to_string()))?,
            );
            frames
        } else {
            Vec::new()
        };
        let frame_ms = whole(f64::from(self.engine.tokens().motion.frame_ms), 1000);
        Ok(Painted {
            png,
            width_px: whole(f64::from(board.width * look.scale), 1 << 16),
            height_px: whole(f64::from(board.height * look.scale), 1 << 16),
            cells,
            faces,
            frames,
            frame_ms,
        })
    }
}
