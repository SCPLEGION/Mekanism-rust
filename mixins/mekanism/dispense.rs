// A dispenser dispensing an item (hook point `native_dispense`, API 83): a balloon latches to what stands in front.
fn on_dispense(game: &mut crate::game::Game<'_>, pos: [i32; 3], facing: [i32; 3], stack: &mut horizon_client::items::ItemStack, handled: &mut bool) {
    crate::game::mekanism::dispense(game, pos, facing, stack, handled);
}
