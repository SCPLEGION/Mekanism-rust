// Wheel steps while playing (hook point `native_wheel`): the mode key (M) and the wheel change a scuba tank's flow.
fn on_wheel(game: &mut crate::game::Game<'_>, steps: f32, claimed: &mut bool) {
    crate::game::mekanism::wheel(game, steps, claimed);
}
