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
