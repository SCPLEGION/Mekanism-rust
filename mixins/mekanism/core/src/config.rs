//! Mekanism's server config (the parts the port reads), with the original's defaults and key names. `GearConfig`
//! lives in the world's `serverconfig/mekanism-gear.toml` (`[atomic_disassembler]`), `UsageConfig` in
//! `serverconfig/mekanism-usage.toml` (`[usage]`). Only the keys the port reads are written.

/// `GearConfig`'s Atomic Disassembler section.
#[derive(Clone, Debug, PartialEq)]
pub struct GearConfig {
    /// `energyUsage`: FE per point of efficiency a block costs.
    pub disassembler_energy_usage: u64,
    /// `slowMode`.
    pub disassembler_slow_mode: bool,
    /// `fastMode`.
    pub disassembler_fast_mode: bool,
    /// `veinMining`: off by default.
    pub disassembler_vein_mining: bool,
    /// `miningCount`: how many blocks a vein takes.
    pub disassembler_mining_count: usize,
    /// Each energy item's (capacity, charge rate) by its config section (`maxEnergy`/`baseEnergyCapacity`, `chargeRate`).
    pub energy: std::collections::BTreeMap<&'static str, (u64, u64)>,
    /// The other whole-number keys by `section.key` (`GEAR_INTS`).
    pub ints: std::collections::BTreeMap<String, u64>,
    /// The damage reduction ratios by `section.key` (`GEAR_RATIOS`), each 0 to 1.
    pub ratios: std::collections::BTreeMap<String, f64>,
    /// The switches by `section.key` (`GEAR_FLAGS`).
    pub flags: std::collections::BTreeMap<String, bool>,
    /// The decimals by `section.key` (`GEAR_DOUBLES`).
    pub doubles: std::collections::BTreeMap<String, f64>,
}

/// The gear switches: (section, key, default).
pub const GEAR_FLAGS: [(&str, &str, bool); 2] = [("mekatool", "extendedMining", true), ("mekasuit", "gravitationalVibrations", true)];

/// The gear decimals: (section, key, default, min, max).
pub const GEAR_DOUBLES: [(&str, &str, f64, f64, f64); 2] = [("mekatool", "baseEfficiency", 4.0, 0.1, 100.0), ("mekatool", "attackSpeed", -2.4, -4.0, 100.0)];

/// A gear switch of the active config (`"mekatool.extendedMining"`), else `default`.
pub fn gear_flag(full_key: &str, default: bool) -> bool {
    ACTIVE.read().ok().and_then(|a| a.as_ref().and_then(|c| c.flags.get(full_key).copied())).unwrap_or(default)
}

/// A gear decimal of the active config (`"mekatool.baseEfficiency"`), else `default`.
pub fn gear_double(full_key: &str, default: f64) -> f64 {
    ACTIVE.read().ok().and_then(|a| a.as_ref().and_then(|c| c.doubles.get(full_key).copied())).unwrap_or(default)
}

/// The damage reduction ratios (`defineInRange(key, 1.0, 0, 1)`): (section, key).
pub const GEAR_RATIOS: [(&str, &str); 4] = [
    ("free_runner", "fallDamageReductionRatio"),
    ("mekasuit.damage_absorption", "fallDamageReductionRatio"),
    ("mekasuit.damage_absorption", "magicDamageReductionRatio"),
    ("mekasuit.damage_absorption", "unspecifiedDamageReductionRatio"),
];

/// A damage reduction ratio of the active config (`"mekasuit.damage_absorption.magicDamageReductionRatio"`), else 1.
pub fn gear_ratio(full_key: &str) -> f64 {
    ACTIVE
        .read()
        .ok()
        .and_then(|a| a.as_ref().and_then(|c| c.ratios.get(full_key).copied()))
        .unwrap_or(1.0)
}

/// The other whole-number gear keys: (section, key, default), `defineInRange(key, default, 0, Integer.MAX_VALUE)`
/// unless the range says otherwise. The Meka-Tool's and MekaSuit's usages sit in their `energy_usage` section.
pub const GEAR_INTS: [(&str, &str, u64); 38] = [
    ("atomic_disassembler", "energyUsageWeapon", 2_000),
    ("atomic_disassembler", "minDamage", 4),
    ("atomic_disassembler", "maxDamage", 20),
    ("mekatool", "baseDamage", 4),
    ("electric_bow", "energyUsage", 120),
    ("electric_bow", "energyUsageFlame", 1_200),
    ("free_runner", "fallEnergyCost", 50),
    ("seismic_reader", "energyUsage", 250),
    ("mekatool", "maxTeleportReach", 100),
    ("jetpack", "capacity", 24_000),
    ("canteen", "maxStorage", 64_000),
    ("scuba_tank", "capacity", 24_000),
    ("flamethrower", "capacity", 24_000),
    ("mekasuit", "jetpackMaxStorage", 24_000),
    ("mekasuit", "nutritionalMaxStorage", 128_000),
    ("mekatool.energy_usage", "base", 10),
    ("mekatool.energy_usage", "silk", 100),
    ("mekatool.energy_usage", "weapon", 2_000),
    ("mekatool.energy_usage", "hoe", 10),
    ("mekatool.energy_usage", "shovel", 10),
    ("mekatool.energy_usage", "axe", 10),
    ("mekatool.energy_usage", "shearEntity", 10),
    ("mekatool.energy_usage", "shearTrim", 10),
    ("mekatool.energy_usage", "teleport", 1_000),
    ("mekasuit", "inventoryChargeRate", 10_000),
    ("mekasuit", "solarRechargingRate", 500),
    ("mekasuit.energy_usage", "damage", 100_000),
    ("mekasuit.energy_usage", "magicReduce", 1_000),
    ("mekasuit.energy_usage", "fall", 50),
    ("mekasuit.energy_usage", "jump", 1_000),
    ("mekasuit.energy_usage", "elytra", 32_000),
    ("mekasuit.energy_usage", "energyUsagePotionTick", 40_000),
    ("mekasuit.energy_usage", "sprintBoost", 100),
    ("mekasuit.energy_usage", "visionEnhancement", 500),
    ("mekasuit.energy_usage", "nutritionalInjection", 20_000),
    ("mekasuit.energy_usage", "gravitationalModulation", 1_000),
    ("mekasuit.energy_usage", "itemAttraction", 250),
    ("mekasuit.energy_usage", "hydrostaticRepulsion", 500),
];

fn int_key(section: &str, key: &str) -> String {
    format!("{section}.{key}")
}

/// A whole-number gear key of the active config (`"mekatool.energy_usage.base"`), else `default`.
pub fn gear_int(full_key: &str, default: u64) -> u64 {
    ACTIVE
        .read()
        .ok()
        .and_then(|a| a.as_ref().and_then(|c| c.ints.get(full_key).copied()))
        .unwrap_or(default)
}

/// The gear sections that hold an item's energy: (section, key of its capacity, capacity, charge rate), all
/// `GearConfig` defaults.
pub const ENERGY_SECTIONS: [(&str, &str, u64, u64); 8] = [
    ("energy_tablet", "maxEnergy", 1_000_000, 5_000),
    ("atomic_disassembler", "maxEnergy", 1_000_000, 5_000),
    ("electric_bow", "maxEnergy", 120_000, 600),
    ("free_runner", "maxEnergy", 64_000, 320),
    ("seismic_reader", "maxEnergy", 12_000, 60),
    ("portable_teleporter", "maxEnergy", 1_000_000, 5_000),
    ("mekatool", "baseEnergyCapacity", 16_000_000, 100_000),
    ("mekasuit", "baseEnergyCapacity", 16_000_000, 100_000),
];

/// The section of the item whose energy a config sets.
pub fn energy_section(id: &str) -> Option<&'static str> {
    Some(match id {
        "mekanism:energy_tablet" => "energy_tablet",
        "mekanism:atomic_disassembler" => "atomic_disassembler",
        "mekanism:electric_bow" => "electric_bow",
        "mekanism:free_runners" | "mekanism:free_runners_armored" => "free_runner",
        crate::seismic::READER => "seismic_reader",
        "mekanism:portable_teleporter" => "portable_teleporter",
        "mekanism:meka_tool" => "mekatool",
        "mekanism:mekasuit_helmet" | "mekanism:mekasuit_bodyarmor" | "mekanism:mekasuit_pants" | "mekanism:mekasuit_boots" => "mekasuit",
        _ => return None,
    })
}

static ACTIVE: std::sync::RwLock<Option<GearConfig>> = std::sync::RwLock::new(None);

/// Makes `config` the one the item energies read.
pub fn set_active(config: &GearConfig) {
    if let Ok(mut a) = ACTIVE.write() {
        *a = Some(config.clone());
    }
}

/// The (capacity, charge rate) the active config gives item `id`, if it is a configurable one.
pub fn energy_override(id: &str) -> Option<(u64, u64)> {
    let section = energy_section(id)?;
    ACTIVE.read().ok()?.as_ref()?.energy.get(section).copied()
}

impl Default for GearConfig {
    fn default() -> Self {
        Self {
            disassembler_energy_usage: 10,
            disassembler_slow_mode: true,
            disassembler_fast_mode: true,
            disassembler_vein_mining: false,
            disassembler_mining_count: 128,
            energy: ENERGY_SECTIONS.iter().map(|(s, _, cap, rate)| (*s, (*cap, *rate))).collect(),
            ratios: GEAR_RATIOS.iter().map(|(s, k)| (int_key(s, k), 1.0)).collect(),
            ints: GEAR_INTS.iter().map(|(s, k, d)| (int_key(s, k), *d)).collect(),
            flags: GEAR_FLAGS.iter().map(|(s, k, d)| (int_key(s, k), *d)).collect(),
            doubles: GEAR_DOUBLES.iter().map(|(s, k, d, ..)| (int_key(s, k), *d)).collect(),
        }
    }
}

const SECTION: &str = "atomic_disassembler";

impl GearConfig {
    /// Reads the file's text; a missing or invalid key keeps its default, a number outside its range is clamped
    /// to it (`defineInRange` would reset it; the default is the safer reading of a bad value).
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if let Some((name, cap_key, _, _)) = ENERGY_SECTIONS.iter().find(|(n, ..)| *n == section) {
                let n = value.replace('_', "").parse::<u64>().ok();
                if let (Some(n), Some(e)) = (n, config.energy.get_mut(name)) {
                    if key == *cap_key && n > 0 {
                        e.0 = n;
                    } else if key == "chargeRate" && n <= i32::MAX as u64 {
                        e.1 = n;
                    }
                }
            }
            if let Some(r) = value.parse::<f64>().ok().filter(|r| (0.0..=1.0).contains(r)) {
                if let Some(v) = config.ratios.get_mut(int_key(&section, key).as_str()) {
                    *v = r;
                }
            }
            if let Some(b) = value.parse::<bool>().ok() {
                if let Some(v) = config.flags.get_mut(int_key(&section, key).as_str()) {
                    *v = b;
                }
            }
            if let Some(d) = value.parse::<f64>().ok() {
                if let Some((.., min, max)) = GEAR_DOUBLES.iter().find(|(s, k, ..)| *s == section && *k == key)
                    && (*min..=*max).contains(&d)
                    && let Some(v) = config.doubles.get_mut(int_key(&section, key).as_str())
                {
                    *v = d;
                }
            }
            if let Some(n) = value.replace('_', "").parse::<u64>().ok().filter(|n| *n <= i32::MAX as u64) {
                let full = int_key(&section, key);
                if let Some(v) = config.ints.get_mut(full.as_str()) {
                    *v = n;
                }
            }
            if section != SECTION {
                continue;
            }
            let flag = value.parse::<bool>().ok();
            let number = value.replace('_', "").parse::<u64>().ok();
            match key {
                "energyUsage" => {
                    if let Some(n) = number.filter(|n| *n <= i32::MAX as u64) {
                        config.disassembler_energy_usage = n;
                    }
                }
                "slowMode" => config.disassembler_slow_mode = flag.unwrap_or(config.disassembler_slow_mode),
                "fastMode" => config.disassembler_fast_mode = flag.unwrap_or(config.disassembler_fast_mode),
                "veinMining" => config.disassembler_vein_mining = flag.unwrap_or(config.disassembler_vein_mining),
                "miningCount" => {
                    if let Some(n) = number.filter(|n| (2..=1_000_000).contains(n)) {
                        config.disassembler_mining_count = n as usize;
                    }
                }
                _ => {}
            }
        }
        config
    }

    /// The file written when there is none.
    pub fn to_toml(&self) -> String {
        let mut out = format!(
            "# Mekanism gear config (server config: this world only).\n\
             [{SECTION}]\n\
             # Energy (FE) one point of efficiency costs. Range 0 ~ 2147483647\n\
             energyUsage = {}\n\
             # Allow the slow mode\n\
             slowMode = {}\n\
             # Allow the fast mode\n\
             fastMode = {}\n\
             # Allow vein mining mode\n\
             veinMining = {}\n\
             # Blocks a vein takes. Range 2 ~ 1000000\n\
             miningCount = {}\n\
             maxEnergy = {}\n\
             chargeRate = {}\n",
            self.disassembler_energy_usage,
            self.disassembler_slow_mode,
            self.disassembler_fast_mode,
            self.disassembler_vein_mining,
            self.disassembler_mining_count,
            self.energy[SECTION].0,
            self.energy[SECTION].1
        );
        let mut last = "";
        for (sec, key, _) in GEAR_INTS {
            if sec != last {
                out.push_str(&format!("[{sec}]\n"));
                last = sec;
            }
            out.push_str(&format!("{key} = {}\n", self.ints[int_key(sec, key).as_str()]));
        }
        for (sec, key) in GEAR_RATIOS {
            out.push_str(&format!("[{sec}]\n{key} = {:?}\n", self.ratios[int_key(sec, key).as_str()]));
        }
        for (sec, key, _) in GEAR_FLAGS {
            out.push_str(&format!("[{sec}]\n{key} = {}\n", self.flags[int_key(sec, key).as_str()]));
        }
        for (sec, key, ..) in GEAR_DOUBLES {
            out.push_str(&format!("[{sec}]\n{key} = {:?}\n", self.doubles[int_key(sec, key).as_str()]));
        }
        for (name, cap_key, _, _) in ENERGY_SECTIONS {
            if name == SECTION {
                continue;
            }
            let (cap, rate) = self.energy[name];
            out.push_str(&format!("[{name}]\n{cap_key} = {cap}\nchargeRate = {rate}\n"));
        }
        out
    }
}

/// `UsageConfig`: FE a tick a machine uses, for the machines whose usage the port reads from here, with the
/// original's defaults (`defineInRange(key, default, 0, Integer.MAX_VALUE)`).
pub const USAGE_DEFAULTS: [(&str, u64); 34] = [
    ("enrichmentChamber", 5),
    ("osmiumCompressor", 15),
    ("combiner", 5),
    ("crusher", 8),
    ("metallurgicInfuser", 5),
    ("purificationChamber", 10),
    ("energizedSmelter", 5),
    ("precisionSawmill", 5),
    ("chemicalInjectionChamber", 15),
    ("electricPump", 2),
    ("fluidicPlenisher", 2),
    ("chemicalWasher", 5),
    ("rotaryCondensentrator", 2),
    ("paintingMachine", 5),
    ("antiprotonicNucleosynthesizer", 15),
    ("nutritionalLiquifier", 8),
    ("chemicalDissolutionChamber", 16),
    ("chemicalCrystallizer", 15),
    ("isotopicCentrifuge", 8),
    ("modificationStation", 10),
    ("pigmentMixer", 5),
    ("pigmentExtractor", 5),
    ("chemicalOxidizer", 15),
    ("chemicalInfuser", 15),
    ("seismicVibrator", 5),
    ("laser", 20),
    ("digitalMiner", 40),
    ("formulaicAssemblicator", 5),
    ("dimensionalStabilizer", 15),
    ("chargePad", 1_024),
    ("pressurizedReactionBase", 2),
    ("teleporterBase", 100),
    ("teleporterDistance", 2),
    ("teleporterDimensionPenalty", 1_000),
];

#[derive(Clone, Debug, PartialEq)]
pub struct UsageConfig {
    values: std::collections::BTreeMap<&'static str, u64>,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self { values: USAGE_DEFAULTS.into_iter().collect() }
    }
}

impl UsageConfig {
    /// A machine's usage; an unknown key gives `default`.
    pub fn get(&self, key: &str, default: u64) -> u64 {
        self.values.get(key).copied().unwrap_or(default)
    }

    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if section != "usage" {
                continue;
            }
            if let (Some((name, _)), Some(n)) = (
                USAGE_DEFAULTS.iter().find(|(k, _)| *k == key.trim()),
                value.trim().replace('_', "").parse::<u64>().ok().filter(|n| *n <= i32::MAX as u64),
            ) {
                config.values.insert(name, n);
            }
        }
        config
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::from("# Mekanism usage config (server config: this world only). Range 0 ~ 2147483647\n[usage]\n");
        for (key, _) in USAGE_DEFAULTS {
            out.push_str(&format!("{key} = {}\n", self.values[key]));
        }
        out
    }
}

/// The `UsageConfig` key of the machine a recipe kind belongs to (a factory uses its machine's).
pub fn usage_key(recipe: &str) -> Option<&'static str> {
    Some(match recipe {
        "enriching" => "enrichmentChamber",
        "crushing" => "crusher",
        "smelting" => "energizedSmelter",
        "sawing" => "precisionSawmill",
        "combining" => "combiner",
        "compressing" => "osmiumCompressor",
        "injecting" => "chemicalInjectionChamber",
        "purifying" => "purificationChamber",
        "infusing" => "metallurgicInfuser",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn item_energy_comes_from_the_config() {
        let c = GearConfig::parse("[energy_tablet]\nmaxEnergy = 2_000_000\nchargeRate = 10\n[mekatool]\nbaseEnergyCapacity = 5");
        assert_eq!(c.energy["energy_tablet"], (2_000_000, 10));
        assert_eq!(c.energy["mekatool"], (5, 100_000));
        assert_eq!(energy_section("mekanism:free_runners_armored"), Some("free_runner"));
    }

    #[test]
    fn usage_defaults_round_trip_and_override() {
        let d = super::UsageConfig::default();
        assert_eq!(d.get("crusher", 0), 8);
        assert_eq!(super::UsageConfig::parse(&d.to_toml()), d);
        assert_eq!(super::UsageConfig::parse("[usage]\ncrusher = 20\nbogus = 3").get("crusher", 0), 20);
    }

    use super::*;

    #[test]
    fn defaults_match_gear_config_and_round_trip() {
        let d = GearConfig::default();
        assert!(!d.disassembler_vein_mining);
        assert_eq!((d.disassembler_energy_usage, d.disassembler_mining_count), (10, 128));
        assert_eq!(GearConfig::parse(&d.to_toml()), d);
    }

    #[test]
    fn gear_ints_default_parse_and_round_trip() {
        let d = GearConfig::default();
        assert_eq!(d.ints["mekatool.energy_usage.teleport"], 1_000);
        let c = GearConfig::parse("[mekatool.energy_usage]\nsilk = 7\n[seismic_reader]\nenergyUsage = 9\n");
        assert_eq!((c.ints["mekatool.energy_usage.silk"], c.ints["seismic_reader.energyUsage"]), (7, 9));
        assert_eq!(GearConfig::parse(&c.to_toml()), c);
    }

    #[test]
    fn reads_values_and_ignores_bad_ones() {
        let c = GearConfig::parse("[atomic_disassembler]\nveinMining = true # on\nminingCount = 1\nslowMode = maybe\nenergyUsage = 20\n[other]\nfastMode = false");
        assert!(c.disassembler_vein_mining);
        assert_eq!(c.disassembler_mining_count, 128);
        assert!(c.disassembler_slow_mode && c.disassembler_fast_mode);
        assert_eq!(c.disassembler_energy_usage, 20);
    }
}
