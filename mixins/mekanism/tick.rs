// Every tick of every `Game` (hook point `native_tick`): the window's player thrusts with a jetpack.
fn on_tick(game: &mut crate::game::Game<'_>) {
    crate::game::mekanism::client_tick(game);
}
