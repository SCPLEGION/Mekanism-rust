// Every tick after the mobs ticked (hook point `native_mobs_tick`, API 46): the Robits follow and collect.
fn on_mobs_tick(game: &mut crate::game::Game<'_>) {
    crate::game::mekanism::mobs_tick(game);
}
