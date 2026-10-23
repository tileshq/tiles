//! Everything Tiles writes to a terminal (ADR-004, ADR-009).
//!
//! [`detect`] names the terminal and picks the image route; [`kitty`] and
//! [`sixel`] are the one graphics encoder, writing only their own headers and
//! base64 or sixel payloads; [`sanitize`] is the one text path; [`block`] is
//! the placeholder block a tool result carries and the wrapper finds again;
//! [`fallback`] is the text a terminal without images shows. Each terminal
//! rule lives here once, never in adapter code.

pub mod b64;
pub mod block;
pub mod detect;
mod diacritics;
pub mod fallback;
pub mod image;
pub mod kitty;
pub mod sanitize;
pub mod sixel;
