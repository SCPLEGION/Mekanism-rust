// A player placed a block (hook point `native_place_block`, API 14): a cube item's energy goes back in.
fn on_place_block(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, pos: [i32; 3]) {
    crate::game::mekanism::placed(game, stack, pos);
}
