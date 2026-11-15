// Gear that glides (hook point `native_glider`, API 54): the MekaSuit's Elytra Unit.
fn on_glider(game: &mut crate::game::Game<'_>, glider: &mut bool) {
    crate::game::mekanism::glider(game, glider);
}
