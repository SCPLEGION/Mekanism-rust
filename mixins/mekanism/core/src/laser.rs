// Lasers (mekanism.common.tile.laser): a laser fires its energy along its facing; the beam breaks the
// first block it meets once enough energy has gone into it, or gives the energy to a laser amplifier or
// tractor beam, which fire it on in turn. The tractor beam keeps what its beam breaks.

pub const LASER: &str = "mekanism:laser";
pub const AMPLIFIER: &str = "mekanism:laser_amplifier";
pub const TRACTOR_BEAM: &str = "mekanism:laser_tractor_beam";

/// `UsageConfig.laser`: what a laser fires each tick.
pub const LASER_USAGE: u64 = 20;
/// The laser stores four ticks of it (`AttributeEnergy.STORAGE_MULTIPLIER`).
pub const LASER_STORAGE: u64 = 4 * LASER_USAGE;
/// `StorageConfig.laserAmplifier` and `laserTractorBeam`.
pub const RECEPTOR_STORAGE: u64 = 5_000_000;
/// `GeneralConfig.laserRange`, in blocks.
pub const RANGE: i32 = 64;
/// `GeneralConfig.laserEnergyPerHardness`: the energy a block takes for each point of its hardness.
pub const ENERGY_PER_HARDNESS: f64 = 100_000.0;
/// The tractor beam's three rows of nine.
pub const TRACTOR_SLOTS: usize = 27;

/// `general.laser.energyPerDamage`: FE a point of damage to an entity in the beam takes.
pub const ENERGY_PER_DAMAGE: u64 = 2_500;

/// `energyPerDamage` of the open world's config (`ENERGY_PER_DAMAGE` is its default).
pub fn energy_per_damage() -> u64 {
    crate::config_files::active_int("general", "laser", "energyPerDamage", ENERGY_PER_DAMAGE)
}
/// Half the beam's width, as its box reaches around the beam's line.
pub const BEAM_HALF_WIDTH: f64 = 0.25;

/// The damage `energy` does to an entity in the beam, and the ticks it burns for (`fireLaser`).
pub fn damage(energy: u64) -> (f32, i32) {
    let value = energy as f64 / energy_per_damage() as f64;
    (value as f32, value.min(f64::from(i32::MAX)) as i32)
}

/// The energy a hit used, from the health the entity lost.
pub fn spent(lost: f32, energy: u64) -> u64 {
    ((f64::from(lost) * energy_per_damage() as f64) as u64).min(energy)
}

/// The beam's box from the laser at `pos` facing `d`, `length` blocks long from the laser's face.
pub fn beam_box(pos: [i32; 3], d: [i32; 3], length: f64) -> ([f64; 3], [f64; 3]) {
    let center = pos.map(|c| f64::from(c) + 0.5);
    let mut min = [0.0; 3];
    let mut max = [0.0; 3];
    for i in 0..3 {
        if d[i] == 0 {
            min[i] = center[i] - BEAM_HALF_WIDTH;
            max[i] = center[i] + BEAM_HALF_WIDTH;
        } else {
            let start = center[i] + f64::from(d[i]) * 0.501;
            let end = start + f64::from(d[i]) * length;
            min[i] = start.min(end);
            max[i] = start.max(end);
        }
    }
    (min, max)
}

pub fn is_receptor(id: &str) -> bool {
    id == AMPLIFIER || id == TRACTOR_BEAM
}

/// What a laser block fires this tick from `stored` (`toFire`): the laser its usage; the amplifier all
/// it holds, kept within its thresholds (nothing while it holds less than the lower one); the tractor
/// beam all it holds.
pub fn to_fire(id: &str, stored: u64, min: u64, max: u64) -> u64 {
    let want = match id {
        LASER => LASER_USAGE,
        AMPLIFIER => stored.clamp(min, max.max(min)),
        _ => stored,
    };
    if want <= stored { want } else { 0 }
}

/// `TileEntityLaserAmplifier.updateMinThreshold`: the new lower threshold (capped at the capacity), pulling the
/// upper one up with it when it would pass it. Returns the new `(min, max)`.
pub fn set_min_threshold(max: u64, capacity: u64, target: u64) -> (u64, u64) {
    let min = target.min(capacity);
    (min, max.max(min))
}

/// `updateMaxThreshold`: the new upper threshold (capped at the capacity), pulling the lower one down with it.
pub fn set_max_threshold(min: u64, capacity: u64, target: u64) -> (u64, u64) {
    let max = target.min(capacity);
    (min.min(max), max)
}

/// The amplifier's delay counter after one tick (`onUpdateServer`): it counts up to the delay, then starts again.
pub fn next_tick(ticks: u64, delay: u64) -> u64 {
    if ticks < delay { ticks + 1 } else { 0 }
}

/// Digging with `energy` more into a block of `hardness` that already has `progress`: the new progress,
/// and whether the block breaks (the progress then starts again). Unbreakable blocks take nothing.
pub fn dig(progress: u64, energy: u64, hardness: f32) -> (u64, bool) {
    if hardness < 0.0 {
        return (progress, false);
    }
    let progress = progress.saturating_add(energy);
    if progress as f64 >= f64::from(hardness) * ENERGY_PER_HARDNESS {
        (0, true)
    } else {
        (progress, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_damage() {
        assert_eq!(damage(5_000), (2.0, 2));
        assert_eq!(spent(2.0, 5_000), 5_000);
        assert_eq!(spent(10.0, 5_000), 5_000);
        let (min, max) = beam_box([0, 0, 0], [1, 0, 0], 3.0);
        assert!((min[0] - 1.001).abs() < 1e-9 && (max[0] - 4.001).abs() < 1e-9);
        assert_eq!((min[1], max[1]), (0.25, 0.75));
        let (min, max) = beam_box([0, 0, 0], [0, 0, -1], 1.0);
        assert!((max[2] + 0.001).abs() < 1e-9 && (min[2] + 1.001).abs() < 1e-9);
    }

    #[test]
    fn thresholds_pull_each_other() {
        assert_eq!(set_min_threshold(100, 1_000, 500), (500, 500));
        assert_eq!(set_min_threshold(100, 1_000, 50), (50, 100));
        assert_eq!(set_max_threshold(80, 1_000, 20), (20, 20));
        assert_eq!(set_max_threshold(0, 1_000, 5_000), (0, 1_000));
    }

    #[test]
    fn delay_counts_up_and_wraps() {
        let mut t = 0;
        let seen: Vec<u64> = (0..6).map(|_| { t = next_tick(t, 2); t }).collect();
        assert_eq!(seen, [1, 2, 0, 1, 2, 0]);
        assert_eq!(next_tick(0, 0), 0);
    }

    #[test]
    fn firing() {
        assert_eq!(to_fire(LASER, 80, 0, 0), 20);
        assert_eq!(to_fire(LASER, 19, 0, 0), 0);
        assert_eq!(to_fire(AMPLIFIER, 300, 0, RECEPTOR_STORAGE), 300);
        assert_eq!(to_fire(AMPLIFIER, 300, 1_000, RECEPTOR_STORAGE), 0);
        assert_eq!(to_fire(AMPLIFIER, 3_000, 1_000, 2_000), 2_000);
        assert_eq!(to_fire(TRACTOR_BEAM, 7, 0, 0), 7);
    }

    #[test]
    fn digging_stone() {
        // Stone (1.5) takes 150,000: one laser alone needs 7,500 ticks.
        let (p, broke) = dig(149_980, 20, 1.5);
        assert_eq!((p, broke), (0, true));
        assert_eq!(dig(0, 20, 1.5), (20, false));
        assert_eq!(dig(5, 1_000_000, -1.0), (5, false));
        assert_eq!(dig(0, 1, 0.0), (0, true));
    }
}
