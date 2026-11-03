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
    /// Heat lost to the air in the last tick (`getLastEnvironmentLoss`).
    pub last_env: f64,
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
            last_env: 0.0,
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
        let mut env = self.heat.lose_to_air(0.0);
        for _ in 0..5 {
            env += self.heat.lose_to_air(INVERSE_INSULATION_COEFFICIENT);
        }
        self.last_env = env;
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

/// `GeneratorsConfig.solarGeneration` and `advancedSolarGeneration`: FE a tick at full sun.
pub const SOLAR_GENERATION: u64 = 2;
pub const ADVANCED_SOLAR_GENERATION: u64 = 12;
/// `GeneratorsStorageConfig`: a generator stores 20 ticks of its peak.
pub const SOLAR_STORAGE: u64 = 2 * 20;
pub const ADVANCED_SOLAR_STORAGE: u64 = 12 * 20;
pub const WIND_STORAGE: u64 = 5 * 20;
/// `windGenerationMin`, `windGenerationMax`, `windGenerationMinY`.
pub const WIND_GENERATION_MIN: u64 = 1;
pub const WIND_GENERATION_MAX: u64 = 5;
pub const WIND_MIN_Y: i32 = 24;

/// `TileEntitySolarGenerator.getProduction`: the peak scaled by the sun's brightness (the sky light
/// factor with weather) and the biome's multiplier, nothing without the sun.
pub fn solar_production(peak: u64, sees_sun: bool, brightness: f32, biome_multiplier: f32) -> u64 {
    if !sees_sun {
        return 0;
    }
    let p = peak as f32 * (brightness.max(0.0) * biome_multiplier);
    if p >= i32::MAX as f32 {
        i32::MAX as u64
    } else {
        p.max(0.0) as u64
    }
}

/// `SolarCheck`'s peak multiplier: cooler biomes help, humid ones (that can rain) hurt.
pub fn solar_biome_multiplier(temperature: f32, downfall: f32, can_rain: bool) -> f32 {
    let temp = 0.3 * (0.8 - temperature);
    let humidity = if can_rain { -0.3 * downfall } else { 0.0 };
    1.0 + temp + humidity
}

/// `TileEntityWindGenerator.getMultiplier` for a generator whose top (4 blocks up) is at `top_y` and
/// sees the sky: linear from the min height to the top of the level. `min_build` is the level's min Y
/// and `max_level` its highest buildable Y.
pub fn wind_multiplier(top_y: i32, min_build: i32, max_level: i32) -> f64 {
    let min_y = WIND_MIN_Y.max(min_build);
    let max_y = max_level;
    if max_y <= min_y {
        return 1.0;
    }
    let y = top_y.clamp(min_y, max_y);
    let slope = (WIND_GENERATION_MAX - WIND_GENERATION_MIN) as f64 / f64::from(max_y - min_y);
    (WIND_GENERATION_MIN as f64 + slope * f64::from(y - min_y)) / WIND_GENERATION_MIN as f64
}

/// `getCurrentGeneration`.
pub fn wind_generation(multiplier: f64) -> u64 {
    let g = WIND_GENERATION_MIN as f64 * multiplier;
    if g >= f64::from(i32::MAX) {
        i32::MAX as u64
    } else {
        g.max(0.0) as u64
    }
}

/// The Bio-Generator (`TileEntityBioGenerator`, `GeneratorsConfig.bio*`): 1 mB of bioethanol a tick
/// makes 25 FE.
pub const BIO_GENERATOR: &str = "mekanismgenerators:bio_generator";
pub const BIOETHANOL: &str = "mekanismgenerators:bioethanol";
pub const BIO_GENERATION: u64 = 25;
pub const BIO_STORAGE: u64 = 30 * 20;
pub const BIO_TANK: u64 = 200;
/// `bioFuelPerItem`: the bioethanol a bio fuel gives (nine times that for a block of it).
pub const BIO_FUEL_PER_ITEM: u64 = 64;

/// One tick of a bio-generator with `fuel` mB and `room` FE free: (fuel used, energy made).
pub fn bio_tick(fuel: u64, room: u64) -> (u64, u64) {
    if fuel >= 1 && room >= BIO_GENERATION {
        (1, BIO_GENERATION)
    } else {
        (0, 0)
    }
}

#[cfg(test)]
mod sun_and_wind_tests {
    use super::*;

    #[test]
    fn solar_needs_the_sun_and_scales_with_it() {
        assert_eq!(solar_production(SOLAR_GENERATION, false, 1.0, 1.0), 0);
        assert_eq!(solar_production(SOLAR_GENERATION, true, 1.0, 1.0), 2);
        assert_eq!(
            solar_production(ADVANCED_SOLAR_GENERATION, true, 0.5, 1.0),
            6
        );
        // Plains: 0.8 and 0.4 downfall; a desert is hot and dry.
        assert!((solar_biome_multiplier(0.8, 0.4, true) - 0.88).abs() < 1e-6);
        assert!((solar_biome_multiplier(2.0, 0.0, false) - 0.64).abs() < 1e-6);
    }

    #[test]
    fn wind_grows_with_height() {
        // Overworld: min -64, top buildable 319.
        assert_eq!(wind_multiplier(0, -64, 319), 1.0);
        assert_eq!(wind_multiplier(319, -64, 319), 5.0);
        assert_eq!(wind_generation(wind_multiplier(319, -64, 319)), 5);
        assert_eq!(wind_generation(wind_multiplier(100, -64, 319)), 2);
    }

    #[test]
    fn bio_generator_needs_room() {
        assert_eq!(bio_tick(10, 600), (1, 25));
        assert_eq!(bio_tick(10, 24), (0, 0));
        assert_eq!(bio_tick(0, 600), (0, 0));
    }
}
