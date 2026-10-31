// Heat (mekanism.common.capabilities.heat.BasicHeatCapacitor, ITileHeatHandler.simulateEnvironment).

/// `HeatAPI.AMBIENT_TEMP`, kelvin. Mekanism adjusts it by biome; this port uses the default for now.
pub const AMBIENT_TEMP: f64 = 300.0;
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
}

impl HeatCapacitor {
    pub fn new(capacity: f64, inverse_conduction: f64, inverse_insulation: f64) -> Self {
        Self {
            heat: capacity * AMBIENT_TEMP,
            capacity,
            inverse_conduction,
            inverse_insulation,
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
        let temp = (self.temperature() - AMBIENT_TEMP) / inv;
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
    fn a_hot_capacitor_cools_toward_ambient() {
        let mut c = HeatCapacitor::new(10.0, 5.0, 100.0);
        c.handle_heat(1_000.0);
        let before = c.temperature();
        assert!(c.lose_to_air(100.0) > 0.0);
        assert!(c.temperature() < before && c.temperature() > AMBIENT_TEMP);
    }
}
