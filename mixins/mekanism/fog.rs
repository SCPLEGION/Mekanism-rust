// The frame's fog (hook point `native_fog`, API 56): the Vision Enhancement Unit's green tint and far plane.
fn on_fog(game: &crate::game::Game<'_>, fog_type: u8, fog: &mut horizon_client::fog::FogData) {
    crate::game::mekanism::fog(game, fog_type, fog);
}
