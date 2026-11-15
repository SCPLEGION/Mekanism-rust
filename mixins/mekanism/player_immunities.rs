// Damage the player takes none of (hook point `native_player_immunities`, API 52): the Gas Mask.
fn on_player_immunities(game: &mut crate::game::Game<'_>, immune: &mut Vec<String>) {
    crate::game::mekanism::player_immunities(game, immune);
}
