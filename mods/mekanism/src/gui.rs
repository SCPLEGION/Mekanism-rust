//! The `gui` area of the mod (see lib.rs). Hook bodies live here; lib.rs only delegates.
//!
//! `gfx.rs` keeps the old `game.native_*` drawing names over the upstream `DrawContext`, and the
//! screens extracted from the fork's runtime sit beside it, so their bodies stay close to the
//! original's, which is what "a faithful port" means here.
use mod_api::{
    BlockEntityPart, BlockPart, ClientHost, Host, Registrar, Screen, ScreenClickEvent, ScreenDrawEvent, ScreenEvent,
    ScreenTooltipEvent,
};

mod gfx;
mod machines;

use gfx::Gfx;

/// State of this area, held in `Mod`.
#[derive(Default)]
pub(crate) struct State {}

/// Attaches the screens to the block entity types the world area declares (its ids, `mekanism:<type>`).
pub(crate) fn register(r: &mut dyn Registrar) {
    let machine = r.block_entity_type_key(&format!("mekanism:{}", crate::world::machine::TYPE));
    r.block_entity_part(&machine, ScreenPart).expect("native block entity type");
}

/// The screen of a Mekanism block, as the fork's `screen`, `screen_draw`, `screen_click` and `screen_tooltip` hooks gave it.
struct ScreenPart;

#[mod_api::horizon_part]
impl BlockPart for ScreenPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for ScreenPart {
    fn screen(&self, _: &dyn Host, event: &mut ScreenEvent<'_>) -> Option<Screen> {
        machines::screen::layout_of(event.block)
    }

    fn screen_draw(&self, host: &dyn ClientHost, event: &mut ScreenDrawEvent<'_>) {
        let (block, data, origin) = (event.block, event.data, event.origin);
        let gfx = Gfx::drawing(host, &mut *event.draw);
        machines::screen::draw(&gfx, block, data, origin);
    }

    fn screen_click(&self, _: &dyn ClientHost, event: &mut ScreenClickEvent<'_>) -> Option<i32> {
        machines::screen::click(event.block, event.data, event.at, event.right)
    }

    fn screen_tooltip(&self, _: &dyn ClientHost, event: &mut ScreenTooltipEvent<'_>) {
        machines::screen::tooltip(event.block, event.data, event.at, event.lines);
    }
}
