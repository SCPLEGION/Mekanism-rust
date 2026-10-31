// The held item's mining speed (hook point `native_destroy_speed`, API 25): the Atomic Disassembler is
// fast while charged.
fn on_destroy_speed(
    game: &crate::game::Game<'_>,
    stack: Option<&horizon_client::items::ItemStack>,
    hardness: f32,
    speed: &mut f32,
) {
    crate::game::mekanism::destroy_speed(game, stack, hardness, speed);
}
