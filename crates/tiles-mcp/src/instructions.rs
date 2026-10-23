//! The session instruction that makes the model choose the tile.
//!
//! Harnesses rank their own tools' instructions (Claude Code's Artifact tool,
//! for one) above an MCP tool's description, so a model asked for a chart
//! builds a web page unless the session itself says to use Tiles. In the
//! prototype, Claude, Grok and Codex each picked the tile 0 of 2 times without
//! this text and nearly always with it. Every harness gets the same words
//! through its own launch option: the MCP `instructions` field, Claude Code's
//! `--append-system-prompt`, Codex's `developer_instructions`, `OpenCode`'s
//! `instructions` file and Pi's system prompt section.

use crate::TOOL_NAME;
use crate::source::Reach;

/// How the card reaches the user, which changes what the model must do with
/// the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Draw {
    /// An add-on draws the saved card beside the tool result (Pi's
    /// extension, the Claude Code mod); the result itself carries no image.
    Image,
    /// `tiles run` draws over the placeholder block the model copies into
    /// its reply.
    Block,
    /// The terminal shows no pictures: the user gets the summary and the
    /// saved PNG's path.
    Text,
}

impl Draw {
    /// The value of `TILES_DRAW`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Draw::Image => "image",
            Draw::Block => "block",
            Draw::Text => "text",
        }
    }

    /// The mode `TILES_DRAW` names; `image` when unset or unknown.
    #[must_use]
    pub fn from_name(name: Option<&str>) -> Draw {
        match name {
            Some("block") => Draw::Block,
            Some("text") => Draw::Text,
            _ => Draw::Image,
        }
    }
}

/// The instruction text, with a sentence for Omni and each configured
/// source in `reach`. Under [`Draw::Text`] with `companion`, where `tiles
/// run`'s companion window shows the cards ([`crate::companion_from_env`],
/// ADR-046), it says the chart shows there and asks for no PNG path.
#[must_use]
pub fn session(draw: Draw, companion: bool, reach: &Reach) -> String {
    let mut text = format!(
        "This terminal session runs with Tiles, which draws chart cards inline in the user's \
         terminal. When the user asks for a chart, graph, plot or other data visual, answer with \
         the {TOOL_NAME} tool. Send its keys (v, form, title, claim, fields, data) at the top \
         level of the call, never nested under another key; pick the form that fits the finding, name \
         the column for each role the form declares, write the finding itself as the title, set \
         the claim's kind and tone, and highlight only the values the title names. Pass small, \
         already aggregated rows inline under data.rows, in full units (128000, not 128 for a \
         title's $128K). When the user asks for several metrics at once, such as KPI scorecards, \
         a dashboard or an overview, or for cards side by side, draw them all with one \
         {TOOL_NAME} call as a dashboard: the first card's keys at the top level, up to 3 more cards beside it under \
         row, further rows under rows (scorecards together in one row, a wider chart such as a \
         line over months in its own row), and dashboard with the shared title and, when there \
         is one, a note (data source, date range). Give every scorecard the metric's \
         recent periods (12 months for a year), not one value, so it shows its change and \
         trend. Title each card with a short metric label of a few words, such as Monthly \
         revenue, and give it no subtitle; the dashboard title \
         and note are written once, so do not repeat them on every card. Do not build an \
         Artifact, web dashboard, web page, HTML or image file, or an ASCII chart for it, \
         unless the user asks for a link, a shareable page or a file. If the tool refuses the call, fix the key it names and call \
         it again."
    );
    if reach.omni {
        text.push_str(
            " When the data lives in Omni, pass data.omni with a topic and its fields instead of \
             rows; find them with the tiles_omni_topics and tiles_omni_fields tools, and name the \
             same fully qualified fields under fields. Tiles fetches the rows itself, so do not \
             query them first.",
        );
    }
    for offer in &reach.offers {
        text.push(' ');
        text.push_str(offer.instruction.trim());
    }
    match draw {
        Draw::Block => text.push_str(
            " The tool returns a block of [tile:] lines inside a ```tiles code fence: put the \
             fenced block in your reply exactly as written, fence lines included. The chart \
             shows in place of the block, so do not tell the user it might not appear.",
        ),
        Draw::Text if companion => text.push_str(
            " This terminal shows no pictures, but the Tiles window beside it shows each chart \
             the tool draws: tell the user in two or three lines what it shows and that it is in \
             that window, and do not ask them to open anything. The PNG path in the result is for \
             reference.",
        ),
        Draw::Text => text.push_str(
            " This terminal shows no pictures: after the tool draws the chart, tell the user in \
             two or three lines what it shows and give the PNG path from the result, so they can \
             open or share it.",
        ),
        Draw::Image => text.push_str(
            " The chart shows in the tool result, so do not tell the user it might not appear \
             and do not repeat its numbers.",
        ),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{Draw, session};
    use crate::source::{Offer, Reach};

    #[test]
    fn the_block_instruction_asks_for_the_block() {
        let t = session(Draw::Block, false, &Reach::default());
        assert!(t.contains("tiles_chart") && t.contains("block of [tile:] lines"));
        // OpenCode draws a bare [tile:] line as a link without its brackets,
        // so the block must reach every harness inside a fence (ADR-052).
        assert!(
            t.contains("inside a ```tiles code fence") && t.contains("fence lines included"),
            "{t}"
        );
        assert!(
            !t.contains("plain lines") && !t.contains("never inside"),
            "{t}"
        );
        assert!(t.contains("several metrics at once"), "{t}");
        assert!(t.contains("further rows under rows"), "{t}");
        assert!(t.contains("12 months for a year"), "{t}");
        assert!(t.contains("short metric label"), "{t}");
        assert!(!t.contains("data.omni"));
    }

    #[test]
    fn the_text_route_names_the_companion_window_only_when_it_is_on() {
        let off = session(Draw::Text, false, &Reach::default());
        assert!(
            off.ends_with("give the PNG path from the result, so they can open or share it."),
            "{off}"
        );
        assert!(!off.contains("Tiles window"), "{off}");
        let on = session(Draw::Text, true, &Reach::default());
        assert!(
            on.ends_with(
                " This terminal shows no pictures, but the Tiles window beside it shows each \
                 chart the tool draws: tell the user in two or three lines what it shows and \
                 that it is in that window, and do not ask them to open anything. The PNG path \
                 in the result is for reference."
            ),
            "{on}"
        );
        assert!(!on.contains("so they can open"), "{on}");
        for draw in [Draw::Image, Draw::Block] {
            assert_eq!(
                session(draw, true, &Reach::default()),
                session(draw, false, &Reach::default()),
                "{draw:?} has no companion window"
            );
        }
    }

    #[test]
    fn omni_is_mentioned_only_when_configured() {
        assert!(session(Draw::Image, false, &Reach::omni(true)).contains("tiles_omni_topics"));
        assert!(!session(Draw::Image, false, &Reach::omni(true)).contains("```tiles"));
    }

    #[test]
    fn a_configured_source_adds_its_own_sentences() {
        let reach = Reach {
            omni: false,
            offers: vec![Offer {
                name: "warehouse",
                origin: "BigQuery",
                tools: Vec::new(),
                instruction: " Pass data.warehouse for warehouse rows. ".into(),
                chart_line: String::new(),
                tool_timeout: std::time::Duration::from_secs(300),
            }],
        };
        let t = session(Draw::Image, false, &reach);
        assert!(
            t.contains("call it again. Pass data.warehouse for warehouse rows. The chart"),
            "{t}"
        );
        assert!(!t.contains("data.omni"));
        assert!(!session(Draw::Image, false, &Reach::default()).contains("data.warehouse"));
    }
}
