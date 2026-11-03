// A mouse button on a mod screen of Mekanism's (hook point `native_ui_click`, API 44).
fn on_ui_click(game: &mut crate::game::Game<'_>, id: &str, at: [f32; 2], button: u8, pressed: bool) {
    crate::game::mekanism::ui_click(game, id, at, button, pressed);
}
