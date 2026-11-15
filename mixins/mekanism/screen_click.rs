// A press on a Mekanism screen (hook point `native_screen_click`, API 20): the reactors' buttons, the
// redstone control tab and the frequency selector.
fn on_screen_click(game: &crate::game::Game<'_>, block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) {
    crate::game::mekanism::screen_click(game, block, data, at, right, button);
}
