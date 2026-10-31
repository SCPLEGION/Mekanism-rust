// A player's right click on a block, before the block reacts (hook point `native_right_click_block`,
// API 21): an upgrade in hand goes into a Mekanism machine.
fn on_right_click_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    if !*canceled && crate::game::mekanism::install_upgrade(game, pos) {
        *canceled = true;
    }
}
