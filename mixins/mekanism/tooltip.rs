// An item's tooltip (hook point `native_tooltip`, API 12): the energy a Mekanism item holds.
fn on_tooltip(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    lines: &mut Vec<(horizon_client::text::Text, u32)>,
) {
    crate::game::mekanism::tooltip(game, stack, lines);
}
