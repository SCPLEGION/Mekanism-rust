// The world's lifecycle (hook point `native_world_lifecycle`, API 3): server side, once per phase.
fn on_world_lifecycle(phase: crate::game::native_lifecycle::WorldPhase) {
    crate::game::mekanism::world_phase(phase);
}
