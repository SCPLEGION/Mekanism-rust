// A mod item in the main hand adds to the attack speed (hook point `native_attack_speed`, API 70): the Atomic Disassembler's and
// the Meka-Tool's `attackSpeed` from their configs.
fn on_attack_speed(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, add: &mut f64) {
    crate::game::mekanism::attack_speed(game, stack, add);
}
