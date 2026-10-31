// A player's right click on a block, before the block reacts (hook point `native_right_click_block`,
// API 21): the Configurator sets a face's mode, and an upgrade in hand goes into a Mekanism machine.
fn on_right_click_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], face: [i32; 3], canceled: &mut bool) {
    if *canceled {
        return;
    }
    if crate::game::mekanism::configure_side(game, pos, face) || crate::game::mekanism::install_upgrade(game, pos) {
        *canceled = true;
    }
}
