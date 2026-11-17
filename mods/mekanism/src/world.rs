//! The `world` area of the mod (see lib.rs): block entities, machine ticks, transmitters, capabilities,
//! payloads, worldgen and mobs. Hook bodies live here; lib.rs only delegates.
//!
//! The fork's one `block_entity*` hook family keyed on `kind(block)` becomes one block entity type per family
//! of blocks, each with a [`BlockEntityPart`] (docs/horizon-api-migration.md). A block entity keeps its state
//! in its saved compound exactly as on the fork, so worlds and the logic in `mekanism-core` carry over.
use mod_api::{Host, Registrar};

pub(crate) mod cable;
pub(crate) mod common;
pub(crate) mod conductor;
pub(crate) mod bin;
pub(crate) mod chem;
pub(crate) mod generators;
pub(crate) mod fission;
pub(crate) mod multiblock;
pub(crate) mod factory;
pub(crate) mod fluid_chem;
pub(crate) mod item_chem;
pub(crate) mod machine;
pub(crate) mod nbt;
pub(crate) mod net;
pub(crate) mod transmit;

/// State of this area, held in `Mod`. Block entities keep their state in their own compound, so there is none yet.
#[derive(Default)]
pub(crate) struct State {}

/// Declares every block entity type, container and part of the area.
pub(crate) fn register(r: &mut dyn Registrar) {
    machine::register(r);
    cable::register(r);
    conductor::register(r);
    generators::register(r);
    fission::register(r);
    factory::register(r);
    bin::register(r);
    fluid_chem::register(r);
    item_chem::register(r);
}

/// Item ids by key and back. The game's registry is fixed once a world is open, so the table is built once:
/// the Metadata-level `item_key_id` is not on `Host` (only registry entries and `item_key` are).
pub(crate) struct ItemIndex {
    ids: std::collections::HashMap<mod_api::ItemKey, String>,
}

static ITEMS: std::sync::OnceLock<ItemIndex> = std::sync::OnceLock::new();

pub(crate) fn items(host: &dyn Host) -> &'static ItemIndex {
    ITEMS.get_or_init(|| {
        let mut ids = std::collections::HashMap::new();
        for id in host.registry_entries("minecraft:item").unwrap_or_default() {
            if let Some(key) = host.item_key(&id) {
                ids.insert(key, id.to_string());
            }
        }
        ItemIndex { ids }
    })
}

impl ItemIndex {
    /// `minecraft:iron_ingot`, or an empty string for a key it does not know.
    pub(crate) fn id(&self, key: mod_api::ItemKey) -> String {
        self.ids.get(&key).cloned().unwrap_or_default()
    }
}

/// An item's id (`minecraft:iron_ingot`).
pub(crate) fn item_id(host: &dyn Host, stack: &mod_api::ItemStack) -> String {
    items(host).id(stack.item())
}

/// The key of the item `id`.
pub(crate) fn item_key(host: &dyn Host, id: &str) -> Option<mod_api::ItemKey> {
    host.item_key(&mod_api::Identifier::parse(id).ok()?)
}

/// Whether the item `id` is in the item tag `tag` (`c:ingots/iron`, no `#`).
pub(crate) fn in_tag(host: &dyn Host, id: &str, tag: &str) -> bool {
    host.registry_entry("minecraft:item", id)
        .and_then(|entry| host.registry_entry_in_tag(&entry, tag))
        .unwrap_or(false)
}
