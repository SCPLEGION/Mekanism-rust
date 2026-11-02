// A player's right click on a block, before the block reacts (hook point `native_right_click_block`,
// API 21): the Configurator sets a face's mode, an upgrade in hand goes into a Mekanism machine, a
// bucket fills or empties a fluid tank, a bin takes or gives items, personal storage stays shut for
// others, QIO blocks take a frequency or a filter, lasers show their energy, a plenisher resets, a sorter takes filters, and the
// Modification Station installs modules.
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
        || crate::game::mekanism::use_boiler(game, pos)
        || crate::game::mekanism::use_sps_port(game, pos)
        || crate::game::mekanism::use_dynamic_valve(game, pos)
        || crate::game::mekanism::use_teleporter(game, pos)
        || crate::game::mekanism::use_bin(game, pos)
        || crate::game::mekanism::use_personal_storage(game, pos)
        || crate::game::mekanism::use_qio(game, pos)
        || crate::game::mekanism::use_laser(game, pos)
        || crate::game::mekanism::use_plenisher(game, pos)
        || crate::game::mekanism::use_sorter(game, pos)
        || crate::game::mekanism::use_modification_station(game, pos)
    {
        *canceled = true;
    }
}
