// A player breaks a block (hook point `native_destroy_block`, API 14): an Atomic Disassembler spends
// its energy on it, and an energy cube drops itself with the energy it held, a bin
// with its items, personal storage with its contents (and only for its owner).
fn on_destroy_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    if !*canceled && crate::game::mekanism::protect_personal(game, pos) {
        *canceled = true;
    }
    if !*canceled {
        crate::game::mekanism::spend_disassembler_energy(game, pos);
    }
    if !*canceled && crate::game::mekanism::break_keeping_energy(game, pos) {
        *canceled = true;
    }
}
