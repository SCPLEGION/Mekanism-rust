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
    pub chemical: &'static str,
    pub per_tick: u64,
    pub output: &'static str,
    pub amount: u64,
}

/// `processing/uranium/hydrofluoric_acid{,_from_block}`. The ore slurries (dirty slurry from ore and
/// sulfuric acid) wait for the washer and crystallizer.
pub const DISSOLUTION: [DissolutionRecipe; 2] = [
    DissolutionRecipe {
        item_tag: "c:gems/fluorite",
        chemical: "mekanism:sulfuric_acid",
        per_tick: 1,
        output: "mekanism:hydrofluoric_acid",
        amount: 1_000,
    },
    DissolutionRecipe {
        item_tag: "c:storage_blocks/fluorite",
        chemical: "mekanism:sulfuric_acid",
        per_tick: 9,
        output: "mekanism:hydrofluoric_acid",
        amount: 9_000,
    },
];

pub fn find_dissolution(item: &str, in_tag: &dyn Fn(&str, &str) -> bool) -> Option<&'static DissolutionRecipe> {
    DISSOLUTION.iter().find(|r| in_tag(r.item_tag, item))
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
        let in_tag = |tag: &str, item: &str| tag == "c:gems/fluorite" && item == "mekanism:fluorite_gem";
        let r = find_dissolution("mekanism:fluorite_gem", &in_tag).unwrap();
        assert_eq!((r.per_tick * u64::from(DISSOLUTION_TICKS), r.amount), (100, 1_000));
        assert!(dissolves_with("mekanism:sulfuric_acid"));
        assert_eq!(centrifuging("mekanism:uranium_hexafluoride"), Some((1, "mekanism:fissile_fuel", 1)));
        assert_eq!(centrifuging("mekanism:nuclear_waste").map(|c| c.0), Some(10));
        assert_eq!(centrifuging("minecraft:water"), None);
    }
}
