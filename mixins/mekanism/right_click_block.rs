// A player's right click on a block, before the block reacts (hook point `native_right_click_block`,
// API 21): the Configurator sets a face's mode, an upgrade in hand goes into a Mekanism machine, and
// a bucket fills or empties a fluid tank.
fn on_right_click_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], face: [i32; 3], canceled: &mut bool) {
    if *canceled {
        return;
    }
    if crate::game::mekanism::configure_side(game, pos, face) || crate::game::mekanism::install_upgrade(game, pos)
        || crate::game::mekanism::fill_jetpack(game, pos)
        || crate::game::mekanism::use_fluid_tank(game, pos)
        || crate::game::mekanism::use_fission_port(game, pos)
        || crate::game::mekanism::show_turbine(game, pos)
        || crate::game::mekanism::use_fusion(game, pos)
    {
        *canceled = true;
    }
}
