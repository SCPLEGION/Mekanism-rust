// The Meka-Tool sheared a block (hook point `native_shears_trim`, API 71): the Shearing Unit's `shearTrim` energy cost.
fn on_shears_trim(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack) {
    crate::game::mekanism::shears_trim(game, stack);
}
