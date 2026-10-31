// The Fission Reactor (mekanism.generators.common.content.fission), simplified: a box of casing and
// reactor glass with ports in its walls and fuel assemblies inside. It burns fissile fuel at a set rate,
// turns the heat into steam from its water and leaves nuclear waste.

pub const MIN_SIZE: i32 = 3;
pub const MAX_SIZE: i32 = 18;
/// `GeneratorsConfig.maxFuelPerAssembly` (8 buckets) and `burnPerAssembly` (1 mB a tick).
pub const FUEL_PER_ASSEMBLY: u64 = 8_000;
/// `cooledCoolantPerTank` and `heatedCoolantPerTank`: buckets per block of the reactor's volume.
pub const WATER_PER_BLOCK: u64 = 100_000;
pub const STEAM_PER_BLOCK: u64 = 1_000_000;
/// `energyPerFissionFuel` (1 MJ a mB) over water's enthalpy as coolant (`maxEnergyPerSteam` / steam
/// efficiency = 50 J a mB): 20,000 mB of water boils for each mB of fuel, here per tenth of a mB.
pub const STEAM_PER_TENTH: u64 = 2_000;
/// `defaultBurnRate`, in tenths of a mB a tick.
pub const DEFAULT_BURN_TENTHS: u64 = 1;
/// The burn rates a click on a port steps through, in tenths of a mB a tick, up to the reactor's maximum.
pub const BURN_STEPS: [u64; 7] = [1, 5, 10, 20, 50, 100, 200];

pub fn is_frame(name: &str) -> bool {
    matches!(
        name,
        "mekanismgenerators:fission_reactor_casing"
            | "mekanismgenerators:reactor_glass"
            | "mekanismgenerators:fission_reactor_port"
            | "mekanismgenerators:fission_reactor_logic_adapter"
    )
}

/// What a formed reactor holds inside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reactor {
    pub assemblies: u64,
    pub volume: u64,
}

impl Reactor {
    /// Counts an inner block; false for one that may not be inside.
    pub fn add_inside(&mut self, name: &str) -> bool {
        match name {
            "mekanismgenerators:fission_fuel_assembly" => self.assemblies += 1,
            "mekanismgenerators:control_rod_assembly" | "minecraft:air" | "minecraft:cave_air" => {}
            _ => return false,
        }
        true
    }

    pub fn fuel_capacity(&self) -> u64 {
        self.assemblies * FUEL_PER_ASSEMBLY
    }

    pub fn water_capacity(&self) -> u64 {
        self.volume * WATER_PER_BLOCK
    }

    pub fn steam_capacity(&self) -> u64 {
        self.volume * STEAM_PER_BLOCK
    }

    /// `getMaxBurnRate`, in tenths.
    pub fn max_burn_tenths(&self) -> u64 {
        self.assemblies * 10
    }
}

/// The next burn rate after `now`, wrapping to the lowest past the reactor's maximum.
pub fn next_burn(now: u64, max: u64) -> u64 {
    BURN_STEPS
        .iter()
        .copied()
        .find(|&s| s > now && s <= max)
        .unwrap_or(DEFAULT_BURN_TENTHS.min(max.max(1)))
}

/// One tick's burn: tenths burned, mB of fuel taken (whole mB, as `partial` tenths add up), water
/// boiled and waste made. None when the water cannot take the heat: the reactor stops (SCRAM) instead
/// of heating up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burn {
    pub fuel: u64,
    pub steam: u64,
    pub waste: u64,
}

pub fn burn(
    rate_tenths: u64,
    partial: &mut u64,
    fuel: u64,
    water: u64,
    steam_room: u64,
    waste_room: u64,
) -> Option<Burn> {
    let tenths = rate_tenths.min(fuel * 10 + *partial);
    if tenths == 0 {
        return Some(Burn {
            fuel: 0,
            steam: 0,
            waste: 0,
        });
    }
    let steam = tenths * STEAM_PER_TENTH;
    if water < steam || steam_room < steam {
        return None;
    }
    // Take whole mB of fuel as the tenths need them, keeping the rest for later ticks.
    let need = tenths.saturating_sub(*partial);
    let taken = need.div_ceil(10);
    *partial = *partial + taken * 10 - tenths;
    if waste_room < taken {
        return None;
    }
    Some(Burn {
        fuel: taken,
        steam,
        waste: taken,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tenth_a_tick_takes_a_mb_every_ten_ticks() {
        let mut partial = 0;
        let mut fuel = 0;
        for _ in 0..10 {
            let b = burn(1, &mut partial, 5, 1_000_000, 1_000_000, 1_000).unwrap();
            assert_eq!(b.steam, 2_000);
            fuel += b.fuel;
        }
        assert_eq!((fuel, partial), (1, 0));
    }

    #[test]
    fn no_water_stops_the_reactor() {
        let mut partial = 0;
        assert_eq!(burn(10, &mut partial, 5, 19_999, u64::MAX, 100), None);
        assert_eq!(
            burn(10, &mut partial, 0, 0, 0, 0),
            Some(Burn {
                fuel: 0,
                steam: 0,
                waste: 0
            })
        );
    }

    #[test]
    fn reactors_count_assemblies_and_step_rates() {
        let mut r = Reactor {
            volume: 27,
            ..Default::default()
        };
        assert!(r.add_inside("mekanismgenerators:fission_fuel_assembly"));
        assert!(!r.add_inside("minecraft:stone"));
        assert_eq!((r.fuel_capacity(), r.max_burn_tenths()), (8_000, 10));
        assert_eq!(next_burn(1, 10), 5);
        assert_eq!(next_burn(10, 10), 1);
    }
}
