// A mod packet from a player, with that player swapped in (hook point `native_server_payload`, API 26):
// a jetpack thrust spends its wearer's hydrogen; a frequency selector sets a frequency.
fn on_server_payload(game: &mut crate::game::Game<'_>, channel: &str, data: &[u8]) {
    crate::game::mekanism::server_payload(game, channel, data);
}
