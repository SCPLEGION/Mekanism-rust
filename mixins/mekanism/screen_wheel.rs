// The mouse wheel on a Mekanism screen (hook point `native_screen_wheel`, API 40): scrolls a filter list.
fn on_screen_wheel(game: &crate::game::Game<'_>, block: &str, steps: f32, taken: &mut bool) {
    crate::game::mekanism::screen_wheel(game, block, steps, taken);
}
