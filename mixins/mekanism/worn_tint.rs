// The tint of the opaque models a piece of gear wears (hook point `native_worn_tint`, API 57): the Color Modulation Unit.
fn on_worn_tint(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, tint: &mut Option<u32>) {
    crate::game::mekanism::worn_tint(game, stack, tint);
}
