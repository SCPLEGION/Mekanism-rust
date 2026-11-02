// Mekanism's windows over a machine's screen, its side configuration (hook point
// `native_screen_overlay`, API 32).
fn on_screen_overlay(
    game: &crate::game::Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut horizon_client::hud::HudBatch,
    origin: [f32; 2],
) {
    crate::game::mekanism::screen_overlay(game, block, data, batch, origin);
}
