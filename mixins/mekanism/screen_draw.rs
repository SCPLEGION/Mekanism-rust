// A Mekanism machine's screen, drawn with Mekanism's GUI sprites (hook point `native_screen_draw`,
// API 19): the panel, slots, power bar, progress bar and energy tab.
fn on_screen_draw(
    game: &crate::game::Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut horizon_client::hud::HudBatch,
    origin: [f32; 2],
) {
    crate::game::mekanism::screen_draw(game, block, data, batch, origin);
}
