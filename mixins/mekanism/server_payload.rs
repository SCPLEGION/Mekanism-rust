// A mod packet from a player, with that player swapped in (hook point `native_server_payload`, API 26):
// a jetpack thrust spends its wearer's hydrogen.
fn on_server_payload(game: &mut crate::game::Game<'_>, channel: &str) {
    crate::game::mekanism::server_payload(game, channel);
}
