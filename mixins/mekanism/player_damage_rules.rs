// Shares of damage the player's armor takes (hook point `native_player_damage_rules`, API 53): the MekaSuit's
// Inhalation Purification Unit against magic damage.
fn on_player_damage_rules(game: &mut crate::game::Game<'_>, absorbed: f32, rules: &mut Vec<(String, f32)>) {
    crate::game::mekanism::player_damage_rules(game, absorbed, rules);
}
