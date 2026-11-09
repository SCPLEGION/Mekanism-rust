// A player right-clicks a block with an item (hook point `native_use_item_on`, API 13): the Meka-Tool's Farming Unit
// strips, flattens and tills an area.
fn on_use_item_on(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, pos: [i32; 3], face: [i32; 3], claimed: &mut bool) {
    crate::game::mekanism::cardboard_use(game, stack, pos, claimed);
    crate::game::mekanism::dictionary_use(game, stack, pos, claimed);
    crate::game::mekanism::farming_use(game, stack, pos, face, claimed);
}
