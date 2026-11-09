// A key on a mod screen of Mekanism's (hook point `native_ui_key`, API 44).
fn on_ui_key(game: &mut crate::game::Game<'_>, id: &str, key: &str, text: Option<&str>, pressed: bool) {
    crate::game::mekanism::ui_key(game, id, key, text, pressed);
}
