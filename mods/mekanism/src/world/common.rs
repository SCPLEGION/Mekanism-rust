//! What every block entity family of the world area shares: slots as `mekanism-core` sees them, energy slots,
//! upgrades and the block's active state. Moved out of `machine.rs` so the other families reuse it.
use crate::gear::data as item_data;
use mekanism_core::energy::{Action, EnergyContainer};
use mekanism_core::machine::Slot;
use mekanism_core::upgrades::{Upgrade, Upgrades};
use mod_api::{Compound, Host, ItemStack, ModBlockEntityView, ServerHost, Tag};
use super::nbt::get_int;

/// Sets the block's `lit` (the furnace's property) or `open`, whichever it has, to show whether the block ran.
/// A property already at the value changes nothing.
pub(crate) fn set_active(host: &mut dyn ServerHost, pos: [i32; 3], active: bool) {
    let value = if active { "true" } else { "false" };
    for key in ["lit", "open"] {
        host.set_block_property(pos, key, value);
    }
}

/// One slot of the block entity as the machine's logic sees it.
pub(crate) fn slot(host: &dyn Host, stack: Option<&ItemStack>) -> Slot {
    match stack {
        Some(s) => Slot { item: Some(super::item_id(host, s)), count: s.count() },
        None => Slot::default(),
    }
}

/// Writes `slot` back to item slot `index`, keeping the stack's components when the item is the same.
pub(crate) fn set_slot(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, index: usize, slot: &Slot) {
    let Some(place) = entity.items.get_mut(index) else {
        return;
    };
    *place = match (&slot.item, place.take()) {
        (None, _) => None,
        (Some(_), Some(mut old)) if super::item_id(host, &old) == *slot.item.as_ref().unwrap() => {
            old.set_count(slot.count).ok().map(|()| old)
        }
        (Some(item), _) => key_of(host, item).and_then(|k| ItemStack::new(k, slot.count)),
    };
}

pub(crate) fn key_of(host: &dyn Host, id: &str) -> Option<mod_api::ItemKey> {
    super::item_key(host, id)
}

pub(crate) fn max_stack(host: &dyn Host, id: &str) -> u32 {
    key_of(host, id).and_then(|k| host.max_stack_size(k)).unwrap_or(64)
}

pub(crate) fn upgrades(nbt: &Compound) -> Upgrades {
    let Some(Tag::Compound(c)) = item_data::get(nbt, "upgrades") else {
        return Upgrades::default();
    };
    let mut u = Upgrades::default();
    for kind in Upgrade::ALL {
        u.install(kind, get_int(c, kind.name()));
    }
    u
}

/// `EnergyInventorySlot.fillContainerOrConvert`: empties the item in slot `index` into `target`, or, for
/// redstone, turns one of it into energy when all of that fits.
pub(crate) fn discharge_slot(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, index: usize, target: &mut EnergyContainer) {
    const REDSTONE_ENERGY: [(&str, u64); 2] = [("minecraft:redstone", 10_000), ("minecraft:redstone_block", 90_000)];
    let Some(Some(stack)) = entity.items.get(index) else {
        return;
    };
    let id = super::item_id(host, stack);
    if let Some(item) = item_data::item_energy_of(stack, &id) {
        let stored = item_data::stored_energy(stack).min(item.capacity);
        let after = mekanism_core::gear::discharge(item, stored, target);
        if after != stored
            && let Some(Some(stack)) = entity.items.get_mut(index)
        {
            item_data::set_energy(stack, after);
        }
    } else if let Some(&(_, energy)) = REDSTONE_ENERGY.iter().find(|(r, _)| *r == id)
        && target.stored + energy <= target.capacity
    {
        target.insert(energy, Action::Execute);
        let Some(Some(stack)) = entity.items.get_mut(index) else {
            return;
        };
        if stack.count() <= 1 {
            entity.items[index] = None;
        } else {
            let _ = stack.set_count(stack.count() - 1);
        }
    }
}


/// `EnergyInventorySlot.charge`: fills the item in slot `index` from `source`.
pub(crate) fn charge_slot(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, index: usize, source: &mut EnergyContainer) {
    let Some(Some(stack)) = entity.items.get(index) else {
        return;
    };
    let id = super::item_id(host, stack);
    let Some(item) = item_data::item_energy_of(stack, &id) else {
        return;
    };
    let stored = item_data::stored_energy(stack).min(item.capacity);
    let after = mekanism_core::gear::charge(item, stored, source);
    if after != stored
        && let Some(Some(stack)) = entity.items.get_mut(index)
    {
        item_data::set_energy(stack, after);
    }
}

/// What clients get of a block entity: the `client` compound of its saved data (`put_client`), which a client's
/// copy merges into its own `nbt`, where the screens read it. Every family's `block_entity_update_tag` returns this.
pub(crate) fn client_tag(entity: &mod_api::ModBlockEntity) -> Option<Compound> {
    let Some(Tag::Compound(client)) = crate::gear::data::get(&entity.nbt, "client") else {
        return None;
    };
    let mut tag = Compound::new();
    tag.insert("client".into(), Tag::Compound(client.clone()));
    Some(tag)
}

/// `put_client` of `runtime.rs`: `new` replaces the keys in `owned` of the compound clients see. A change is
/// queued for the viewers (`set_client_update_fields`), so they hear of it with this tick's write-back.
pub(crate) fn put_client(entity: &mut ModBlockEntityView<'_>, owned: &[&str], new: Compound) {
    let before = match crate::gear::data::get(&entity.nbt, "client") {
        Some(Tag::Compound(c)) => Some(c.clone()),
        _ => None,
    };
    let mut client = before.clone().unwrap_or_default();
    client.retain(|k, _| !owned.contains(&k.to_string_lossy().as_str()));
    for (k, v) in new.iter() {
        client.insert(k.clone(), v.clone());
    }
    if before.as_ref() != Some(&client) {
        entity.nbt.insert("client".into(), Tag::Compound(client.clone()));
        let mut fields = Compound::new();
        fields.insert("client".into(), Tag::Compound(client));
        entity.set_client_update_fields(fields);
    }
}
