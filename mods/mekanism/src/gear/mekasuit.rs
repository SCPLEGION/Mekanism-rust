//! MekaSuit: what its pieces do on the server once a second. The Charge Distribution Unit moves energy between
//! the suit's pieces and into the inventory (`ModuleChargeDistributionUnit.tickServer`).
use mekanism_core::{gear as core_gear, modules};
use mod_api::{ItemStack, PlayerRef, ServerHost};

use super::data;

/// Inventory slots of the armor (`Inventory.EQUIPMENT_SLOT_MAPPING`): feet, legs, chest, head.
const ARMOR_SLOTS: [u32; 4] = [39, 38, 37, 36];
const PIECES: [&str; 4] = [modules::HELMET, modules::BODYARMOR, modules::PANTS, modules::BOOTS];
const UNIT: &str = "mekanism:charge_distribution_unit";

/// `slot` numbers of the player's inventory: 0..36 main, 36 feet, 37 legs, 38 chest, 39 head.
fn armor_slot(piece: usize) -> u32 {
    match piece {
        0 => 39,
        1 => 38,
        2 => 37,
        _ => 36,
    }
}

/// Once a second for each player: the body armor's Charge Distribution Unit.
pub(super) fn distribute(host: &mut dyn ServerHost, player: PlayerRef) {
    let Some(slots) = player.inventory_slots(&*host) else {
        return;
    };
    let _ = ARMOR_SLOTS;
    let mut slots: std::collections::BTreeMap<u32, ItemStack> = slots.into_iter().collect();
    let id = |host: &dyn mod_api::Host, stack: &ItemStack| stack.item().id(host);
    let Some(body) = slots.get(&armor_slot(1)).filter(|s| id(&*host, s) == modules::BODYARMOR).cloned() else {
        return;
    };
    // The two switches: the inventory (off by default) and the suit's own pieces (on).
    let charge_inventory = data::module_choice(&body, UNIT, "charge_inventory") == Some(1);
    let charge_suit = data::module_choice(&body, UNIT, "charge_suit") != Some(0);
    let mut changed: Vec<u32> = Vec::new();
    if charge_inventory {
        let rate = mekanism_core::config::gear_int("mekasuit.inventoryChargeRate", modules::INVENTORY_CHARGE_RATE) * 20;
        let mut available = data::stored_energy(&body).min(rate);
        let mut given = 0;
        let selected = player.selected_slot(&*host).unwrap_or(0) as u32;
        let order: Vec<u32> = std::iter::once(selected).chain((0..36).filter(|&i| i != selected)).collect();
        for i in order {
            if available == 0 {
                break;
            }
            let Some(stack) = slots.get_mut(&i) else {
                continue;
            };
            let item = id(&*host, stack);
            let Some(e) = data::item_energy_of(stack, &item) else {
                continue;
            };
            let stored = data::stored_energy(stack);
            let put = e.capacity.saturating_sub(stored).min(e.rate * 20).min(available);
            if put > 0 {
                data::set_energy(stack, stored + put);
                available -= put;
                given += put;
                changed.push(i);
            }
        }
        if given > 0 {
            if let Some(body) = slots.get_mut(&armor_slot(1)) {
                let stored = data::stored_energy(body);
                data::set_energy(body, stored - given);
                changed.push(armor_slot(1));
            }
        }
    }
    if charge_suit {
        let pieces: Vec<u32> = (0..4)
            .map(armor_slot)
            .filter(|s| slots.get(s).is_some_and(|stack| modules::is_container(&id(&*host, stack))))
            .collect();
        if pieces.len() >= 2 {
            let total: u64 = pieces.iter().filter_map(|s| slots.get(s)).map(data::stored_energy).sum();
            let caps: Vec<u64> = pieces.iter().filter_map(|s| slots.get(s)).map(|stack| data::item_energy_of(stack, &id(&*host, stack)).map_or(0, |e| e.capacity)).collect();
            for (s, share) in pieces.iter().zip(modules::split_evenly(total, &caps)) {
                if let Some(stack) = slots.get_mut(s) {
                    if data::stored_energy(stack) != share {
                        data::set_energy(stack, share);
                        changed.push(*s);
                    }
                }
            }
        }
    }
    changed.sort_unstable();
    changed.dedup();
    for slot in changed {
        let _ = player.set_inventory_slot(host, slot, slots.get(&slot).cloned());
    }
    let _ = (PIECES, core_gear::JETPACK_CAPACITY);
}
