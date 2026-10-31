// The item slots and menu of a Mekanism block (hook point `native_block_entity_container`, API 17).
fn on_block_entity_container(block: &str, container: &mut Option<horizon_client::containers::ModContainer>) {
    crate::game::mekanism::block_entity_container(block, container);
}
