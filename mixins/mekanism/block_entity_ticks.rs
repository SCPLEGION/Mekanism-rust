// Which Mekanism block entities tick (hook point `native_block_entity_ticks`, API 15).
fn on_block_entity_ticks(block: &str, ticks: &mut bool) {
    crate::game::mekanism::block_entity_ticks(block, ticks);
}
