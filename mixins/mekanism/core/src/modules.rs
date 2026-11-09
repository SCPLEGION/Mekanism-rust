// Modules (mekanism.common.content.gear, MekanismModules, the `supported_modules` data map) and the
// Meka-Tool (ItemMekaTool): module items installed into the Meka-Tool and the MekaSuit at a Modification
// Station change what they do.

use std::collections::BTreeMap;

pub const MEKA_TOOL: &str = "mekanism:meka_tool";
pub const HELMET: &str = "mekanism:mekasuit_helmet";
pub const BODYARMOR: &str = "mekanism:mekasuit_bodyarmor";
pub const PANTS: &str = "mekanism:mekasuit_pants";
pub const BOOTS: &str = "mekanism:mekasuit_boots";
pub const STATION: &str = "mekanism:modification_station";
/// The item a module is, told apart by `mekanism:module_type` in its custom data.
pub const MODULE_ITEM: &str = "mekanism:module_base";

/// `GearConfig`: the Meka-Tool's and the MekaSuit's base energy and charge rate.
pub const BASE_CAPACITY: u64 = 16_000_000;
pub const BASE_CHARGE_RATE: u64 = 100_000;
/// `GearConfig.mekaTool*`.
pub const TOOL_BASE_EFFICIENCY: u64 = 4;
pub const TOOL_ENERGY_USAGE: u64 = 10;
/// `GearConfig.mekaToolEnergyUsageSilk`.
pub const TOOL_ENERGY_USAGE_SILK: u64 = 100;
/// `GearConfig.disassemblerMiningCount`: how many blocks of each ore a vein takes.
pub const VEIN_COUNT: usize = 128;
pub const TOOL_TELEPORT_ENERGY: u64 = 1_000;
pub const TOOL_TELEPORT_REACH: f64 = 100.0;

/// Which items take a module (`supported_modules`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fits {
    Any,
    Armor,
    Tool,
    Helmet,
    Chest,
    Legs,
    Boots,
}

/// Every module: its id, how many can be installed (`maxStackSize`) and what takes it.
pub const MODULES: &[(&str, u32, Fits)] = &[
    ("mekanism:energy_unit", 8, Fits::Any),
    ("mekanism:laser_dissipation_unit", 1, Fits::Armor),
    ("mekanism:radiation_shielding_unit", 1, Fits::Armor),
    ("mekanism:excavation_escalation_unit", 4, Fits::Tool),
    ("mekanism:attack_amplification_unit", 4, Fits::Tool),
    ("mekanism:farming_unit", 4, Fits::Tool),
    ("mekanism:shearing_unit", 1, Fits::Tool),
    ("mekanism:silk_touch_unit", 1, Fits::Tool),
    ("mekanism:fortune_unit", 3, Fits::Tool),
    ("mekanism:blasting_unit", 4, Fits::Tool),
    ("mekanism:vein_mining_unit", 4, Fits::Tool),
    ("mekanism:teleportation_unit", 1, Fits::Tool),
    ("mekanism:electrolytic_breathing_unit", 4, Fits::Helmet),
    ("mekanism:inhalation_purification_unit", 1, Fits::Helmet),
    ("mekanism:vision_enhancement_unit", 4, Fits::Helmet),
    ("mekanism:nutritional_injection_unit", 1, Fits::Helmet),
    ("mekanism:dosimeter_unit", 1, Fits::Chest),
    ("mekanism:geiger_unit", 1, Fits::Chest),
    ("mekanism:jetpack_unit", 4, Fits::Chest),
    ("mekanism:charge_distribution_unit", 1, Fits::Chest),
    ("mekanism:gravitational_modulating_unit", 1, Fits::Chest),
    ("mekanism:elytra_unit", 1, Fits::Chest),
    ("mekanism:locomotive_boosting_unit", 4, Fits::Legs),
    ("mekanism:gyroscopic_stabilization_unit", 1, Fits::Legs),
    ("mekanism:hydrostatic_repulsor_unit", 4, Fits::Legs),
    ("mekanism:motorized_servo_unit", 5, Fits::Legs),
    ("mekanism:hydraulic_propulsion_unit", 4, Fits::Boots),
    ("mekanism:magnetic_attraction_unit", 4, Fits::Boots),
    ("mekanism:frost_walker_unit", 2, Fits::Boots),
    ("mekanism:soul_surfer_unit", 3, Fits::Boots),
    ("mekanismgenerators:solar_recharging_unit", 8, Fits::Helmet),
    (
        "mekanismgenerators:geothermal_generator_unit",
        8,
        Fits::Legs,
    ),
];

pub fn is_container(item: &str) -> bool {
    [MEKA_TOOL, HELMET, BODYARMOR, PANTS, BOOTS].contains(&item)
}

/// How many of `module` `item` takes; 0 when it doesn't.
pub fn max_installed(item: &str, module: &str) -> u32 {
    let Some(&(_, max, fits)) = MODULES.iter().find(|(id, ..)| *id == module) else {
        return 0;
    };
    let ok = match fits {
        Fits::Any => is_container(item),
        Fits::Armor => [HELMET, BODYARMOR, PANTS, BOOTS].contains(&item),
        Fits::Tool => item == MEKA_TOOL,
        Fits::Helmet => item == HELMET,
        Fits::Chest => item == BODYARMOR,
        Fits::Legs => item == PANTS,
        Fits::Boots => item == BOOTS,
    };
    if ok { max } else { 0 }
}

/// What a container item has installed, by module id.
pub type Installed = BTreeMap<String, u32>;

/// Installs up to `n` of `module` into `item`'s `installed` and says how many went in.
pub fn install(item: &str, installed: &mut Installed, module: &str, n: u32) -> u32 {
    let have = installed.get(module).copied().unwrap_or(0);
    let put = n.min(max_installed(item, module).saturating_sub(have));
    if put > 0 {
        installed.insert(module.to_owned(), have + put);
    }
    put
}

/// `ModuleEnergyUnit`: each energy unit doubles the capacity and the charge rate.
pub fn energy(installed: &Installed) -> (u64, u64) {
    energy_from(installed, BASE_CAPACITY, BASE_CHARGE_RATE)
}

/// The same from a configured base (`gear.mekatool` / `gear.mekasuit` `baseEnergyCapacity` and
/// `baseChargeRate`).
pub fn energy_from(installed: &Installed, capacity: u64, rate: u64) -> (u64, u64) {
    let n = installed.get("mekanism:energy_unit").copied().unwrap_or(0).min(32);
    (capacity.saturating_mul(1u64 << n), rate.saturating_mul(1u64 << n))
}

/// The Meka-Tool's efficiency: 4 without an Excavation Escalation Unit, and with one its fastest mode
/// the installed units reach (`ExcavationMode`: normal 16, fast 32, super fast 64, extreme 128).
pub fn tool_efficiency(installed: &Installed) -> u64 {
    match installed
        .get("mekanism:excavation_escalation_unit")
        .copied()
        .unwrap_or(0)
    {
        0 => TOOL_BASE_EFFICIENCY,
        n => 8 << n.min(4),
    }
}

/// `ItemMekaTool.getDestroyEnergy`: 10 FE (100 with silk touch) times the efficiency, half for blocks
/// that break at once.
pub fn tool_destroy_energy(efficiency: u64, hardness: f32, silk: bool) -> u64 {
    let usage = if silk {
        TOOL_ENERGY_USAGE_SILK
    } else {
        TOOL_ENERGY_USAGE
    };
    halved(usage * efficiency, hardness)
}

fn halved(base: u64, hardness: f32) -> u64 {
    if hardness == 0.0 {
        (base / 2).max(1)
    } else {
        base
    }
}

/// What a block vein-mined or blasted `distance` steps from the first one costs (`veinMineArea`'s
/// multiplier, on the energy without the efficiency's share): half the base energy times the
/// distance to the power 1.5 for ores and 2 for anything else.
pub fn area_energy(silk: bool, hardness: f32, distance: u32, ore: bool) -> u64 {
    let usage = if silk {
        TOOL_ENERGY_USAGE_SILK
    } else {
        TOOL_ENERGY_USAGE
    };
    let multiplier = 0.5 * f64::from(distance).powf(if ore { 1.5 } else { 2.0 });
    (halved(usage, hardness) as f64 * multiplier).ceil() as u64
}

/// `ModuleBlastingUnit.BlastingPower`: the radius of the strongest power the installed units reach.
pub fn blast_radius(installed: &Installed) -> i32 {
    installed
        .get("mekanism:blasting_unit")
        .copied()
        .unwrap_or(0)
        .min(4) as i32
}

/// `ModuleVeinMiningUnit`: whether vein mining is installed.
pub fn veins(installed: &Installed) -> bool {
    installed
        .get("mekanism:vein_mining_unit")
        .copied()
        .unwrap_or(0)
        > 0
}

/// `IBlastingItem.findPositions`: the offsets a blast clears around the mined block, by the axis of the
/// face it was hit on (0 x, 1 y, 2 z): a square flat on top and bottom faces, and on side faces a wall
/// from one block below to `2r - 1` above.
pub fn blast_offsets(radius: i32, axis: usize) -> Vec<[i32; 3]> {
    let (lo, hi) = match axis {
        1 => ([-radius, 0, -radius], [radius, 0, radius]),
        0 => ([0, -1, -radius], [0, 2 * radius - 1, radius]),
        _ => ([-radius, -1, 0], [radius, 2 * radius - 1, 0]),
    };
    let mut out = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                if [x, y, z] != [0, 0, 0] {
                    out.push([x, y, z]);
                }
            }
        }
    }
    out
}

/// The enchantments a container's enchantment modules stand for (`registerEnchantBased`): silk touch
/// (which wins over fortune, as they exclude each other), fortune, frost walker and soul speed (the Soul Surfer
/// Unit), at the installed count.
pub fn enchantments(installed: &Installed) -> Vec<(&'static str, u32)> {
    let n = |m: &str| installed.get(m).copied().unwrap_or(0);
    let mut out = Vec::new();
    if n("mekanism:silk_touch_unit") > 0 {
        out.push(("minecraft:silk_touch", 1));
    } else if n("mekanism:fortune_unit") > 0 {
        out.push(("minecraft:fortune", n("mekanism:fortune_unit")));
    }
    if n("mekanism:frost_walker_unit") > 0 {
        out.push(("minecraft:frost_walker", n("mekanism:frost_walker_unit")));
    }
    if n("mekanism:soul_surfer_unit") > 0 {
        out.push(("minecraft:soul_speed", n("mekanism:soul_surfer_unit")));
    }
    out
}

pub const MODULE_ENCHANTMENTS: [&str; 4] = [
    "minecraft:silk_touch",
    "minecraft:fortune",
    "minecraft:frost_walker",
    "minecraft:soul_speed",
];

/// `ItemMekaTool.getDestroySpeed`: the efficiency while the energy is there, the base efficiency scaled
/// by how much of it there is otherwise.
pub fn tool_destroy_speed(efficiency: u64, stored: u64, hardness: f32, silk: bool) -> f32 {
    let required = tool_destroy_energy(efficiency, hardness, silk);
    if stored < required {
        return TOOL_BASE_EFFICIENCY as f32 * (stored as f32 / required as f32);
    }
    efficiency as f32
}

/// `ItemMekaTool.use` with a Teleportation Unit: 1,000 FE for each 10 blocks² of distance.
pub fn teleport_energy(distance_sq: f64) -> u64 {
    (TOOL_TELEPORT_ENERGY as f64 * (distance_sq / 10.0)).ceil() as u64
}

/// `GearConfig.mekaToolEnergyUsageWeapon`: the energy a hit costs for each 4 of extra damage.
pub const TOOL_WEAPON_ENERGY: f64 = 2_000.0;

/// `ModuleAttackAmplificationUnit.AttackDamage`: the extra damage of the strongest mode the installed
/// units reach (low 4, medium 8, high 16, extreme 24).
pub fn attack_bonus(installed: &Installed) -> f32 {
    [0.0, 4.0, 8.0, 16.0, 24.0][installed
        .get("mekanism:attack_amplification_unit")
        .copied()
        .unwrap_or(0)
        .min(4) as usize]
}

/// `ItemMekaTool.adjustAttributes` and `hurtEnemy`: the extra damage a hit gets with `stored` energy, and
/// the energy it costs (2,000 FE per 4 damage). Short of the energy, the damage is scaled down by how
/// much there is.
pub fn attack(bonus: f32, stored: u64) -> (f32, u64) {
    if bonus <= 0.0 {
        return (0.0, 0);
    }
    let cost = (TOOL_WEAPON_ENERGY * f64::from(bonus) / 4.0) as u64;
    if stored >= cost {
        (bonus, cost)
    } else {
        (bonus * stored as f32 / cost as f32, stored)
    }
}

/// `GearConfig.mekaSuitJetpackMaxStorage`: 24 buckets of hydrogen.
pub const SUIT_JETPACK_CAPACITY: u64 = 24_000;
/// `GearConfig.mekaSuitEnergyUsageSprintBoost`.
pub const SUIT_SPRINT_ENERGY: f64 = 100.0;

fn units(installed: &Installed, module: &str) -> u32 {
    installed.get(module).copied().unwrap_or(0)
}

/// `ModuleHydraulicPropulsionUnit.StepAssist`: the highest step the installed units reach (low 0.5,
/// medium 1, high 1.5, ultra 2 blocks), 0 without the unit.
pub fn step_assist(installed: &Installed) -> f64 {
    [0.0, 0.5, 1.0, 1.5, 2.0]
        [units(installed, "mekanism:hydraulic_propulsion_unit").min(4) as usize]
}

/// `ModuleLocomotiveBoostingUnit.SprintBoost`: the strongest boost the installed units reach (low
/// 0.05, medium 0.1, high 0.25, ultra 0.5), 0 without the unit.
pub fn sprint_boost(installed: &Installed) -> f32 {
    [0.0, 0.05, 0.1, 0.25, 0.5]
        [units(installed, "mekanism:locomotive_boosting_unit").min(4) as usize]
}

/// The energy a tick of sprint boost costs (`canFunction`: 100 FE for each 0.1 of boost).
pub fn sprint_energy(boost: f32) -> u64 {
    (SUIT_SPRINT_ENERGY * f64::from(boost) / 0.1).round() as u64
}

/// `ModuleLocomotiveBoostingUnit.tick`: `moveRelative(boost, (0, 0, 1))` toward the player's yaw, a fifth
/// as strong in the air or in water.
pub fn sprint_push(boost: f32, yaw_degrees: f32, on_ground: bool, in_water: bool) -> [f64; 2] {
    let mut b = boost;
    if !on_ground {
        b /= 5.0;
    }
    if in_water {
        b /= 5.0;
    }
    let yaw = f64::from(yaw_degrees).to_radians();
    [-yaw.sin() * f64::from(b), yaw.cos() * f64::from(b)]
}

/// `ModuleElectrolyticBreathingUnit`: 2^units of oxygen (air) a tick under water, each costing twice
/// hydrogen's energy density (4 FE); the hydrogen it makes alongside (two for each) fills a jetpack on
/// the chest.
pub const BREATHING_USAGE: u64 = 4;

pub fn breathing_rate(installed: &Installed) -> u32 {
    match installed
        .get("mekanism:electrolytic_breathing_unit")
        .copied()
        .unwrap_or(0)
    {
        0 => 0,
        n => 1 << n.min(4),
    }
}

/// One breathing tick: the air given, the hydrogen made and the energy it all costs, for `rate`, the
/// air missing and the hydrogen room on the chest.
pub fn breathe(rate: u32, air_missing: u32, hydrogen_room: u64) -> (u32, u64, u64) {
    let oxygen = rate.min(air_missing);
    let hydrogen = u64::from(2 * rate).min(hydrogen_room);
    let used = u64::from(oxygen).max(hydrogen.div_ceil(2));
    (oxygen, hydrogen, BREATHING_USAGE * used)
}

/// `GearConfig.mekaSuitSolarRechargingRate`: the Solar Recharging Unit's peak, times the installed units.
pub const SOLAR_RATE: u64 = 500;
/// `GearConfig.mekaSuitInventoryChargeRate`: what the Charge Distribution Unit gives the inventory a
/// tick.
pub const INVENTORY_CHARGE_RATE: u64 = 10_000;

/// `EmitUtils.sendToAcceptors` over the worn pieces: `total` split evenly by count, what a full piece
/// can't take going to the others.
pub fn split_evenly(total: u64, capacities: &[u64]) -> Vec<u64> {
    let mut out = vec![0; capacities.len()];
    let mut left = total;
    let mut open: Vec<usize> = (0..capacities.len()).collect();
    while left > 0 && !open.is_empty() {
        let share = (left / open.len() as u64).max(1);
        let mut next = Vec::new();
        for &i in &open {
            let give = share.min(capacities[i] - out[i]).min(left);
            out[i] += give;
            left -= give;
            if out[i] < capacities[i] {
                next.push(i);
            }
            if left == 0 {
                break;
            }
        }
        open = next;
    }
    out
}

/// `GearConfig.mekaSuitEnergyUsageDamage`: the energy each point of damage a MekaSuit piece takes costs.
pub const SUIT_DAMAGE_ENERGY: f32 = 100_000.0;

/// `ItemMekaSuitArmor`'s absorption of each piece (helmet, body armor, pants, boots): a whole suit
/// takes every hit that armor can stop.
pub const SUIT_ABSORPTION: [f32; 4] = [0.15, 0.4, 0.3, 0.15];

/// The share of each hit the worn pieces take: those with the energy for at least one point of it.
pub fn suit_share(stored: [Option<u64>; 4]) -> f32 {
    stored
        .iter()
        .zip(SUIT_ABSORPTION)
        .filter(|(e, _)| e.is_some_and(|e| e as f32 >= SUIT_DAMAGE_ENERGY))
        .map(|(_, r)| r)
        .sum()
}

/// What `absorbed` damage, taken at `suit_share(stored)`, costs each piece (never more than it holds).
pub fn suit_costs(absorbed: f32, stored: [Option<u64>; 4]) -> [u64; 4] {
    let share = suit_share(stored);
    let mut out = [0; 4];
    if share <= 0.0 || absorbed <= 0.0 {
        return out;
    }
    for (i, e) in stored.iter().enumerate() {
        if let Some(e) = e.filter(|&e| e as f32 >= SUIT_DAMAGE_ENERGY) {
            let cost = absorbed * SUIT_ABSORPTION[i] / share * SUIT_DAMAGE_ENERGY;
            out[i] = (cost.round() as u64).min(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suit_absorbs_with_energy() {
        let full = Some(16_000_000);
        assert!((suit_share([full; 4]) - 1.0).abs() < 1e-6);
        assert!((suit_share([None, full, Some(10), full]) - 0.55).abs() < 1e-6);
        // 5.5 damage taken by body armor (0.4) and boots (0.15).
        let costs = suit_costs(5.5, [None, full, Some(10), full]);
        assert_eq!(costs, [0, 400_000, 0, 150_000]);
        assert_eq!(
            suit_costs(1.0, [Some(150_000), None, None, None]),
            [100_000, 0, 0, 0]
        );
    }

    #[test]
    fn modules_fit_their_items_up_to_their_limit() {
        let mut m = Installed::new();
        assert_eq!(
            install(MEKA_TOOL, &mut m, "mekanism:excavation_escalation_unit", 9),
            4
        );
        assert_eq!(install(MEKA_TOOL, &mut m, "mekanism:jetpack_unit", 1), 0);
        assert_eq!(install(BODYARMOR, &mut m, "mekanism:jetpack_unit", 1), 1);
        assert_eq!(install(BOOTS, &mut m, "mekanism:energy_unit", 3), 3);
        assert_eq!(energy(&m), (128_000_000, 800_000));
        assert_eq!(energy_from(&m, 1_000, 10), (8_000, 80));
        assert_eq!(tool_efficiency(&m), 128);
    }

    #[test]
    fn tool_energy() {
        let none = Installed::new();
        assert_eq!(tool_efficiency(&none), 4);
        assert_eq!(tool_destroy_energy(16, 1.5, false), 160);
        assert_eq!(tool_destroy_energy(16, 0.0, false), 80);
        assert_eq!(tool_destroy_energy(4, 1.5, true), 400);
        assert_eq!(tool_destroy_speed(16, 80, 1.5, false), 2.0);
        assert_eq!(area_energy(false, 3.0, 2, false), 20);
        assert_eq!(area_energy(false, 3.0, 4, true), 40);
        assert_eq!(teleport_energy(250.0), 25_000);
    }

    #[test]
    fn blasting_and_enchantments() {
        assert_eq!(blast_offsets(1, 1).len(), 8);
        let wall = blast_offsets(2, 2);
        assert_eq!(wall.len(), 5 * 5 - 1);
        assert!(wall.contains(&[0, -1, 0]) && wall.contains(&[2, 3, 0]));
        let mut m = Installed::new();
        install(MEKA_TOOL, &mut m, "mekanism:fortune_unit", 3);
        assert_eq!(enchantments(&m), vec![("minecraft:fortune", 3)]);
        install(MEKA_TOOL, &mut m, "mekanism:silk_touch_unit", 1);
        assert_eq!(enchantments(&m), vec![("minecraft:silk_touch", 1)]);
    }

    #[test]
    fn attack_amplification() {
        let mut m = Installed::new();
        assert_eq!(attack(attack_bonus(&m), 1_000), (0.0, 0));
        install(MEKA_TOOL, &mut m, "mekanism:attack_amplification_unit", 2);
        assert_eq!(attack_bonus(&m), 8.0);
        assert_eq!(attack(8.0, 10_000), (8.0, 4_000));
        assert_eq!(attack(8.0, 1_000), (2.0, 1_000));
    }

    #[test]
    fn breathing_and_sharing() {
        let mut m = Installed::new();
        assert_eq!(breathing_rate(&m), 0);
        install(HELMET, &mut m, "mekanism:electrolytic_breathing_unit", 2);
        assert_eq!(breathing_rate(&m), 4);
        assert_eq!(breathe(4, 3, 0), (3, 0, 12));
        assert_eq!(breathe(4, 0, 100), (0, 8, 16));
        assert_eq!(split_evenly(100, &[10, 1_000, 1_000]), vec![10, 45, 45]);
        assert_eq!(split_evenly(7, &[100, 100]), vec![4, 3]);
    }

    #[test]
    fn suit_modules() {
        let mut m = Installed::new();
        assert_eq!(step_assist(&m), 0.0);
        install(BOOTS, &mut m, "mekanism:hydraulic_propulsion_unit", 2);
        assert_eq!(step_assist(&m), 1.0);
        install(PANTS, &mut m, "mekanism:locomotive_boosting_unit", 4);
        assert_eq!(sprint_boost(&m), 0.5);
        assert_eq!(sprint_energy(0.5), 500);
        let [x, z] = sprint_push(0.5, 0.0, true, false);
        assert!(x.abs() < 1e-9 && (z - 0.5).abs() < 1e-9);
        let [_, z] = sprint_push(0.5, 0.0, false, true);
        assert!((z - 0.02).abs() < 1e-6);
    }
}

/// `UsageConfig.modificationStation`: what the Modification Station takes a tick.
pub const STATION_USAGE: u64 = 10;
/// `TileEntityModificationStation.BASE_TICKS_REQUIRED`: half a second per install.
pub const STATION_TICKS: u32 = 10;

/// The module types `item` takes, in the order of `MODULES` (`ModuleContainer.moduleTypes`).
pub fn supported(item: &str) -> Vec<&'static str> {
    MODULES.iter().filter(|(id, ..)| max_installed(item, id) > 0).map(|(id, ..)| *id).collect()
}

/// Takes up to `n` of `module` out of `installed` and says how many came out.
pub fn uninstall(installed: &mut Installed, module: &str, n: u32) -> u32 {
    let have = installed.get(module).copied().unwrap_or(0);
    let out = n.min(have);
    if out == have {
        installed.remove(module);
    } else if out > 0 {
        installed.insert(module.to_owned(), have - out);
    }
    out
}

#[cfg(test)]
mod station_tests {
    use super::*;

    #[test]
    fn station_lists_and_removes() {
        assert!(supported(MEKA_TOOL).contains(&"mekanism:silk_touch_unit"));
        assert!(!supported(HELMET).contains(&"mekanism:silk_touch_unit"));
        let mut m = Installed::new();
        assert_eq!(install(MEKA_TOOL, &mut m, "mekanism:energy_unit", 3), 3);
        assert_eq!(uninstall(&mut m, "mekanism:energy_unit", 1), 1);
        assert_eq!(uninstall(&mut m, "mekanism:energy_unit", 5), 2);
        assert!(m.is_empty());
    }
}
