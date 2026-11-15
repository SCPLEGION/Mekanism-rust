// A mod bow is let go with an arrow (hook point `native_bow_shot`, API 66): the Electric Bow pays for the shot and burns in fire mode.
fn on_bow_shot(game: &mut crate::game::Game<'_>, bow: &horizon_client::items::ItemStack, flame: &mut bool, allow: &mut bool) {
    crate::game::mekanism::bow_shot(game, bow, flame, allow);
}
