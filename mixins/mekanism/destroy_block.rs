// A player breaks a block (hook point `native_destroy_block`, API 14): an energy cube drops itself with
// the energy it held.
fn on_destroy_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    if !*canceled && crate::game::mekanism::break_keeping_energy(game, pos) {
        *canceled = true;
    }
}
