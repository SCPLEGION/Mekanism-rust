// A hit on a player (hook point `native_player_damage`, API 28): the MekaSuit takes its share and
// spends the energy for it.
fn on_player_damage(game: &mut crate::game::Game<'_>, absorbed: f32, share: &mut f32) {
    crate::game::mekanism::player_damage(game, absorbed, share);
}
