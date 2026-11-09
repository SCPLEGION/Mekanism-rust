//! The rest of Mekanism's server configs, each in the world's `serverconfig/mekanism-<name>.toml` with the original's
//! keys, sections, defaults and ranges: `StorageConfig` (`machine-storage`), `TierConfig` (`tiers`), `GeneralConfig`
//! (`general`) and `WorldConfig` (`world`). A value outside its range goes back to its default, as NeoForge's
//! `defineInRange` does.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Double(f64),
    /// An enum, by its constant's name.
    Enum(String),
    List(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Range {
    None,
    Int(i64, i64),
    Double(f64, f64),
    /// The allowed constant names.
    Choice(&'static [&'static str]),
}

#[derive(Clone, Debug)]
pub struct Entry {
    /// Dotted table path; empty for a key at the top.
    pub section: String,
    pub key: String,
    pub default: Value,
    pub range: Range,
}

fn entry(section: &str, key: &str, default: Value, range: Range) -> Entry {
    Entry { section: section.to_owned(), key: key.to_owned(), default, range }
}

fn flag(section: &str, key: &str, default: bool) -> Entry {
    entry(section, key, Value::Bool(default), Range::None)
}

fn int(section: &str, key: &str, default: i64, min: i64, max: i64) -> Entry {
    entry(section, key, Value::Int(default), Range::Int(min, max))
}

fn dbl(section: &str, key: &str, default: f64, min: f64, max: f64) -> Entry {
    entry(section, key, Value::Double(default), Range::Double(min, max))
}

const INT_MAX: i64 = i32::MAX as i64;
const LONG_MAX: i64 = i64::MAX;
const BUCKET: i64 = 1000;

/// One config file's entries.
#[derive(Clone, Debug)]
pub struct Spec {
    /// `getFileName()`; the file is `mekanism-<name>.toml`.
    pub name: &'static str,
    pub title: &'static str,
    pub entries: Vec<Entry>,
}

/// `StorageConfig`.
pub fn storage() -> Spec {
    Spec {
        name: "machine-storage",
        title: "Storage Config",
        entries: vec![
            int("", "laserAmplifier", 5_000_000, 1, LONG_MAX),
            int("", "laserTractorBeam", 5_000_000, 1, LONG_MAX),
            int("", "teleporter", 1_000, 1, LONG_MAX),
            int("", "spsPort", 1_000_000_000, 1, LONG_MAX),
        ],
    }
}

const TIERS: [&str; 5] = ["basic", "advanced", "elite", "ultimate", "creative"];

/// `TierConfig`.
pub fn tiers() -> Spec {
    let mut e = Vec::new();
    // Capacity and output per tier, the creative tier at the maxima.
    let capacity_output = |e: &mut Vec<Entry>, section: &str, rows: [(i64, i64); 5]| {
        for (tier, (capacity, output)) in TIERS.iter().zip(rows) {
            e.push(int(section, &format!("{tier}Capacity"), capacity, 1, LONG_MAX));
            e.push(int(section, &format!("{tier}Output"), output, 1, INT_MAX));
        }
    };
    capacity_output(
        &mut e,
        "energy_cubes",
        [(4_000_000, 4_000), (16_000_000, 16_000), (64_000_000, 64_000), (256_000_000, 256_000), (LONG_MAX, INT_MAX)],
    );
    capacity_output(
        &mut e,
        "fluid_tanks",
        [
            (32 * BUCKET, BUCKET),
            (64 * BUCKET, 4 * BUCKET),
            (128 * BUCKET, 16 * BUCKET),
            (256 * BUCKET, 64 * BUCKET),
            (LONG_MAX, INT_MAX),
        ],
    );
    capacity_output(
        &mut e,
        "chemical_tanks",
        [
            (64 * BUCKET, BUCKET),
            (256 * BUCKET, 16 * BUCKET),
            (1_024 * BUCKET, 128 * BUCKET),
            (8_192 * BUCKET, 512 * BUCKET),
            (LONG_MAX, INT_MAX),
        ],
    );
    for (tier, capacity) in TIERS.iter().zip([4_096, 8_192, 32_768, 262_144, LONG_MAX]) {
        e.push(int("bins", &format!("{tier}Capacity"), capacity, 1, LONG_MAX));
    }
    for (tier, capacity) in TIERS.iter().zip([8_000_000_000, 64_000_000_000, 512_000_000_000, 4_000_000_000_000]) {
        e.push(int("induction", &format!("{tier}Capacity"), capacity, 1, LONG_MAX));
    }
    for (tier, output) in TIERS.iter().zip([256_000, 2_048_000, 16_384_000, 131_072_000]) {
        e.push(int("induction", &format!("{tier}Output"), output, 1, LONG_MAX));
    }
    for (tier, capacity) in TIERS.iter().zip([8_000, 128_000, 1_024_000, 8_192_000]) {
        e.push(int("transmitters.energy", &format!("{tier}Capacity"), capacity, 1, LONG_MAX));
    }
    for (section, rows) in [
        ("transmitters.fluid", [(2 * BUCKET, BUCKET / 4), (8 * BUCKET, BUCKET), (32 * BUCKET, 8 * BUCKET), (128 * BUCKET, 32 * BUCKET)]),
        ("transmitters.chemical", [(4 * BUCKET, 750), (16 * BUCKET, 2 * BUCKET), (256 * BUCKET, 64 * BUCKET), (1_024 * BUCKET, 256 * BUCKET)]),
    ] {
        for (tier, (capacity, pull)) in TIERS.iter().zip(rows) {
            e.push(int(section, &format!("{tier}Capacity"), capacity, 1, LONG_MAX));
            e.push(int(section, &format!("{tier}PullAmount"), pull, 1, INT_MAX));
        }
    }
    for (tier, (pull, speed)) in TIERS.iter().zip([(1, 5), (16, 10), (32, 20), (64, 50)]) {
        e.push(int("transmitters.items", &format!("{tier}PullAmount"), pull, 1, INT_MAX));
        e.push(int("transmitters.items", &format!("{tier}Speed"), speed, 1, INT_MAX));
    }
    for (tier, insulation) in TIERS.iter().zip([10.0, 400.0, 8_000.0, 100_000.0]) {
        e.push(dbl("transmitters.heat", &format!("{tier}InverseConduction"), 5.0, 1.0, f64::MAX));
        e.push(dbl("transmitters.heat", &format!("{tier}HeatCapacity"), 1.0, 1.0, f64::MAX));
        e.push(dbl("transmitters.heat", &format!("{tier}Insulation"), insulation, 0.0, f64::MAX));
    }
    Spec { name: "tiers", title: "Tier Config", entries: e }
}

/// `GeneralConfig`.
pub fn general() -> Spec {
    let max_volume = 18 * 18 * 18;
    let e = vec![
        flag("", "logPackets", false),
        flag("", "allowChunkloading", true),
        int("", "blockDeactivationDelay", 60, 0, 1_200),
        flag("", "aestheticWorldDamage", true),
        flag("", "transmitterAlloyUpgrade", true),
        int("", "maxUpgradeMultiplier", 10, 1, INT_MAX),
        int("", "maxSolarNeutronActivatorRate", 64, 1, 1_024),
        dbl("heater", "heatPerFuelTick", 400.0, 0.1, 4_000_000.0),
        int("heater", "fuelwoodTickMultiplier", 1, 1, 1_000),
        dbl("heater", "resistiveEfficiency", 0.6, 0.0, 1.0),
        flag("cardboard_box", "strictUnboxing", false),
        entry("cardboard_box", "modBlacklist", Value::List(vec![]), Range::None),
        int("item_fill_rate", "fluid", 1_024, 1, INT_MAX),
        int("item_fill_rate", "chemical", 1_024, 1, INT_MAX),
        int("dynamic_tank", "mBPerTank", 16_000 * BUCKET, 1, LONG_MAX / max_volume),
        int("auto_eject", "fluid", 1_024, 1, INT_MAX),
        int("auto_eject", "chemical", 1_024, 1, INT_MAX),
        dbl("auto_eject", "dumpExcessKeepRatio", 0.85, 0.001, 1.0),
        flag("prefilled", "fluidTanks", true),
        flag("prefilled", "chemicalTanks", true),
        int("energy_conversion", "maxEnergyPerSteam", 10, 0, LONG_MAX),
        flag("radiation", "enabled", true),
        int("radiation", "chunkCheckRadius", 5, 1, 100),
        dbl("radiation", "sourceDecayRate", 0.9995, 0.0, 1.0),
        dbl("radiation", "targetDecayRate", 0.9995, 0.0, 1.0),
        dbl("radiation", "negativeEffectsMinSeverity", 0.1, 0.0, 1.0),
        int("radiation", "wasteBarrelCapacity", 512 * BUCKET, 1, LONG_MAX),
        int("radiation", "wasteBarrelProcessTicks", 20, 1, INT_MAX),
        int("radiation", "wasteBarrelDecayAmount", 1, 0, INT_MAX),
        int("digital_miner", "silkMultiplier", 12, 1, INT_MAX),
        int("digital_miner", "maxRadius", 32, 1, INT_MAX),
        int("digital_miner", "ticksPerMine", 80, 1, INT_MAX),
        flag("digital_miner", "easyMinerFilters", false),
        int("laser", "range", 64, 1, 1_024),
        int("laser", "energyPerHardness", 100_000, 0, INT_MAX),
        int("laser", "energyPerDamage", 2_500, 1, INT_MAX),
        entry(
            "oredictionificator",
            "validItemFilters",
            Value::List(["ingots/", "ores/", "dusts/", "nuggets/", "storage_blocks/", "raw_materials/"].map(|p| format!("c:{p}")).to_vec()),
            Range::None,
        ),
        int("pump", "range", 80, 1, 512),
        flag("pump", "pumpInfiniteFluidSources", false),
        int("pump", "heavyWaterAmount", BUCKET / 100, 1, BUCKET),
        int("pump", "maxPlenisherNodes", 4_000, 1, 1_000_000),
        int("quantum_entangloporter", "energyBuffer", 256_000_000, 1, LONG_MAX),
        int("quantum_entangloporter", "fluidBuffer", 256 * BUCKET, 1, LONG_MAX),
        int("quantum_entangloporter", "chemicalBuffer", 8_192 * BUCKET, 1, LONG_MAX),
        flag("security", "allowProtection", true),
        flag("security", "opsBypassRestrictions", false),
        dbl("nutritional_paste", "saturation", 0.8, 0.0, 100.0),
        int("nutritional_paste", "mbPerFood", 50, 1, INT_MAX / 20),
        int("boiler", "waterPerTank", 16 * BUCKET, 1, LONG_MAX / max_volume),
        int("boiler", "steamPerTank", 160 * BUCKET, 10, LONG_MAX / max_volume),
        int("boiler", "heatedCoolantPerTank", 256 * BUCKET, 1, LONG_MAX / max_volume),
        int("boiler", "cooledCoolantPerTank", 256 * BUCKET, 1, LONG_MAX / max_volume),
        dbl("boiler", "waterConductivity", 0.7, 0.01, 1.0),
        dbl("boiler", "superheatingHeatTransfer", 16_000_000.0, 0.1, 1_024_000_000.0),
        dbl("thermal_evaporation", "heatDissipation", 0.02, 0.001, 1_000.0),
        dbl("thermal_evaporation", "solarMultiplier", 0.2, 0.001, 1_000_000.0),
        dbl("thermal_evaporation", "tempMultiplier", 0.4, 0.001, 1_000_000.0),
        dbl("thermal_evaporation", "heatCapacity", 100.0, 1.0, 1_000_000.0),
        int("thermal_evaporation", "fluidPerTank", 64 * BUCKET, 1, LONG_MAX / (18 * 4)),
        int("thermal_evaporation", "outputTankCapacity", 10 * BUCKET, 1, LONG_MAX),
        int("sps", "inputPerAntimatter", BUCKET, 1, INT_MAX),
        int("sps", "outputTankCapacity", BUCKET, 1, LONG_MAX),
        int("sps", "energyPerInput", 1_000_000, 0, LONG_MAX),
    ];
    Spec { name: "general", title: "General Config", entries: e }
}

const SHAPES: &[&str] = &["TRAPEZOID", "UNIFORM"];
const ANCHORS: &[&str] = &["ABSOLUTE", "ABOVE_BOTTOM", "BELOW_TOP"];

/// An ore vein of `OreType`: name, per chunk, discard chance, size, shape, minimum anchor, maximum anchor, plateau.
type Vein = (&'static str, i64, f64, i64, &'static str, (&'static str, i64), (&'static str, i64), i64);

const ORES: [(&str, &[Vein]); 5] = [
    (
        "tin",
        &[
            ("small", 14, 0.0, 4, "TRAPEZOID", ("ABSOLUTE", -20), ("ABSOLUTE", 94), 0),
            ("large", 12, 0.0, 9, "TRAPEZOID", ("ABSOLUTE", -32), ("ABSOLUTE", 72), 0),
        ],
    ),
    (
        "osmium",
        &[
            ("upper", 65, 0.0, 7, "TRAPEZOID", ("ABSOLUTE", 72), ("BELOW_TOP", -24), 8),
            ("middle", 6, 0.0, 9, "TRAPEZOID", ("ABSOLUTE", -32), ("ABSOLUTE", 56), 0),
            ("small", 8, 0.0, 4, "UNIFORM", ("ABOVE_BOTTOM", 0), ("ABSOLUTE", 64), 0),
        ],
    ),
    (
        "uranium",
        &[
            ("small", 4, 0.0, 4, "TRAPEZOID", ("ABOVE_BOTTOM", 0), ("ABSOLUTE", 8), 0),
            ("buried", 7, 0.75, 9, "TRAPEZOID", ("ABOVE_BOTTOM", -24), ("ABOVE_BOTTOM", 56), 16),
        ],
    ),
    (
        "fluorite",
        &[
            ("normal", 5, 0.0, 5, "UNIFORM", ("ABOVE_BOTTOM", 0), ("ABSOLUTE", 23), 0),
            ("buried", 3, 1.0, 13, "TRAPEZOID", ("ABOVE_BOTTOM", 0), ("ABSOLUTE", 4), 0),
        ],
    ),
    ("lead", &[("normal", 8, 0.25, 9, "TRAPEZOID", ("ABOVE_BOTTOM", -24), ("ABSOLUTE", 64), 0)]),
];

/// `WorldConfig`.
pub fn world() -> Spec {
    let mut e = vec![flag("", "enableRegeneration", false), int("", "userWorldGenVersion", 0, 0, INT_MAX)];
    for (ore, veins) in ORES {
        e.push(flag(ore, "shouldGenerate", true));
        for (name, per_chunk, discard, size, shape, min, max, plateau) in veins.iter().copied() {
            let s = format!("{ore}.{name}");
            e.push(flag(&s, "shouldGenerate", true));
            e.push(int(&s, "perChunk", per_chunk, 1, 256));
            e.push(int(&s, "maxVeinSize", size, 1, 64));
            e.push(dbl(&s, "discardChanceOnAirExposure", discard, 0.0, 1.0));
            e.push(entry(&s, "shape", Value::Enum(shape.to_owned()), Range::Choice(SHAPES)));
            e.push(int(&s, "plateau", plateau, 0, INT_MAX));
            for (path, (kind, value)) in [("minInclusive", min), ("maxInclusive", max)] {
                let section = format!("{s}.{path}");
                e.push(entry(&section, "type", Value::Enum(kind.to_owned()), Range::Choice(ANCHORS)));
                e.push(int(&section, "value", value, i64::from(i32::MIN), INT_MAX));
            }
        }
    }
    e.push(flag("salt", "shouldGenerate", true));
    e.push(int("salt", "perChunk", 2, 1, 256));
    e.push(int("salt", "minRadius", 2, 1, 4));
    e.push(int("salt", "maxRadius", 3, 1, 4));
    e.push(int("salt", "halfHeight", 1, 0, 2_031));
    Spec { name: "world", title: "World Config", entries: e }
}

/// Every file of this module.
pub fn all() -> [Spec; 4] {
    [storage(), tiers(), general(), world()]
}

/// A config file's values.
#[derive(Clone, Debug)]
pub struct Config {
    spec: Spec,
    values: BTreeMap<(String, String), Value>,
}

impl Config {
    pub fn defaults(spec: Spec) -> Self {
        let values = spec.entries.iter().map(|e| ((e.section.clone(), e.key.clone()), e.default.clone())).collect();
        Self { spec, values }
    }

    pub fn file_name(&self) -> String {
        format!("mekanism-{}.toml", self.spec.name)
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&Value> {
        self.values.get(&(section.to_owned(), key.to_owned()))
    }

    pub fn int(&self, section: &str, key: &str) -> i64 {
        match self.get(section, key) {
            Some(Value::Int(v)) => *v,
            _ => 0,
        }
    }

    pub fn double(&self, section: &str, key: &str) -> f64 {
        match self.get(section, key) {
            Some(Value::Double(v)) => *v,
            _ => 0.0,
        }
    }

    pub fn flag(&self, section: &str, key: &str) -> bool {
        matches!(self.get(section, key), Some(Value::Bool(true)))
    }

    /// Reads a file's text; a missing, invalid or out-of-range key keeps its default.
    pub fn parse(spec: Spec, text: &str) -> Self {
        let mut config = Self::defaults(spec);
        let mut section = String::new();
        for line in text.lines() {
            let line = strip_comment(line).trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some(entry) = config.spec.entries.iter().find(|e| e.section == section && e.key == key.trim()) else {
                continue;
            };
            if let Some(v) = read_value(value.trim(), entry) {
                config.values.insert((section.clone(), key.trim().to_owned()), v);
            }
        }
        // `maxRadius` of the salt may not undercut `minRadius` (its validator).
        if config.spec.name == "world" && config.int("salt", "maxRadius") < config.int("salt", "minRadius") {
            let default = config.spec.entries.iter().find(|e| e.section == "salt" && e.key == "maxRadius").map(|e| e.default.clone());
            if let Some(default) = default {
                config.values.insert(("salt".into(), "maxRadius".into()), default);
            }
        }
        config
    }

    /// The text of the file, with each key's default and range as comments.
    pub fn to_toml(&self) -> String {
        let mut out = format!("# Mekanism {} (server config: this world only).\n", self.spec.title);
        let mut written: Vec<&str> = Vec::new();
        // Keys at the top first, then each table in order of first appearance.
        let mut sections: Vec<&str> = Vec::new();
        for e in &self.spec.entries {
            if !sections.contains(&e.section.as_str()) {
                sections.push(&e.section);
            }
        }
        sections.sort_by_key(|s| !s.is_empty());
        for section in sections {
            if !section.is_empty() {
                out.push_str(&format!("\n[{section}]\n"));
            } else if !written.is_empty() {
                continue;
            }
            written.push(section);
            for e in self.spec.entries.iter().filter(|e| e.section == section) {
                match &e.range {
                    Range::Int(a, b) => out.push_str(&format!("# Range: {a} ~ {b}\n")),
                    Range::Double(a, b) => out.push_str(&format!("# Range: {a:?} ~ {b:?}\n")),
                    Range::Choice(options) => out.push_str(&format!("# Allowed Values: {}\n", options.join(", "))),
                    Range::None => {}
                }
                out.push_str(&format!("{} = {}\n", e.key, show(self.values.get(&(e.section.clone(), e.key.clone())).unwrap_or(&e.default))));
            }
        }
        out
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

fn show(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Double(d) => format!("{d:?}"),
        Value::Enum(s) => format!("\"{s}\""),
        Value::List(items) => format!("[{}]", items.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")),
    }
}

fn read_value(text: &str, entry: &Entry) -> Option<Value> {
    match &entry.default {
        Value::Bool(_) => text.parse().ok().map(Value::Bool),
        Value::Int(_) => {
            let n: i64 = text.replace('_', "").parse().ok()?;
            match entry.range {
                Range::Int(a, b) => (a..=b).contains(&n).then_some(Value::Int(n)),
                _ => Some(Value::Int(n)),
            }
        }
        Value::Double(_) => {
            let n: f64 = text.replace('_', "").parse().ok()?;
            match entry.range {
                Range::Double(a, b) => (a..=b).contains(&n).then_some(Value::Double(n)),
                _ => Some(Value::Double(n)),
            }
        }
        Value::Enum(_) => {
            let name = text.trim_matches('"');
            match entry.range {
                Range::Choice(options) => options.contains(&name).then(|| Value::Enum(name.to_owned())),
                _ => None,
            }
        }
        Value::List(_) => {
            let inner = text.strip_prefix('[')?.strip_suffix(']')?;
            let items: Vec<String> = inner.split(',').map(|s| s.trim().trim_matches('"').to_owned()).filter(|s| !s.is_empty()).collect();
            Some(Value::List(items))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_round_trips() {
        for spec in all() {
            let defaults = Config::defaults(spec.clone());
            let parsed = Config::parse(spec, &defaults.to_toml());
            assert_eq!(parsed.values, defaults.values);
        }
    }

    #[test]
    fn original_defaults() {
        let t = Config::defaults(tiers());
        assert_eq!(t.int("energy_cubes", "ultimateCapacity"), 256_000_000);
        assert_eq!(t.int("transmitters.chemical", "basicPullAmount"), 750);
        assert_eq!(t.double("transmitters.heat", "eliteInsulation"), 8_000.0);
        let g = Config::defaults(general());
        assert_eq!(g.int("dynamic_tank", "mBPerTank"), 16_000_000);
        assert_eq!(g.double("radiation", "sourceDecayRate"), 0.9995);
        assert!(g.flag("security", "allowProtection"));
        let w = Config::defaults(world());
        assert_eq!(w.int("osmium.upper", "perChunk"), 65);
        assert_eq!(Config::defaults(storage()).int("", "spsPort"), 1_000_000_000);
    }

    #[test]
    fn bad_values_keep_the_default_and_good_ones_apply() {
        let g = Config::parse(general(), "[laser]\nrange = 5000 # too far\nenergyPerDamage = 3_000\n[pump]\nrange = 100\n");
        assert_eq!(g.int("laser", "range"), 64);
        assert_eq!(g.int("laser", "energyPerDamage"), 3_000);
        assert_eq!(g.int("pump", "range"), 100);
        let w = Config::parse(world(), "[salt]\nminRadius = 4\nmaxRadius = 2\n[tin.small]\nshape = \"UNIFORM\"\n[tin.small.minInclusive]\ntype = \"NOPE\"\n");
        assert_eq!(w.int("salt", "maxRadius"), 3);
        assert_eq!(w.get("tin.small", "shape"), Some(&Value::Enum("UNIFORM".into())));
        assert_eq!(w.get("tin.small.minInclusive", "type"), Some(&Value::Enum("ABSOLUTE".into())));
    }
}
