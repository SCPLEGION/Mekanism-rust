//! A chemical tank as a block entity keeps it in its compound (`get_tank`/`put_tank` of `runtime.rs`), and the
//! usage config constants. Shared by every family that holds chemicals.
use super::nbt::{get_long, put_long};
use crate::gear::data as item_data;
use mekanism_core::chemical::ChemicalTank;
use mod_api::{Compound, Tag};

pub(crate) fn get_tank(nbt: &Compound, key: &str, capacity: u64) -> ChemicalTank {
    let mut t = ChemicalTank::new(capacity);
    if let Some(Tag::Compound(c)) = item_data::get(nbt, key)
        && let Some(Tag::String(id)) = item_data::get(c, "id")
        && let Ok(id) = id.to_string()
    {
        t.insert(&id, get_long(c, "amount"));
    }
    t
}

pub(crate) fn put_tank(nbt: &mut Compound, key: &str, t: &ChemicalTank) {
    match &t.chemical {
        Some(id) if t.amount > 0 => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_long(&mut c, "amount", t.amount);
            nbt.insert(key.into(), Tag::Compound(c));
        }
        _ => {
            nbt.retain(|k, _| k.to_string_lossy() != key);
        }
    }
}

/// A usage constant of the config (`usage_const` of `runtime.rs`): the config file is read by the skeleton later,
/// until then every constant has its default.
pub(crate) fn usage_const(_key: &str, default: u64) -> u64 {
    default
}

/// `chemical_need` of `runtime.rs`: the chemical a process draws this tick.
pub(crate) fn chemical_need(
    u: &mekanism_core::upgrades::Upgrades,
    base_ticks: u32,
    ticks: u32,
    progress: u32,
    used: u32,
    statistical: bool,
    chemical_upgrades: bool,
    roll: f64,
) -> u64 {
    use mekanism_core::upgrades::Upgrade;
    if statistical {
        return mekanism_core::upgrades::inverse_poisson(u.chemical_multiplier(), roll);
    }
    let total = if chemical_upgrades {
        (f64::from(base_ticks)
            * mekanism_core::upgrades::max_upgrade_multiplier()
                .powf(u.fraction_of(Upgrade::Speed) - u.fraction_of(Upgrade::Chemical)))
        .round() as i64
    } else {
        i64::from(base_ticks)
    };
    let (remaining, ticks_left) = (total - i64::from(used), i64::from(ticks) - i64::from(progress));
    if remaining < ticks_left {
        0
    } else if remaining == ticks_left || ticks_left <= 0 {
        1
    } else {
        (remaining as f64 / ticks_left as f64) as u64
    }
}

// --- Chemical tank items ------------------------------------------------------------------------------------

use super::common::key_of;
use crate::gear::data::{self as gdata, CHEMICAL_KEY};
use mekanism_core::tier::BaseTier;
use mod_api::{Host, ItemStack, ModBlockEntityView};

/// What a chemical item holds at most: a chemical tank of any tier, or the hohlraum.
pub(crate) fn chemical_item_capacity(id: &str) -> Option<u64> {
    if id == mekanism_core::fusion::HOHLRAUM {
        return Some(mekanism_core::fusion::HOHLRAUM_CAPACITY);
    }
    let tier = id.strip_prefix("mekanism:")?.strip_suffix("_chemical_tank")?;
    let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
    Some(chem::chemical_tank(tier).0)
}

use mekanism_core::chemical as chem;

/// What a chemical item takes or gives in one operation (the hohlraum's `fillRate`).
fn chemical_item_rate(id: &str) -> u64 {
    if id == mekanism_core::fusion::HOHLRAUM { mekanism_core::fusion::HOHLRAUM_FILL_RATE } else { u64::MAX }
}

fn chemical_item_accepts(id: &str, chemical: &str) -> bool {
    id != mekanism_core::fusion::HOHLRAUM || chemical == mekanism_core::fusion::FUSION_FUEL
}

/// The chemical an item holds, when it holds some.
pub(crate) fn item_tank(stack: &ItemStack, capacity: u64) -> Option<ChemicalTank> {
    let mut c = Compound::new();
    c.insert("t".into(), gdata::get(gdata::custom_data(stack)?, CHEMICAL_KEY)?.clone());
    let tank = get_tank(&c, "t", capacity);
    (tank.amount > 0).then_some(tank)
}

/// Writes `tank` as the chemical an item holds.
pub(crate) fn set_item_tank(stack: &mut ItemStack, tank: &ChemicalTank) {
    let mut c = Compound::new();
    put_tank(&mut c, "t", tank);
    let value = match gdata::get(&c, "t") {
        Some(Tag::Compound(t)) => Some(Tag::Compound(t.clone())),
        _ => None,
    };
    gdata::set_custom(stack, CHEMICAL_KEY, value);
}

/// `ChemicalInventorySlot.fill`: a chemical tank item in slot `from` gives what it holds to `tank` while the tank
/// takes it (`accepts`) and has room.
pub(crate) fn fill_tank_from_item(
    host: &dyn Host,
    entity: &mut ModBlockEntityView<'_>,
    from: usize,
    tank: &mut ChemicalTank,
    accepts: impl Fn(&str) -> bool,
) -> bool {
    let Some(Some(mut stack)) = entity.items.get(from).cloned() else {
        return false;
    };
    let id = super::item_id(host, &stack);
    let Some(capacity) = chemical_item_capacity(&id) else {
        return false;
    };
    let Some(mut held) = item_tank(&stack, capacity) else {
        return false;
    };
    let Some(chemical) = held.chemical.clone() else {
        return false;
    };
    if !accepts(&chemical) || !tank.accepts(&chemical) {
        return false;
    }
    let moved = held.amount.min(tank.capacity - tank.amount).min(chemical_item_rate(&id));
    if moved == 0 {
        return false;
    }
    tank.insert(&chemical, moved);
    held.extract(moved);
    set_item_tank(&mut stack, &held);
    entity.items[from] = Some(stack);
    true
}

/// `ChemicalInventorySlot.drain`: `tank`'s chemical goes into a chemical tank item in slot `from` that is empty or
/// holds the same chemical, while it has room.
pub(crate) fn drain_tank_into_item(
    host: &dyn Host,
    entity: &mut ModBlockEntityView<'_>,
    from: usize,
    tank: &mut ChemicalTank,
) -> bool {
    let Some(Some(mut stack)) = entity.items.get(from).cloned() else {
        return false;
    };
    let id = super::item_id(host, &stack);
    let (Some(capacity), Some(chemical)) = (chemical_item_capacity(&id), tank.chemical.clone()) else {
        return false;
    };
    let mut held = item_tank(&stack, capacity).unwrap_or_else(|| ChemicalTank::new(capacity));
    if !held.accepts(&chemical) || !chemical_item_accepts(&id, &chemical) {
        return false;
    }
    let moved = tank.amount.min(held.capacity - held.amount).min(chemical_item_rate(&id));
    if moved == 0 {
        return false;
    }
    held.insert(&chemical, moved);
    tank.extract(moved);
    set_item_tank(&mut stack, &held);
    entity.items[from] = Some(stack);
    true
}

/// A chemical item in slot `from` that converts into `tank`'s chemical (`find_conversion`), one item at a time
/// when all of its chemical fits (`ChemicalInventorySlot.fillOrConvert`).
pub(crate) fn convert_item_into_tank(
    host: &dyn Host,
    entity: &mut ModBlockEntityView<'_>,
    from: usize,
    tank: &mut ChemicalTank,
    accepts: impl Fn(&str) -> bool,
) {
    let Some(Some(item)) = entity.items.get(from).cloned() else {
        return;
    };
    let in_tag = |tag: &str, i: &str| super::in_tag(host, i, tag.trim_start_matches('#'));
    if let Some(c) = mekanism_core::recipes::find_conversion(&super::item_id(host, &item), &in_tag)
        && accepts(c.output)
        && tank.accepts(c.output)
        && tank.amount + c.amount <= tank.capacity
    {
        tank.insert(c.output, c.amount);
        entity.items[from] = item.shrink(1);
    }
}

/// `push_chemical` of `runtime.rs`: gives up to `rate` of `from` to the chemical handlers beside `pos`
/// (`transmit::push_chemical`). Every face of the giver lets it out until the side configuration lands.
pub(crate) fn push_chemical(host: &mut dyn mod_api::ServerHost, pos: [i32; 3], from: &mut ChemicalTank, rate: u64) {
    super::transmit::push_chemical(host, pos, from, None, rate, false, &|_, _| true);
}

/// An item stack of `count` of the item `id`.
pub(crate) fn stack_of(host: &dyn Host, id: &str, count: u32) -> Option<ItemStack> {
    key_of(host, id).and_then(|k| ItemStack::new(k, count))
}

/// `capacity` of a machine's energy and its upgraded numbers (`machine_power` of `runtime.rs`): the upgrades, the
/// energy per tick, the ticks one operation takes (0 with none), and the most it stores.
pub(crate) fn machine_power(
    nbt: &Compound,
    base_usage: u64,
    base_ticks: Option<u32>,
) -> (mekanism_core::upgrades::Upgrades, u64, u32, u64) {
    let u = super::common::upgrades(nbt);
    let usage = u.energy_per_tick(base_usage);
    let ticks = base_ticks.map_or(0, |t| u.ticks(t));
    let capacity = usage.saturating_mul(u64::from(ticks).max(mekanism_core::machine::STORAGE_MULTIPLIER)).max(usage);
    (u, usage, ticks, capacity)
}

