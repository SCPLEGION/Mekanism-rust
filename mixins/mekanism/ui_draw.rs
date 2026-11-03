// A mod screen of Mekanism's own is drawn (hook point `native_ui_draw`, API 44): the module tweaker.
fn on_ui_draw(game: &crate::game::Game<'_>, id: &str, batch: &mut horizon_client::hud::HudBatch) {
    crate::game::mekanism::ui_draw(game, id, batch);
}
