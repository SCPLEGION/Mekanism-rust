// Heat (mekanism.common.capabilities.heat.BasicHeatCapacitor, ITileHeatHandler.simulateEnvironment).

/// `HeatAPI.AMBIENT_TEMP`, kelvin: the ambient temperature of a biome at temperature 0.8.
pub const AMBIENT_TEMP: f64 = 300.0;

/// `HeatAPI.getAmbientTemp(biomeTemp)`: 300 K shifted 25 K per biome-temperature step from 0.8.
pub fn ambient_temp(biome_temp: f64) -> f64 {
    AMBIENT_TEMP + 25.0 * (biome_temp.clamp(-5.0, 5.0) - 0.8)
}
/// `HeatAPI.AIR_INVERSE_COEFFICIENT`.
pub const AIR_INVERSE_COEFFICIENT: f64 = 10_000.0;
/// `HeatAPI.EPSILON`.
pub const EPSILON: f64 = 1e-6;

#[derive(Clone, Debug, PartialEq)]
pub struct HeatCapacitor {
    /// Stored heat; temperature is heat / capacity. Starts at the ambient temperature.
    pub heat: f64,
    pub capacity: f64,
    pub inverse_conduction: f64,
    pub inverse_insulation: f64,
    /// The temperature it relaxes toward (`ambientTempSupplier`).
    pub ambient: f64,
}

impl HeatCapacitor {
    pub fn new(capacity: f64, inverse_conduction: f64, inverse_insulation: f64) -> Self {
        Self::with_ambient(capacity, inverse_conduction, inverse_insulation, AMBIENT_TEMP)
    }

    pub fn with_ambient(
        capacity: f64,
        inverse_conduction: f64,
        inverse_insulation: f64,
        ambient: f64,
    ) -> Self {
        Self {
            heat: capacity * ambient,
            capacity,
            inverse_conduction,
            inverse_insulation,
            ambient,
        }
    }

    pub fn temperature(&self) -> f64 {
        self.heat / self.capacity
    }

    /// `handleHeat`: adds (or with a negative `transfer`, removes) heat; never below zero.
    pub fn handle_heat(&mut self, transfer: f64) {
        if transfer.abs() > EPSILON {
            self.heat = (self.heat + transfer).max(0.0);
        }
    }

    /// One side's loss to the air (`simulateEnvironment`'s loop body), with this side's inverse
    /// insulation; returns the temperature moved when it is hotter than ambient, else 0.
    pub fn lose_to_air(&mut self, inverse_insulation: f64) -> f64 {
        let inv = AIR_INVERSE_COEFFICIENT + inverse_insulation + self.inverse_conduction;
        let temp = (self.temperature() - self.ambient) / inv;
        self.handle_heat(-temp * self.capacity);
        temp.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_capacitor_is_at_ambient_and_never_goes_below_zero_heat() {
        let mut c = HeatCapacitor::new(10.0, 5.0, 100.0);
        assert_eq!(c.temperature(), AMBIENT_TEMP);
        c.handle_heat(-1e9);
        assert_eq!(c.heat, 0.0);
    }

    #[test]
    fn ambient_follows_the_biome_temperature() {
        assert_eq!(ambient_temp(0.8), 300.0);
        assert_eq!(ambient_temp(2.0), 330.0);
        assert_eq!(ambient_temp(-0.5), 267.5);
        assert_eq!(ambient_temp(99.0), 300.0 + 25.0 * 4.2);
        let mut c = HeatCapacitor::with_ambient(10.0, 5.0, 100.0, 330.0);
        assert_eq!(c.temperature(), 330.0);
        c.handle_heat(1_000.0);
        assert!(c.lose_to_air(100.0) > 0.0 && c.temperature() > 330.0);
    }

    #[test]
    fn a_hot_capacitor_cools_toward_ambient() {
        let mut c = HeatCapacitor::new(10.0, 5.0, 100.0);
        c.handle_heat(1_000.0);
        let before = c.temperature();
        assert!(c.lose_to_air(100.0) > 0.0);
        assert!(c.temperature() < before && c.temperature() > AMBIENT_TEMP);
    }
}
