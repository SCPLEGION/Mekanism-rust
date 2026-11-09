// The fusion reactor's fuels: heavy water from the Electric Pump (`TileEntityElectricPump`), split into
// deuterium by the Electrolytic Separator, and tritium from lithium in the Solar Neutron Activator
// (`TileEntitySolarNeutronActivator`).

/// `BASE_TICKS_REQUIRED`: the pump takes a block's fluid every 19 ticks, at 2 FE a tick (`usage.electricPump`).
pub const PUMP_TICKS: u32 = 19;
pub const PUMP_USAGE: u64 = 2;
/// `MAX_FLUID`: 10 buckets.
pub const PUMP_TANK: u64 = 10_000;
/// `general.pump.heavyWaterAmount`: a water source gives 10 mB of heavy water with a filter upgrade.
pub const HEAVY_WATER_PER_BLOCK: u64 = 10;

/// What the pump gets from a source block of `fluid`, with or without a filter upgrade.
pub fn pumped(fluid: &str, filter: bool) -> Option<(&'static str, u64)> {
    match fluid {
        "minecraft:water" if filter => Some((crate::chemical::HEAVY_WATER, crate::config_files::active_int("general", "pump", "heavyWaterAmount", HEAVY_WATER_PER_BLOCK))),
        "minecraft:water" => Some(("minecraft:water", 1_000)),
        "minecraft:lava" => Some(("minecraft:lava", 1_000)),
        _ => None,
    }
}

/// `MAX_GAS`: both of the activator's tanks hold 10 buckets.
pub const ACTIVATOR_TANK: u64 = 10_000;
/// `general.maxSolarNeutronActivatorRate`: mB a tick at full sun.
pub const ACTIVATOR_PEAK: f64 = 64.0;

/// The activating recipes (`recipe/activating/tritium.json`, `processing/lategame/polonium.json`): the
/// mB of input an operation takes and the chemical it makes, 1 mB each: lithium into tritium 1:1,
/// nuclear waste into polonium 10:1.
pub fn activating(chemical: &str) -> Option<(u64, &'static str)> {
    match chemical {
        "mekanism:lithium" => Some((1, "mekanismgenerators:tritium")),
        "mekanism:nuclear_waste" => Some((10, "mekanism:polonium")),
        _ => None,
    }
}

/// The activator's operations this tick: `peakProductionRate × brightness`, rounded down, while it
/// sees the sun.
pub fn activator_rate(sees_sun: bool, brightness: f64) -> u64 {
    if sees_sun {
        (crate::config_files::active_int("general", "", "maxSolarNeutronActivatorRate", ACTIVATOR_PEAK as u64) as f64 * brightness.clamp(0.0, 1.0)) as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_turns_water_into_heavy_water() {
        assert_eq!(
            pumped("minecraft:water", false),
            Some(("minecraft:water", 1_000))
        );
        assert_eq!(
            pumped("minecraft:water", true),
            Some(("mekanism:heavy_water", 10))
        );
        assert_eq!(pumped("minecraft:lava", true), Some(("minecraft:lava", 1_000)));
    }

    #[test]
    fn the_activator_follows_the_sun() {
        assert_eq!(activator_rate(true, 1.0), 64);
        assert_eq!(activator_rate(true, 0.5), 32);
        assert_eq!(activator_rate(false, 1.0), 0);
        assert_eq!(
            activating("mekanism:lithium"),
            Some((1, "mekanismgenerators:tritium"))
        );
        assert_eq!(
            activating("mekanism:nuclear_waste"),
            Some((10, "mekanism:polonium"))
        );
    }
}
