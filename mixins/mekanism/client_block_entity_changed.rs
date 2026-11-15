// A block entity's client view reached this client (hook point `native_client_block_entity_changed`, API 23):
// a wind generator is one to draw the rotor of.
fn on_client_block_entity_changed(pos: [i32; 3], id: &str) {
    crate::game::mekanism::rotor_seen(pos, id);
}
