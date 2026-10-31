// A server tick of a Mekanism block entity (hook point `native_block_entity_tick`, API 18 helpers).
fn on_block_entity_tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
) {
    crate::game::mekanism::block_entity_tick(game, pos, entity);
}
