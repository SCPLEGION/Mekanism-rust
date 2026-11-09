// Flight in survival (hook point `native_may_fly`, API 60): the MekaSuit's Gravitational Modulating Unit.
fn on_may_fly(game: &crate::game::Game<'_>, may_fly: &mut bool) {
    crate::game::mekanism::may_fly(game, may_fly);
}
