// A player right-clicks with an item (hook point `native_use_item`, API 13): sneaking with the Atomic
// Disassembler cycles its mode.
fn on_use_item(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, claimed: &mut bool) {
    crate::game::mekanism::use_item(game, stack, claimed);
}
