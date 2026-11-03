// Every tick a mod item is in continuous use (hook point `native_using_tick`, API 50): the Flamethrower's flames.
fn on_using_tick(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, claimed: &mut bool, keep: &mut bool) {
    crate::game::mekanism::using_tick(game, stack, claimed, keep);
}
