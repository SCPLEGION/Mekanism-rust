//! Client-server channels. On the fork every payload came to one `server_payload` / `client_payload` hook with the
//! channel name; here each channel is registered once (both directions, main-thread handlers) and a router calls
//! the function its feature declared. The channel names of `runtime.rs` stay the names features use: a
//! `mekanismadditions:` one travels as `mekanism:additions_<name>`, since the mod owns one namespace for them.
use mod_api::{
    ClientHost, IPayloadContext, IPayloadHandler, PayloadBytes, PayloadHandleError, PayloadId, PlayerRef, Registrar,
    ServerHost,
};
use std::sync::{Arc, Mutex};

/// A server-side receiver: the sending player and the bytes.
pub(crate) type ServerFn = fn(&mut dyn ServerHost, PlayerRef, &[u8]);
/// A client-side receiver.
pub(crate) type ClientFn = fn(&mut dyn ClientHost, &[u8]);

struct Channel {
    wire: String,
    server: Option<ServerFn>,
    client: Option<ClientFn>,
}

static CHANNELS: Mutex<Vec<Channel>> = Mutex::new(Vec::new());

/// The id a channel travels under.
pub(crate) fn wire_id(legacy: &str) -> PayloadId {
    PayloadId(match legacy.strip_prefix("mekanismadditions:") {
        Some(name) => format!("mekanism:additions_{name}"),
        None => legacy.to_owned(),
    })
}

/// Declares channel `legacy` (`mekanism:frequency`) with its receivers, during `Hooks::register`.
pub(crate) fn channel(r: &mut dyn Registrar, legacy: &str, server: Option<ServerFn>, client: Option<ClientFn>) {
    let id = wire_id(legacy);
    CHANNELS.lock().unwrap().push(Channel { wire: id.0.clone(), server, client });
    let router = Arc::new(Router { wire: id.0.clone() });
    r.payload_handlers().registrar("1").play_bidirectional(id, router.clone(), Some(router));
}

struct Router {
    wire: String,
}

impl IPayloadHandler for Router {
    fn handle(&self, payload: PayloadBytes, context: &mut dyn IPayloadContext) -> Result<(), PayloadHandleError> {
        let failed = |e: mod_api::PayloadContextError| PayloadHandleError::Failed(format!("{e:?}"));
        let table = CHANNELS.lock().unwrap();
        let Some(channel) = table.iter().find(|c| c.wire == self.wire) else {
            return Ok(());
        };
        match context.flow() {
            mod_api::PayloadDirection::ToServer => {
                let (Some(receive), player) = (channel.server, context.player().map_err(failed)?) else {
                    return Ok(());
                };
                receive(context.server_host().map_err(failed)?, player, &payload.0);
            }
            _ => {
                if let Some(receive) = channel.client {
                    receive(context.client_host().map_err(failed)?, &payload.0);
                }
            }
        }
        Ok(())
    }
}

/// Sends `bytes` from this client to the server on `channel`; false when the connection refused it.
pub(crate) fn to_server(host: &mut dyn ClientHost, channel: &str, bytes: &[u8]) -> bool {
    host.send_payload_to_server(&wire_id(channel), PayloadBytes(bytes.to_vec())).is_ok()
}

/// Sends `bytes` from the server to `player` on `channel`; false when the connection refused it.
pub(crate) fn to_player(host: &mut dyn ServerHost, player: PlayerRef, channel: &str, bytes: &[u8]) -> bool {
    player.send_payload(host, &wire_id(channel), PayloadBytes(bytes.to_vec())).is_ok()
}
