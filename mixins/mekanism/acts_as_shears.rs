// A mod's item that works as shears (hook point `native_acts_as_shears`, API 58): the Meka-Tool with a Shearing Unit.
fn on_acts_as_shears(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, shears: &mut bool) {
    crate::game::mekanism::acts_as_shears(game, stack, shears);
}
