//! Mekanism on Horizon's own mod API. All logic is `mekanism-core`; this crate is the glue that
//! `mixins/mekanism/runtime.rs` was on the fork (map: docs/horizon-api-migration.md).
//!
//! Three areas, one file each, so parallel work does not collide:
//! - `world.rs`: block entities, machine ticks, transmitters, capabilities, payloads, worldgen, mobs;
//! - `gui.rs`: screens, menus, widgets, JEI ghost targets;
//! - `gear.rs`: Meka-Tool, MekaSuit, modules, jetpack, weapons, tooltips, keys, HUD, commands.
//!
//! A hook is implemented once, here, as a one-line call into the area file that owns it
//! (`world::server_tick(self, host, event)`). To add a hook: add the method below and the function in
//! your area file; never put logic in this file.
use mod_api::{self, Hooks, PlayerLoggedInEvent, Registrar, RegistrationContext, ServerHost, Text};

mod gear;
mod gui;
mod world;

/// State shared by the three areas. Add a field per feature, behind its own `RefCell`/`Mutex` if it
/// mutates (hooks take `&self`); keep the area that owns it in a comment.
#[derive(Default)]
pub(crate) struct Mod {
    pub(crate) world: world::State,
    pub(crate) gui: gui::State,
    pub(crate) gear: gear::State,
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(player) = host.player(event.player) {
            let _ = player.tell(host, Text::literal(format!("Mekanism {} loaded", env!("CARGO_PKG_VERSION"))));
        }
    }

    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        gear::register(r);
    }
}
