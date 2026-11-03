// A mod screen closed (hook point `native_ui_closed`, API 44): the Robit's inventory lets go of the cursor.
fn on_ui_closed(game: &mut crate::game::Game<'_>, id: &str) {
    crate::game::mekanism::ui_close_hook(game, id);
}
