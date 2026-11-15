// A button pressed on a Mekanism screen, on the server (hook point `native_menu_button`, API 20).
fn on_menu_button(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
    id: i32,
) {
    crate::game::mekanism::menu_button(game, pos, entity, id);
}
