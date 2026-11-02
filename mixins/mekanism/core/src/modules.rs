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
    let n = installed.get("mekanism:energy_unit").copied().unwrap_or(0);
    (BASE_CAPACITY << n, BASE_CHARGE_RATE << n)
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

/// `ItemMekaTool.getDestroyEnergy`: 10 FE times the efficiency, half for blocks that break at once.
pub fn tool_destroy_energy(efficiency: u64, hardness: f32) -> u64 {
    let base = TOOL_ENERGY_USAGE * efficiency;
    if hardness == 0.0 {
        (base / 2).max(1)
    } else {
        base
    }
}

/// `ItemMekaTool.getDestroySpeed`: the efficiency while the energy is there, the base efficiency scaled
/// by how much of it there is otherwise.
pub fn tool_destroy_speed(efficiency: u64, stored: u64, hardness: f32) -> f32 {
    let required = tool_destroy_energy(efficiency, hardness);
    if stored < required {
        return TOOL_BASE_EFFICIENCY as f32 * (stored as f32 / required as f32);
    }
    efficiency as f32
}

/// `ItemMekaTool.use` with a Teleportation Unit: 1,000 FE for each 10 blocks² of distance.
pub fn teleport_energy(distance_sq: f64) -> u64 {
    (TOOL_TELEPORT_ENERGY as f64 * (distance_sq / 10.0)).ceil() as u64
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(tool_efficiency(&m), 128);
    }

    #[test]
    fn tool_energy() {
        let none = Installed::new();
        assert_eq!(tool_efficiency(&none), 4);
        assert_eq!(tool_destroy_energy(16, 1.5), 160);
        assert_eq!(tool_destroy_energy(16, 0.0), 80);
        assert_eq!(tool_destroy_speed(16, 80, 1.5), 2.0);
        assert_eq!(teleport_energy(250.0), 25_000);
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
