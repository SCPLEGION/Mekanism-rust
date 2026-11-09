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
}

impl Default for GearConfig {
    fn default() -> Self {
        Self {
            disassembler_energy_usage: 10,
            disassembler_slow_mode: true,
            disassembler_fast_mode: true,
            disassembler_vein_mining: false,
            disassembler_mining_count: 128,
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
            if section != SECTION {
                continue;
            }
            let (key, value) = (key.trim(), value.trim());
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
        format!(
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
             miningCount = {}\n",
            self.disassembler_energy_usage,
            self.disassembler_slow_mode,
            self.disassembler_fast_mode,
            self.disassembler_vein_mining,
            self.disassembler_mining_count
        )
    }
}

/// `UsageConfig`: FE a tick a machine uses, for the machines whose usage the port reads from here, with the
/// original's defaults (`defineInRange(key, default, 0, Integer.MAX_VALUE)`).
pub const USAGE_DEFAULTS: [(&str, u64); 9] = [
    ("enrichmentChamber", 5),
    ("osmiumCompressor", 15),
    ("combiner", 5),
    ("crusher", 8),
    ("metallurgicInfuser", 5),
    ("purificationChamber", 10),
    ("energizedSmelter", 5),
    ("precisionSawmill", 5),
    ("chemicalInjectionChamber", 15),
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
    fn reads_values_and_ignores_bad_ones() {
        let c = GearConfig::parse("[atomic_disassembler]\nveinMining = true # on\nminingCount = 1\nslowMode = maybe\nenergyUsage = 20\n[other]\nfastMode = false");
        assert!(c.disassembler_vein_mining);
        assert_eq!(c.disassembler_mining_count, 128);
        assert!(c.disassembler_slow_mode && c.disassembler_fast_mode);
        assert_eq!(c.disassembler_energy_usage, 20);
    }
}
