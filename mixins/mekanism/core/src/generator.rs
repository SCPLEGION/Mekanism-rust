// The Heat Generator (mekanism.generators.common.tile.TileEntityHeatGenerator): furnace fuel becomes
// lava in its tank, burning lava heats it, and heat above ambient becomes energy at the Carnot efficiency.

use crate::energy::EnergyContainer;
use crate::heat::{AMBIENT_TEMP, HeatCapacitor};

/// `TileEntityHeatGenerator` constants.
pub const HEAT_CAPACITY: f64 = 10.0;
pub const INVERSE_CONDUCTION_COEFFICIENT: f64 = 5.0;
pub const INVERSE_INSULATION_COEFFICIENT: f64 = 100.0;
pub const THERMAL_EFFICIENCY: f64 = 0.5;
/// `GeneratorsConfig` defaults: heat per tick while burning lava, lava burnt per tick, tank size.
pub const HEAT_GENERATION: f64 = 100.0;
pub const FLUID_RATE: u32 = 100;
pub const TANK_CAPACITY: u32 = 1_000;
/// `GeneratorsConfig.heatGenerationLava` and `heatGenerationNether` defaults, for MAX_PRODUCTION.
pub const HEAT_GENERATION_LAVA: u64 = 7;
pub const HEAT_GENERATION_NETHER: u64 = 10;
/// `MAX_PRODUCTION`: lava on all six sides and below, the nether bonus and burning: 159 with the defaults.
pub const MAX_PRODUCTION: u64 =
    HEAT_GENERATION_LAVA * 7 + HEAT_GENERATION_NETHER + HEAT_GENERATION as u64;
/// `GeneratorsStorageConfig.heatGenerator` default: 240 FE.
pub const STORAGE: u64 = 12 * 20;
/// The burn time of a lava bucket, which `FluidFuelInventorySlot.forFuel` measures fuel against.
pub const LAVA_BUCKET_BURN_TIME: u32 = 20_000;

#[derive(Clone, Debug, PartialEq)]
pub struct HeatGenerator {
    pub energy: EnergyContainer,
    pub heat: HeatCapacitor,
    /// Lava in the tank, mB.
    pub lava: u32,
    pub active: bool,
    /// Energy made in the last tick.
    pub producing: u64,
}

impl Default for HeatGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl HeatGenerator {
    pub fn new() -> Self {
        Self {
            energy: EnergyContainer::new(STORAGE, u64::MAX),
            heat: HeatCapacitor::new(
                HEAT_CAPACITY,
                INVERSE_CONDUCTION_COEFFICIENT,
                INVERSE_INSULATION_COEFFICIENT,
            ),
            lava: 0,
            active: false,
            producing: 0,
        }
    }

    /// The lava a fuel item of `burn_time` ticks gives (bucket volume × burn time / lava bucket's).
    pub fn lava_for(burn_time: u32) -> u32 {
        (1_000u64 * burn_time as u64 / LAVA_BUCKET_BURN_TIME as u64) as u32
    }

    /// Whether one fuel item of `burn_time` fits in the tank now (`fillOrBurn` takes it only then).
    pub fn accepts_fuel(&self, burn_time: u32) -> bool {
        let lava = Self::lava_for(burn_time);
        lava > 0 && self.lava + lava <= TANK_CAPACITY
    }

    pub fn add_fuel(&mut self, burn_time: u32) {
        self.lava += Self::lava_for(burn_time);
    }

    /// One server tick (`onUpdateServer` then `simulate`); the passive lava boost is not modelled yet.
    pub fn tick(&mut self) {
        self.energy.start_tick();
        let before = self.energy.stored;
        self.active = false;
        if self.lava >= FLUID_RATE && self.energy.needed() > 0 {
            self.lava -= FLUID_RATE;
            self.active = true;
            self.heat.handle_heat(HEAT_GENERATION);
        }
        // simulate(): the thermoelectric part, then the environment, six sides (the bottom keeps the
        // default insulation of 0).
        let temp = self.heat.temperature();
        let carnot = 1.0 - temp.min(AMBIENT_TEMP) / temp.max(AMBIENT_TEMP);
        let heat_lost = THERMAL_EFFICIENCY * (temp - AMBIENT_TEMP);
        self.heat.handle_heat(-heat_lost);
        let energy = ((heat_lost.abs() * carnot) as u64).min(MAX_PRODUCTION);
        self.energy.insert(energy, crate::energy::Action::Execute);
        self.heat.lose_to_air(0.0);
        for _ in 0..5 {
            self.heat.lose_to_air(INVERSE_INSULATION_COEFFICIENT);
        }
        self.producing = self.energy.stored - before;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_production_is_159_with_the_defaults() {
        assert_eq!(MAX_PRODUCTION, 159);
    }

    #[test]
    fn coal_gives_eighty_millibuckets_of_lava() {
        assert_eq!(HeatGenerator::lava_for(1_600), 80);
        let mut g = HeatGenerator::new();
        g.lava = 950;
        assert!(!g.accepts_fuel(1_600));
    }

    #[test]
    fn burning_lava_warms_it_up_and_makes_energy() {
        let mut g = HeatGenerator::new();
        g.lava = 1_000;
        let mut made = 0;
        for _ in 0..10 {
            g.tick();
            made += g.producing;
            g.energy.stored = 0;
        }
        assert!(
            g.heat.temperature() > AMBIENT_TEMP + 50.0,
            "{}",
            g.heat.temperature()
        );
        assert_eq!(g.lava, 0);
        // The heat left turns into energy as it cools.
        for _ in 0..200 {
            g.tick();
            made += g.producing;
            g.energy.stored = 0;
        }
        assert!(made > 50, "{made}");
        assert!(
            g.heat.temperature() < AMBIENT_TEMP + 1.0,
            "{}",
            g.heat.temperature()
        );
    }

    #[test]
    fn without_lava_it_cools_and_stops() {
        let mut g = HeatGenerator::new();
        g.tick();
        assert!(!g.active);
        assert_eq!(g.producing, 0);
    }
}
