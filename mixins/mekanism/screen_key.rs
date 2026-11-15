// A key on a Mekanism screen's focused text box (hook point `native_screen_key`, API 33).
fn on_screen_key(game: &mut crate::game::Game<'_>, block: &str, key: &str, text: Option<&str>) {
    crate::game::mekanism::screen_key(game, block, key, text);
}
