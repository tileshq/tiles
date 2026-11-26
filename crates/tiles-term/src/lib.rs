//! Everything Tiles writes to a terminal (ADR-004, ADR-009).
//!
//! [`detect`] names the terminal and picks the image route; [`kitty`] and
//! [`sixel`] are the one graphics encoder, writing only their own headers and
//! base64 or sixel payloads; [`sanitize`] is the one text path; [`block`] is
//! the placeholder block a tool result carries and the wrapper finds again;
//! [`fallback`] is the text a terminal without images shows; [`style`] is the
//! step list `tiles setup` prints; [`pane`] and [`input`] are what the
//! `tiles pane` prototype writes and reads (ADR-063). Each terminal rule
//! lives here once, never in adapter code.

pub mod b64;
pub mod block;
pub mod detect;
mod diacritics;
pub mod fallback;
pub mod image;
pub mod input;
pub mod kitty;
pub mod pane;
pub mod sanitize;
pub mod sixel;
pub mod style;
