// A player's melee hit (hook point `native_player_attack`, API 27): the Meka-Tool's Attack
// Amplification Unit adds its damage and spends the energy for it.
fn on_player_attack(game: &mut crate::game::Game<'_>, strength: f32, amount: &mut f32) {
    crate::game::mekanism::player_attack(game, strength, amount);
}
