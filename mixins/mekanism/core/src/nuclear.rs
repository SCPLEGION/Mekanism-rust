// The nuclear fuel chain's machines (mekanism.common.tile.machine): the Chemical Dissolution Chamber
// (fluorite + sulfuric acid → hydrofluoric acid) and the Isotopic Centrifuge (uranium hexafluoride →
// fissile fuel, nuclear waste → plutonium). Uranium oxide comes from the oxidizer and uranium
// hexafluoride from the chemical infuser, which already run their recipe tables.

/// `TileEntityChemicalDissolutionChamber.BASE_TICKS_REQUIRED`.
pub const DISSOLUTION_TICKS: u32 = 100;
/// `UsageConfig.chemicalDissolutionChamber` (FE a tick).
pub const DISSOLUTION_USAGE: u64 = 16;
/// `TileEntityChemicalDissolutionChamber.MAX_CHEMICAL`, its input and output tanks.
pub const DISSOLUTION_TANK: u64 = 10_000;
/// `UsageConfig.isotopicCentrifuge` (FE a tick).
pub const CENTRIFUGE_USAGE: u64 = 8;
/// `TileEntityIsotopicCentrifuge.MAX_CHEMICAL`.
pub const CENTRIFUGE_TANK: u64 = 10_000;

/// A `dissolution` recipe: an item (by tag) and `per_tick` of a chemical every tick of its run make
/// `amount` of the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DissolutionRecipe {
    pub item_tag: &'static str,
    pub count: u32,
    pub chemical: &'static str,
    pub per_tick: u64,
    pub output: &'static str,
    pub amount: u64,
}

pub use crate::slurry_table::{CRYSTALLIZING, DISSOLUTION, WASHING};

pub fn find_dissolution(
    item: &str,
    count: u32,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'static DissolutionRecipe> {
    DISSOLUTION
        .iter()
        .find(|r| count >= r.count && in_tag(r.item_tag, item))
}

/// `TileEntityChemicalWasher`: `UsageConfig.chemicalWasher` FE a tick, its 10-bucket water tank and
/// chemical tanks.
pub const WASHER_USAGE: u64 = 5;
pub const WASHER_TANK: u64 = 10_000;
/// `TileEntityChemicalCrystallizer`: 200 ticks at `UsageConfig.chemicalCrystallizer` FE a tick.
pub const CRYSTALLIZER_TICKS: u32 = 200;
pub const CRYSTALLIZER_USAGE: u64 = 15;
pub const CRYSTALLIZER_TANK: u64 = 10_000;

/// A `washing` recipe: `input_amount` of a dirty slurry and `water` mB of water make `amount` of the
/// clean one, each tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WashingRecipe {
    pub input: &'static str,
    pub input_amount: u64,
    pub water: u64,
    pub output: &'static str,
    pub amount: u64,
}

pub fn find_washing(input: &str) -> Option<&'static WashingRecipe> {
    WASHING.iter().find(|r| r.input == input)
}

/// A `crystallizing` recipe: `amount` of a chemical becomes `count` items over the crystallizer's run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrystallizingRecipe {
    pub input: &'static str,
    pub amount: u64,
    pub output: &'static str,
    pub count: u32,
}

pub fn find_crystallizing(input: &str) -> Option<&'static CrystallizingRecipe> {
    CRYSTALLIZING.iter().find(|r| r.input == input)
}

/// Whether the dissolution chamber takes `chemical` into its input tank.
pub fn dissolves_with(chemical: &str) -> bool {
    DISSOLUTION.iter().any(|r| r.chemical == chemical)
}

/// `centrifuging` recipes: (input mB, output, output mB) for an input chemical.
pub fn centrifuging(input: &str) -> Option<(u64, &'static str, u64)> {
    match input {
        "mekanism:uranium_hexafluoride" => Some((1, "mekanism:fissile_fuel", 1)),
        "mekanism:nuclear_waste" => Some((10, "mekanism:plutonium", 1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluorite_dissolves_and_hexafluoride_spins() {
        let in_tag =
            |tag: &str, item: &str| tag == "c:gems/fluorite" && item == "mekanism:fluorite_gem";
        let r = find_dissolution("mekanism:fluorite_gem", 1, &in_tag).unwrap();
        assert_eq!(
            (r.per_tick * u64::from(DISSOLUTION_TICKS), r.amount),
            (100, 1_000)
        );
        assert!(dissolves_with("mekanism:sulfuric_acid"));
        assert_eq!(
            centrifuging("mekanism:uranium_hexafluoride"),
            Some((1, "mekanism:fissile_fuel", 1))
        );
        assert_eq!(
            centrifuging("mekanism:nuclear_waste").map(|c| c.0),
            Some(10)
        );
        assert_eq!(centrifuging("minecraft:water"), None);
    }

    #[test]
    fn ores_go_to_slurry_and_crystals() {
        let in_tag =
            |tag: &str, item: &str| tag == "c:raw_materials/iron" && item == "minecraft:raw_iron";
        let r = find_dissolution("minecraft:raw_iron", 3, &in_tag).unwrap();
        assert_eq!((r.output, r.amount), ("mekanism:dirty_iron", 2_000));
        assert!(find_dissolution("minecraft:raw_iron", 2, &in_tag).is_none());
        let w = find_washing("mekanism:dirty_iron").unwrap();
        assert_eq!((w.water, w.output, w.amount), (5, "mekanism:clean_iron", 1));
        let c = find_crystallizing("mekanism:clean_iron").unwrap();
        assert_eq!((c.amount, c.output), (200, "mekanism:crystal_iron"));
    }
}
