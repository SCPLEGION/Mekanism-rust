//! The `gui` area of the mod (see lib.rs). Hook bodies live here; lib.rs only delegates.
//!
//! `gfx.rs` keeps the old `game.native_*` drawing names over the upstream `DrawContext`, and the
//! screens extracted from the fork's runtime sit beside it, so their bodies stay close to the
//! original's, which is what "a faithful port" means here.
mod gfx;
mod machines;

/// State of this area, held in `Mod`.
#[derive(Default)]
pub(crate) struct State {}
