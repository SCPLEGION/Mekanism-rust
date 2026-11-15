// The Meka-Tool sheared (hook point `native_shears_used`, API 59): the Shearing Unit's energy cost.
fn on_shears_used(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack) {
    crate::game::mekanism::shears_used(game, stack);
}
