// The tooltips of a Mekanism machine's screen (hook point `native_screen_tooltip`, API 20): energy,
// progress, usage and upgrades.
fn on_screen_tooltip(game: &crate::game::Game<'_>, block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) {
    crate::game::mekanism::screen_tooltip(game, block, data, at, lines);
}
