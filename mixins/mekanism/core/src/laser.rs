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
