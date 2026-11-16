//! Mekanism on Horizon's own mod API. All logic is `mekanism-core`; this crate is the glue that
//! `mixins/mekanism/runtime.rs` was on the fork (map: docs/horizon-api-migration.md).
use mod_api::{self, Hooks, PlayerLoggedInEvent, ServerHost, Text};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// Stage 0 of the move: the mod loads, and `mekanism-core` is linked.
    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(player) = host.player(event.player) {
            let _ = player.tell(host, Text::literal(format!("Mekanism {} loaded", env!("CARGO_PKG_VERSION"))));
        }
    }
}
