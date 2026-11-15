// The bars a stack shows over its slot (hook point `native_item_bars`, API 65): the energy and chemical bars.
fn on_item_bars(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    bar: &mut Option<(i32, u32)>,
    extra: &mut Vec<(i32, i32, u32)>,
) {
    crate::game::mekanism::item_bars(game, stack, bar, extra);
}
