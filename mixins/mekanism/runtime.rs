// Mekanism's glue inside Horizon's game crate (`crate::game::mekanism`). Game logic lives in the
// `mekanism_core` crate; this module only translates between the game's types and its own.

use super::native_lifecycle::WorldPhase;

pub(super) fn world_phase(phase: WorldPhase) {
    if phase == WorldPhase::WorldOpenReady {
        eprintln!("{}", mekanism_core::loaded_message("(server)"));
    }
}
