// The HUD (hook point `native_hud`): the MekaSuit's energy readout (`HUDRenderer.renderMekaSuitEnergyIcons`).
fn on_hud(game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    crate::game::mekanism::hud(game, batch);
}
