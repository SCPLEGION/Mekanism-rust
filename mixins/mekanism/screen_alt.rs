// Whether a Mekanism screen shows its second layout (hook point `native_screen_alt`, API 36): a QIO
// block's frequency selector.
fn on_screen_alt(game: &crate::game::Game<'_>, block: &str, alt: &mut bool) {
    crate::game::mekanism::screen_alt(game, block, alt);
}
