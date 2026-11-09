//! Ore and salt generation from the world config (`mekanism-world.toml`): the placed and configured features
//! `ResizableOreFeature`, `DisableableFeaturePlacement`, `ConfigurableConstantInt`, `ConfigurableHeightProvider` and
//! `ResizableDiskReplaceFeature` build from it, written as the vanilla features Horizon's generator runs. Horizon's
//! `native_worldgen_entries` takes them over the carried defaults. Retrogen (`enableRegeneration`) has no equivalent.

use crate::config_files::{Config, ORES};

/// (bundle, id, JSON): a worldgen file for the generator.
pub type Entry = (String, String, String);

fn anchor(kind: &str, value: i64) -> String {
    let key = match kind {
        "ABOVE_BOTTOM" => "above_bottom",
        "BELOW_TOP" => "below_top",
        _ => "absolute",
    };
    format!("{{\"{key}\":{value}}}")
}

fn height(world: &Config, section: &str) -> String {
    let shape = world.string(section, "shape");
    let min = anchor(&world.string(&format!("{section}.minInclusive"), "type"), world.int(&format!("{section}.minInclusive"), "value"));
    let max = anchor(&world.string(&format!("{section}.maxInclusive"), "type"), world.int(&format!("{section}.maxInclusive"), "value"));
    let plateau = world.int(section, "plateau");
    if shape == "TRAPEZOID" {
        let plateau = if plateau > 0 { format!(",\"plateau\":{plateau}") } else { String::new() };
        format!("{{\"type\":\"minecraft:trapezoid\",\"min_inclusive\":{min},\"max_inclusive\":{max}{plateau}}}")
    } else {
        format!("{{\"type\":\"minecraft:uniform\",\"min_inclusive\":{min},\"max_inclusive\":{max}}}")
    }
}

/// Stone and deepslate variants of `ore`, replaced under the same rules as `OreType`'s targets.
fn targets(ore: &str) -> String {
    let rule = |state: String, lo: i32, hi: i32, tag: &str| {
        format!(
            "{{\"state\":{{\"Name\":\"{state}\"}},\"target\":{{\"predicate_type\":\"minecraft:any_of\",\"rules\":[\
             {{\"predicate_type\":\"minecraft:all_of\",\"rules\":[\
             {{\"predicate_type\":\"minecraft:tag_match\",\"tag\":\"minecraft:height_specific_ore_replaceables\"}},\
             {{\"max_inclusive\":{hi},\"min_inclusive\":{lo},\"predicate_type\":\"minecraft:height_match\"}}]}},\
             {{\"predicate_type\":\"minecraft:all_of\",\"rules\":[\
             {{\"predicate_type\":\"minecraft:not\",\"rule\":{{\"predicate_type\":\"minecraft:tag_match\",\"tag\":\"minecraft:height_specific_ore_replaceables\"}}}},\
             {{\"predicate_type\":\"minecraft:tag_match\",\"tag\":\"{tag}\"}}]}}]}}}}"
        )
    };
    format!(
        "[{},{}]",
        rule(format!("mekanism:{ore}_ore"), 0, 2031, "minecraft:stone_ore_replaceables"),
        rule(format!("mekanism:deepslate_{ore}_ore"), -2032, 8, "minecraft:deepslate_ore_replaceables")
    )
}

/// A placed feature that never places (a disabled ore: `DisableableFeaturePlacement`).
fn disabled(feature: &str) -> String {
    format!("{{\"feature\":\"{feature}\",\"placement\":[{{\"type\":\"minecraft:count\",\"count\":0}}]}}")
}

/// The features of every vein and of the salt disk.
pub fn entries(world: &Config) -> Vec<Entry> {
    let mut out = Vec::new();
    for (ore, veins) in ORES {
        for (name, ..) in veins {
            let section = format!("{ore}.{name}");
            let id = format!("mekanism:ore_{ore}_{name}");
            let on = world.flag(ore, "shouldGenerate") && world.flag(&section, "shouldGenerate");
            let feature = format!(
                "{{\"type\":\"minecraft:ore\",\"config\":{{\"targets\":{},\"size\":{},\"discard_chance_on_air_exposure\":{}}}}}",
                targets(ore),
                world.int(&section, "maxVeinSize"),
                world.double(&section, "discardChanceOnAirExposure")
            );
            let placed = if on {
                format!(
                    "{{\"feature\":\"{id}\",\"placement\":[{{\"type\":\"minecraft:count\",\"count\":{}}},{{\"type\":\"minecraft:in_square\"}},\
                     {{\"type\":\"minecraft:height_range\",\"height\":{}}},{{\"type\":\"minecraft:biome\"}}]}}",
                    world.int(&section, "perChunk"),
                    height(world, &section)
                )
            } else {
                disabled(&id)
            };
            out.push(("features".into(), id.clone(), feature));
            out.push(("placed_features".into(), id, placed));
        }
    }
    out.extend(salt(world));
    out
}

/// The salt disk: vanilla's disk with the config's radius, depth and count. Vanilla's codec allows a half height of 0 to 4,
/// so a larger configured value is cut to 4.
fn salt(world: &Config) -> Vec<Entry> {
    let id = "mekanism:salt";
    let min = world.int("salt", "minRadius");
    let max = world.int("salt", "maxRadius").max(min);
    let feature = format!(
        "{{\"type\":\"minecraft:disk\",\"config\":{{\"state_provider\":{{\"type\":\"minecraft:simple_state_provider\",\"state\":{{\"Name\":\"mekanism:block_salt\"}}}},\
         \"target\":{{\"predicate_type\":\"minecraft:matching_blocks\",\"blocks\":[\"minecraft:dirt\",\"minecraft:clay\"]}},\
         \"radius\":{{\"type\":\"minecraft:uniform\",\"min_inclusive\":{min},\"max_inclusive\":{max}}},\"half_height\":{}}}}}",
        world.int("salt", "halfHeight").min(4)
    );
    let placed = if world.flag("salt", "shouldGenerate") {
        format!(
            "{{\"feature\":\"{id}\",\"placement\":[{{\"type\":\"minecraft:count\",\"count\":{}}},{{\"type\":\"minecraft:in_square\"}},\
             {{\"type\":\"minecraft:heightmap\",\"heightmap\":\"OCEAN_FLOOR_WG\"}},\
             {{\"type\":\"minecraft:block_predicate_filter\",\"predicate\":{{\"type\":\"minecraft:matching_fluids\",\"fluids\":\"minecraft:water\"}}}},\
             {{\"type\":\"minecraft:biome\"}}]}}",
            world.int("salt", "perChunk")
        )
    } else {
        disabled(id)
    };
    vec![("features".into(), id.into(), feature), ("placed_features".into(), id.into(), placed)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_files::world;

    #[test]
    fn defaults_make_every_vein_and_the_salt() {
        let entries = entries(&Config::defaults(world()));
        assert_eq!(entries.len(), (10 + 1) * 2);
        let osmium = entries.iter().find(|e| e.1 == "mekanism:ore_osmium_upper" && e.0 == "placed_features").unwrap();
        assert!(osmium.2.contains("\"count\":65") && osmium.2.contains("\"plateau\":8") && osmium.2.contains("{\"below_top\":-24}"));
        for (_, _, json) in &entries {
            assert!(json.starts_with('{') && json.matches('{').count() == json.matches('}').count(), "{json}");
        }
    }

    #[test]
    fn a_disabled_ore_places_nothing() {
        let mut config = Config::defaults(world());
        config = Config::parse(world(), &config.to_toml().replace("\n[tin]\nshouldGenerate = true", "\n[tin]\nshouldGenerate = false"));
        let entries = entries(&config);
        let tin = entries.iter().find(|e| e.1 == "mekanism:ore_tin_small" && e.0 == "placed_features").unwrap();
        assert!(tin.2.contains("\"count\":0"), "{}", tin.2);
    }
}
