// The wheel on a mod screen of Mekanism's (hook point `native_ui_wheel`, API 44): scrolls a list.
fn on_ui_wheel(game: &crate::game::Game<'_>, id: &str, steps: f32) {
    crate::game::mekanism::ui_wheel(game, id, steps);
}
