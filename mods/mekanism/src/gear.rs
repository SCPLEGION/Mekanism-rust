//! The `gear` area of the mod (see lib.rs). Hook bodies live here; lib.rs only delegates.
//! Meka-Tool, MekaSuit, modules, jetpacks, weapons, tooltips, keys, HUD and commands.
//!
//! On the fork every gear item went through global hooks that asked the item's name; here each gear item gets a
//! [`GearPart`] in `register`, and the game asks it only for its own item (docs/horizon-api-migration.md).
use mekanism_core::{gear as core_gear, modules};
use mod_api::{ClientHost, ItemBarEvent, ItemDecorationsEvent, ItemPart, Registrar};

pub(crate) mod data;

/// State of this area, held in `Mod`. Items carry their own state in `custom_data`, so there is none yet.
#[derive(Default)]
pub(crate) struct State {}

const SCUBA_TANK: &str = "mekanism:scuba_tank";
const CANTEEN: &str = "mekanism:canteen";
const FLAMETHROWER: &str = mekanism_core::flamethrower::ITEM;
const JETPACKS: [&str; 2] = ["mekanism:jetpack", "mekanism:jetpack_armored"];

/// Items whose bar and tooltip are a chemical tank, not energy.
const TANK_ITEMS: [&str; 5] = [JETPACKS[0], JETPACKS[1], SCUBA_TANK, CANTEEN, FLAMETHROWER];

/// Every item of the gear area gets a [`GearPart`].
pub(crate) fn register(r: &mut dyn Registrar) {
    let mut ids = core_gear::energy_item_ids();
    ids.extend(TANK_ITEMS.map(str::to_owned));
    ids.sort();
    ids.dedup();
    for id in ids {
        let key = r.item_key(&id);
        // The part knows its item: the game asks it for that item only.
        r.item_part(key, GearPart { id: Box::leak(id.into_boxed_str()) });
    }
}

/// What every piece of Mekanism gear shares: bars, tooltips, and (in later stages) use, tick and wear.
struct GearPart {
    id: &'static str,
}

fn width(amount: u64, capacity: u64) -> i32 {
    ((13.0 * amount as f64 / capacity.max(1) as f64).round() as i32).clamp(0, 13)
}

/// Bar colour of a chemical (`Chemical.getTint`); grey for one the gear never holds.
pub(crate) fn chemical_color(id: &str) -> u32 {
    match id {
        "mekanism:hydrogen" => 0xFFFFFF,
        "mekanism:oxygen" => 0x6CE2FF,
        "mekanism:nutritional_paste" => 0xEB6CA3,
        _ => 0xAAAAAA,
    }
}

#[mod_api::horizon_part]
impl ItemPart for GearPart {
    /// `ItemEnergyBarDecorator` (0x3CFE9A) for an item holding energy; a tank item (jetpack, scuba tank,
    /// flamethrower, canteen) shows its tank in the chemical's colour instead.
    fn item_bar(&self, _: &dyn ClientHost, event: &mut ItemBarEvent<'_>) {
        let stack = event.stack;
        if stack.count() != 1 {
            return;
        }
        if TANK_ITEMS.contains(&self.id) {
            let fuel = data::fuel(stack, self.id);
            if fuel > 0 {
                *event.bar = Some((width(fuel, data::tank_capacity(stack, self.id)), chemical_color(data::item_chemical(self.id))));
            }
        } else if let Some(e) = data::item_energy_of(stack, self.id) {
            let stored = data::stored_energy(stack);
            if stored > 0 {
                *event.bar = Some((width(stored, e.capacity), 0x3CFE9A));
            }
        }
    }

    /// The second bar of a MekaSuit piece (`MekaSuitBarDecorator`): the tank of the piece, under the energy bar:
    /// the body armor's hydrogen with a Jetpack Unit, the helmet's nutritional paste with a Nutritional Injection Unit.
    fn item_decorations(&self, _: &dyn ClientHost, event: &mut ItemDecorationsEvent<'_>) {
        let stack = event.stack;
        if stack.count() != 1 {
            return;
        }
        let (amount, capacity, color) = if self.id == modules::BODYARMOR && data::enabled_modules(stack).contains_key(data::JETPACK_UNIT) {
            (data::fuel(stack, self.id), data::tank_capacity(stack, self.id), chemical_color(data::item_chemical(self.id)))
        } else if self.id == modules::HELMET && data::enabled_modules(stack).contains_key(data::NUTRITION_UNIT) {
            (data::suit_paste(stack), data::suit_paste_capacity(), chemical_color(mekanism_core::chemical::NUTRITIONAL_PASTE))
        } else {
            return;
        };
        let (x, y) = (event.x, event.y);
        event.draw.fill([x + 2.0, y + 12.0, 13.0, 2.0], 0xFF00_0000);
        event.draw.fill([x + 2.0, y + 12.0, width(amount, capacity) as f32, 1.0], 0xFF00_0000 | color);
    }
}
