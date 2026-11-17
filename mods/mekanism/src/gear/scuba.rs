//! Scuba gear: a mask on the head and a flowing tank with oxygen on the chest (`CommonPlayerTickHandler.isScubaMaskOn`).
use mod_api::{EquipmentSlot, PlayerRef, ServerHost};

use super::data;

pub(super) const MASK: &str = "mekanism:scuba_mask";
pub(super) const TANK: &str = "mekanism:scuba_tank";
const MODE_KEY: &str = "mekanism:scuba_tank_mode";

/// `ItemScubaTank.getMode`: whether the tank flows.
pub(super) fn flowing(stack: &mod_api::ItemStack) -> bool {
    data::custom_data(stack).is_some_and(|c| data::get_long(c, MODE_KEY) == 1)
}

/// Every tick for each player: one mB of oxygen and the air missing, while the mask is on and the air is short
/// (`CommonPlayerTickHandler.playerTick`).
pub(super) fn breathe(host: &mut dyn ServerHost, player: PlayerRef) {
    let entity = player.entity();
    let (Some(air), Some(max)) = (entity.air_supply(&*host), entity.max_air_supply(&*host)) else {
        return;
    };
    if air >= max {
        return;
    }
    let Some(living) = host.living(entity) else {
        return;
    };
    let on = living.equipment(&*host, EquipmentSlot::Head).is_some_and(|s| s.item().id(&*host) == MASK);
    if !on {
        return;
    }
    let Some(mut tank) = living.equipment(&*host, EquipmentSlot::Chest) else {
        return;
    };
    if tank.item().id(&*host) != TANK || !flowing(&tank) {
        return;
    }
    let stored = data::fuel(&tank, TANK);
    if stored == 0 {
        return;
    }
    let give = (max - air).max(0) as u64;
    // The first mB is spent whatever happens, then as much as the air can take.
    let used = (1 + give).min(stored);
    let gained = used.saturating_sub(1).min(give);
    data::set_fuel(&mut tank, TANK, stored - used);
    let _ = living.set_equipment(host, EquipmentSlot::Chest, Some(tank));
    entity.set_air_supply(host, (air + gained as i32).min(max));
}
