// A block beside another changed (hook point `native_neighbor_changed`, API 6): a transmitter that was told
// looks at its sides again at once (`TileEntityTransmitter.onNeighborBlockChange`), instead of at its next check.
fn on_neighbor_changed(level: &crate::game::GameLevel<'_>, pos: [i32; 3], state: horizon_world::registry::BlockStateId) {
    crate::game::mekanism::transmitter_told(pos, &level.facts.get(state).name);
}
