// The Network Reader (`ItemNetworkReader`), the Tier Installers (`ItemTierInstaller`) and the alloys that upgrade
// transmitters (`ItemAlloy`, `TileEntityTransmitter.onAlloyInteraction`).

use crate::tier::BaseTier;

pub const ITEM: &str = "mekanism:network_reader";
/// `GearConfig.networkReaderMaxEnergy` and `networkReaderChargeRate`.
pub const CAPACITY: u64 = 60_000;
pub const CHARGE_RATE: u64 = 300;
/// `GearConfig.networkReaderEnergyUsage`: what one reading costs.
pub const USAGE: u64 = 400;
/// `ItemAlloy.useOn` upgrades at most this many transmitters of the network at once.
pub const ALLOY_UPGRADES: usize = 8;

/// The blocks with tiers that an installer swaps (`AttributeUpgradeable`): a block id is `mekanism:<tier>_<type>`.
const TIERED: [&str; 4] = ["bin", "energy_cube", "fluid_tank", "chemical_tank"];

/// The transmitters an alloy upgrades (`IUpgradeableTransmitter`).
const TRANSMITTERS: [&str; 5] = ["universal_cable", "pressurized_tube", "mechanical_pipe", "logistical_transporter", "thermodynamic_conductor"];

/// `ItemTierInstaller`'s (from, to) of an item: the basic one has no `from`, it turns a machine into a factory.
pub fn installer(item: &str) -> Option<(Option<BaseTier>, BaseTier)> {
    let name = item.strip_prefix("mekanism:")?.strip_suffix("_tier_installer")?;
    let to = BaseTier::ALL.into_iter().find(|t| t.name() == name && *t != BaseTier::Creative)?;
    let from = BaseTier::ALL.into_iter().find(|t| t.next() == Some(to));
    Some((from, to))
}

/// The tier an alloy upgrades transmitters to (`AlloyTier.getBaseTier`).
pub fn alloy(item: &str) -> Option<BaseTier> {
    match item {
        "mekanism:alloy_infused" => Some(BaseTier::Advanced),
        "mekanism:alloy_reinforced" => Some(BaseTier::Elite),
        "mekanism:alloy_atomic" => Some(BaseTier::Ultimate),
        _ => None,
    }
}

/// The tier of a block id of the form `mekanism:<tier>_<rest>`, and `rest`.
fn split_tier(block: &str) -> Option<(BaseTier, &str)> {
    let name = block.strip_prefix("mekanism:")?;
    BaseTier::ALL.into_iter().find_map(|t| name.strip_prefix(t.name())?.strip_prefix('_').map(|rest| (t, rest)))
}

/// `AttributeUpgradeable.upgradeResult` for an installer from `from` to `to`: the block of the next tier.
pub fn upgrade_block(block: &str, from: BaseTier, to: BaseTier) -> Option<String> {
    let (tier, rest) = split_tier(block)?;
    let tiered = TIERED.contains(&rest) || rest.ends_with("_factory");
    (tier == from && from != to && tiered).then(|| format!("mekanism:{}_{rest}", to.name()))
}

/// `Machine`'s `AttributeUpgradeable`: the basic factory a machine becomes, and the factory kind it is.
pub fn factory_of_machine(block: &str) -> Option<(String, &'static str)> {
    let kind = match block {
        "mekanism:enrichment_chamber" => "enriching",
        "mekanism:crusher" => "crushing",
        "mekanism:energized_smelter" => "smelting",
        "mekanism:precision_sawmill" => "sawing",
        "mekanism:combiner" => "combining",
        "mekanism:osmium_compressor" => "compressing",
        "mekanism:purification_chamber" => "purifying",
        "mekanism:chemical_injection_chamber" => "injecting",
        "mekanism:metallurgic_infuser" => "infusing",
        _ => return None,
    };
    Some((format!("mekanism:basic_{kind}_factory"), kind))
}

/// The transmitter block an alloy of tier `target` turns `block` into: one tier below it, of a kind that upgrades
/// (`IUpgradeableTransmitter.canUpgrade`: the alloy's tier is the transmitter's plus one).
pub fn upgrade_transmitter(block: &str, target: BaseTier) -> Option<String> {
    let (tier, rest) = split_tier(block)?;
    (TRANSMITTERS.contains(&rest) && tier.next() == Some(target)).then(|| format!("mekanism:{}_{rest}", target.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installers_and_alloys() {
        assert_eq!(installer("mekanism:basic_tier_installer"), Some((None, BaseTier::Basic)));
        assert_eq!(installer("mekanism:elite_tier_installer"), Some((Some(BaseTier::Advanced), BaseTier::Elite)));
        assert_eq!(installer("mekanism:creative_tier_installer"), None);
        assert_eq!(alloy("mekanism:alloy_reinforced"), Some(BaseTier::Elite));
    }

    #[test]
    fn upgrades_swap_the_tier_only() {
        let (b, a) = (BaseTier::Basic, BaseTier::Advanced);
        assert_eq!(upgrade_block("mekanism:basic_energy_cube", b, a).as_deref(), Some("mekanism:advanced_energy_cube"));
        assert_eq!(upgrade_block("mekanism:basic_crushing_factory", b, a).as_deref(), Some("mekanism:advanced_crushing_factory"));
        assert_eq!(upgrade_block("mekanism:advanced_bin", b, a), None);
        assert_eq!(upgrade_block("mekanism:basic_universal_cable", b, a), None);
        assert_eq!(upgrade_transmitter("mekanism:basic_universal_cable", a).as_deref(), Some("mekanism:advanced_universal_cable"));
        assert_eq!(upgrade_transmitter("mekanism:elite_universal_cable", a), None);
        assert_eq!(factory_of_machine("mekanism:crusher").unwrap().0, "mekanism:basic_crushing_factory");
    }
}
