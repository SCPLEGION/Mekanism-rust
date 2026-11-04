// The Thermoelectric Boiler (mekanism.common.content.boiler.BoilerMultiblockData), simplified: a box of
// boiler casing, valves and structural glass with a full layer of pressure dispersers inside and
// superheating elements below it. Heat from Resistive Heaters beside its walls boils its water into
// steam.

pub const MIN_SIZE: i32 = 3;
pub const MAX_SIZE: i32 = 18;
pub const VALVE: &str = "mekanism:boiler_valve";
pub const DISPERSER: &str = "mekanism:pressure_disperser";
pub const ELEMENT: &str = "mekanism:superheating_element";
pub const HEATER: &str = "mekanism:resistive_heater";
/// The Fuelwood Heater: furnace fuel into heat (`GeneralConfig.heatPerFuelTick`, 400 a tick of burn
/// time).
pub const FUELWOOD_HEATER: &str = "mekanism:fuelwood_heater";
pub const HEAT_PER_FUEL_TICK: f64 = 400.0;
/// The Resistive and Fuelwood Heaters' `BasicHeatCapacitor`: capacity, inverse conduction, inverse insulation.
pub const HEATER_HEAT_CAPACITY: f64 = 100.0;
pub const HEATER_INVERSE_CONDUCTION: f64 = 5.0;
pub const HEATER_INVERSE_INSULATION: f64 = 10.0;

/// `general.boiler`: `waterPerTank`, `steamPerTank`, `waterConductivity`, `superheatingHeatTransfer`.
pub const WATER_PER_BLOCK: u64 = 16_000;
pub const STEAM_PER_BLOCK: u64 = 160_000;
const WATER_CONDUCTIVITY: f64 = 0.7;
const SUPERHEATING_HEAT_TRANSFER: f64 = 16_000_000.0;
/// `CASING_HEAT_CAPACITY` per block, and the casing's inverse insulation and conduction.
pub const CASING_HEAT_CAPACITY: f64 = 50.0;
pub const CASING_INVERSE_INSULATION: f64 = 100_000.0;
pub const CASING_INVERSE_CONDUCTION: f64 = 1.0;
/// `general.boiler.heatedCoolantPerTank` and `cooledCoolantPerTank`: 256 buckets a tank.
pub const COOLANT_PER_BLOCK: u64 = 256_000;
/// The heated coolant chemical and its cooled variant (`HeatedCoolant` / `CooledCoolant` data maps).
pub const HEATED_COOLANT: &str = "mekanism:superheated_sodium";
pub const COOLED_COOLANT: &str = "mekanism:sodium";
/// `HeatedCoolant(sodium, thermalEnthalpy 5)` with the base conductivity 0.4 and temperature 100,000 K.
pub const COOLANT_ENTHALPY: f64 = 5.0;
pub const COOLANT_CONDUCTIVITY: f64 = 0.4;
pub const COOLANT_TEMPERATURE: f64 = 100_000.0;

/// `HeatUtils.BASE_BOIL_TEMP`: 100 °C.
pub const BOIL_TEMP: f64 = 373.15;
const STEAM_EFFICIENCY: f64 = 0.2;
const WATER_ENTHALPY: f64 = 10.0;

/// `TileEntityResistiveHeater.BASE_USAGE` and `general.resistiveEfficiency`.
pub const HEATER_USAGE: u64 = 100;
pub const HEATER_EFFICIENCY: f64 = 0.6;
/// The usages a click on a heater steps through (its screen's energy usage field).
pub const HEATER_STEPS: [u64; 5] = [100, 1_000, 10_000, 100_000, 1_000_000];

pub fn next_heater_usage(now: u64) -> u64 {
    HEATER_STEPS
        .iter()
        .copied()
        .find(|&s| s > now)
        .unwrap_or(HEATER_USAGE)
}

pub fn is_frame(name: &str) -> bool {
    matches!(
        name,
        "mekanism:boiler_casing" | VALVE | "mekanism:structural_glass"
    )
}

/// What a formed boiler holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Boiler {
    pub elements: u64,
    /// The whole box, for the casing's heat capacity.
    pub volume: u64,
    /// Inside blocks below the dispersers (water) and above them (steam).
    pub water_volume: u64,
    pub steam_volume: u64,
}

impl Boiler {
    pub fn water_capacity(&self) -> u64 {
        self.water_volume * WATER_PER_BLOCK
    }

    pub fn steam_capacity(&self) -> u64 {
        self.steam_volume * STEAM_PER_BLOCK
    }

    /// The superheated coolant tank holds a tank's worth for each block of water space.
    pub fn heated_coolant_capacity(&self) -> u64 {
        self.water_volume * COOLANT_PER_BLOCK
    }

    /// The cooled coolant tank, for each block of steam space.
    pub fn cooled_coolant_capacity(&self) -> u64 {
        self.steam_volume * COOLANT_PER_BLOCK
    }

    /// The coolant step of `BoilerMultiblockData.tick`: the share of `heated` mB that gives its heat to the casing at
    /// `temperature`, limited by the room in the cooled tank. Returns the mB cooled; the casing gains
    /// `cooled * COOLANT_ENTHALPY` heat.
    pub fn cool(temperature: f64, heated: u64, cooled_room: u64) -> u64 {
        let portion = COOLANT_CONDUCTIVITY * heated as f64;
        let to_cool = (portion * (1.0 - temperature / COOLANT_TEMPERATURE)).round();
        (to_cool.max(0.0) as u64).min(cooled_room).min(heated)
    }

    pub fn heat_capacity(&self) -> f64 {
        CASING_HEAT_CAPACITY * self.volume as f64
    }

    /// One tick of `BoilerMultiblockData.tick` after the heat came in: `getHeatAvailable` over the
    /// boiling point, capped by the superheating elements, boils water into steam and takes its heat.
    /// `heat` is the casing's stored heat (temperature × capacity). Returns the mB boiled.
    pub fn boil(&self, heat: &mut f64, water: u64, steam_room: u64) -> u64 {
        if water == 0 {
            return 0;
        }
        let boiled = self.max_boil(*heat).min(steam_room).min(water);
        *heat -= boiled as f64 * WATER_ENTHALPY / STEAM_EFFICIENCY;
        boiled
    }

    /// `lastMaxBoil`: what the heat above the boiling point could boil this tick.
    pub fn max_boil(&self, heat: f64) -> u64 {
        let capacity = self.heat_capacity();
        if capacity <= 0.0 || heat / capacity < BOIL_TEMP {
            return 0;
        }
        let available = ((heat - BOIL_TEMP * capacity) * WATER_CONDUCTIVITY)
            .min(SUPERHEATING_HEAT_TRANSFER * self.elements as f64);
        (STEAM_EFFICIENCY * available / WATER_ENTHALPY).floor() as u64
    }

    /// The second rate bar's level: the max boil rate over what the elements could carry.
    pub fn max_boil_level(&self, max: u64) -> f64 {
        if self.elements == 0 {
            return 0.0;
        }
        (max as f64 * WATER_ENTHALPY / (self.elements as f64 * SUPERHEATING_HEAT_TRANSFER)).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boiler() -> Boiler {
        Boiler {
            elements: 1,
            volume: 27,
            water_volume: 1,
            steam_volume: 0,
        }
    }

    #[test]
    fn cold_water_does_not_boil() {
        let b = boiler();
        let mut heat = b.heat_capacity() * 300.0;
        assert_eq!(b.boil(&mut heat, 1_000, 1_000), 0);
    }

    #[test]
    fn heat_over_the_boiling_point_boils_50_heat_a_mb() {
        let b = boiler();
        let start = b.heat_capacity() * BOIL_TEMP + 1_000.0;
        let mut heat = start;
        // 1,000 over × 0.7 = 700 available → 0.2 × 700 / 10 = 14 mB.
        assert_eq!(b.boil(&mut heat, 1_000, 1_000), 14);
        assert!((start - heat - 700.0).abs() < 1e-3);
        // 300 left over → 210 available → 4 mB.
        assert_eq!(b.boil(&mut heat, 1_000, 1_000), 4);
    }

    #[test]
    fn hot_coolant_heats_the_casing_and_cools() {
        // 1,000 mB at 300 K: 0.4 × 1,000 × (1 - 300 / 100,000) = 398.8 → 399.
        assert_eq!(Boiler::cool(300.0, 1_000, u64::MAX), 399);
        // Limited by the room for the cooled coolant.
        assert_eq!(Boiler::cool(300.0, 1_000, 100), 100);
        // A casing hotter than the coolant gets nothing.
        assert_eq!(Boiler::cool(200_000.0, 1_000, u64::MAX), 0);
        let b = boiler();
        assert_eq!(b.heated_coolant_capacity(), 256_000);
        assert_eq!(b.cooled_coolant_capacity(), 0);
    }

    #[test]
    fn heaters_step_their_usage() {
        assert_eq!(next_heater_usage(100), 1_000);
        assert_eq!(next_heater_usage(1_000_000), 100);
    }
}
