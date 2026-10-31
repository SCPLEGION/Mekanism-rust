//! The Thermal Evaporation Plant (`EvaporationMultiblockData`): a 4x4 tower, 3 to 18 blocks high, that
//! heats water into brine. Its heat comes from solar panels on its top corners and leaks back to the
//! ambient temperature.

/// `EvaporationMultiblockData.MAX_HEIGHT`, and the shortest tower that forms.
pub const MAX_HEIGHT: u32 = 18;
pub const MIN_HEIGHT: u32 = 3;
pub const MAX_MULTIPLIER_TEMP: f64 = 3_000.0;
/// `HeatAPI.AMBIENT_TEMP`, in kelvin.
pub const AMBIENT_TEMP: f64 = 300.0;
/// `GeneralConfig`'s evaporation defaults.
pub const HEAT_DISSIPATION: f64 = 0.02;
pub const SOLAR_MULTIPLIER: f64 = 0.2;
pub const TEMP_MULTIPLIER: f64 = 0.4;
pub const FLUID_PER_TANK: u64 = 64_000;
pub const OUTPUT_TANK: u64 = 10_000;
/// `recipe/evaporating/brine.json`: 10 mB of water make 1 mB of brine.
pub const WATER_PER_BRINE: u64 = 10;

/// The water tank grows with the tower: a quarter of its volume, times 64 buckets.
pub fn input_capacity(height: u32) -> u64 {
    u64::from(4 * 4 * height / 4) * FLUID_PER_TANK
}

/// One tick of `simulateEnvironment`, in temperature (the capacitor's heat over its capacity): each
/// active solar adds 0.2 K, and the tower moves towards the ambient by 0.02 K per square root of the
/// difference.
pub fn step_temperature(temperature: f64, solars: u32, ambient: f64) -> f64 {
    let t = temperature + f64::from(solars) * SOLAR_MULTIPLIER;
    let diff = (t - ambient).abs();
    if diff < 0.001 {
        return ambient;
    }
    let incr = HEAT_DISSIPATION * diff.sqrt();
    if t > ambient { t - incr } else { t + incr }
}

/// `tempMultiplier`: how many operations a tick a tower of `height` runs at `temperature`.
pub fn temp_multiplier(temperature: f64, height: u32) -> f64 {
    (temperature.min(MAX_MULTIPLIER_TEMP) - AMBIENT_TEMP) * TEMP_MULTIPLIER * f64::from(height)
        / f64::from(MAX_HEIGHT)
}

/// Operations to run this tick: below 1 a multiplier runs one operation every `ceil(1 / m)` ticks
/// (`setRequiredTicks`), above it `floor(m)` operations every tick (`setBaselineMaxOperations`).
/// `progress` counts the ticks waited.
pub fn operations(multiplier: f64, progress: &mut u32) -> u64 {
    if multiplier <= 0.0 {
        *progress = 0;
        return 0;
    }
    if multiplier < 1.0 {
        let required = (1.0 / multiplier).ceil() as u32;
        *progress += 1;
        if *progress >= required {
            *progress = 0;
            return 1;
        }
        return 0;
    }
    *progress = 0;
    multiplier as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_solars_settle_at_1900_kelvin() {
        let mut t = AMBIENT_TEMP;
        for _ in 0..200_000 {
            t = step_temperature(t, 4, AMBIENT_TEMP);
        }
        assert!((t - 1_900.0).abs() < 1.0, "{t}");
        let m = temp_multiplier(t, MAX_HEIGHT);
        assert!((m - 640.0).abs() < 1.0, "{m}");
    }

    #[test]
    fn a_cold_tower_does_nothing_and_a_warm_one_waits() {
        let mut p = 0;
        assert_eq!(operations(temp_multiplier(AMBIENT_TEMP, 18), &mut p), 0);
        let m = 0.4;
        let ran: u64 = (0..9).map(|_| operations(m, &mut p)).sum();
        assert_eq!(ran, 3);
        assert_eq!(input_capacity(3), 12 * FLUID_PER_TANK);
    }
}
