// A mod packet from the server (hook point `native_client_payload`): the Robits' ids and state.
fn on_client_payload(channel: &str, data: &[u8]) {
    crate::game::mekanism::client_payload(channel, data);
}
