// A Mekanism block gets its block entity (hook point `native_block_entity`, API 15).
fn on_block_entity(block: &str, entity: &mut Option<(String, horizon_world::nbt::Compound)>) {
    crate::game::mekanism::block_entity(block, entity);
}
