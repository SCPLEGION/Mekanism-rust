// Tiers (mekanism.api.tier.BaseTier and the per-block tier enums in mekanism.common.tier).

/// `BaseTier`: the four upgradeable tiers plus creative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BaseTier {
    Basic,
    Advanced,
    Elite,
    Ultimate,
    Creative,
}

impl BaseTier {
    pub const ALL: [BaseTier; 5] = [
        Self::Basic,
        Self::Advanced,
        Self::Elite,
        Self::Ultimate,
        Self::Creative,
    ];

    /// The lowercase name used in registry ids (`basic_energy_cube`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Advanced => "advanced",
            Self::Elite => "elite",
            Self::Ultimate => "ultimate",
            Self::Creative => "creative",
        }
    }

    /// The tier a tier installer or crafting upgrade moves to; none past ultimate.
    pub fn next(self) -> Option<BaseTier> {
        match self {
            Self::Basic => Some(Self::Advanced),
            Self::Advanced => Some(Self::Elite),
            Self::Elite => Some(Self::Ultimate),
            Self::Ultimate | Self::Creative => None,
        }
    }
}

/// `EnergyCubeTier`: capacity (FE) and output rate (FE/t) per tier.
pub fn energy_cube(tier: BaseTier) -> (u64, u64) {
    let (capacity, output) = energy_cube_default(tier);
    let name = tier.name();
    (
        active("tiers", "energy_cubes", &format!("{name}Capacity"), capacity),
        active("tiers", "energy_cubes", &format!("{name}Output"), output),
    )
}

/// `TierConfig`'s value of `key` (`tiers`, `machine-storage`, ...) in the open world, else the original's default.
fn active(file: &str, section: &str, key: &str, default: u64) -> u64 {
    crate::config_files::active_int(file, section, key, default)
}

fn energy_cube_default(tier: BaseTier) -> (u64, u64) {
    match tier {
        BaseTier::Basic => (4_000_000, 4_000),
        BaseTier::Advanced => (16_000_000, 16_000),
        BaseTier::Elite => (64_000_000, 64_000),
        BaseTier::Ultimate => (256_000_000, 256_000),
        BaseTier::Creative => (i64::MAX as u64, i32::MAX as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_upgrade_in_order_and_stop_at_ultimate() {
        assert_eq!(BaseTier::Basic.next(), Some(BaseTier::Advanced));
        assert_eq!(BaseTier::Ultimate.next(), None);
        assert_eq!(BaseTier::Creative.next(), None);
    }

    #[test]
    fn energy_cubes_quadruple_per_tier() {
        assert_eq!(energy_cube(BaseTier::Basic), (4_000_000, 4_000));
        assert_eq!(energy_cube(BaseTier::Ultimate), (256_000_000, 256_000));
    }
}

/// `CableTier`: what a universal cable of each tier carries in a tick (FE).
pub fn universal_cable(tier: BaseTier) -> Option<u64> {
    let default = match tier {
        BaseTier::Basic => 8_000,
        BaseTier::Advanced => 128_000,
        BaseTier::Elite => 1_024_000,
        BaseTier::Ultimate => 8_192_000,
        BaseTier::Creative => return None,
    };
    Some(active("tiers", "transmitters.energy", &format!("{}Capacity", tier.name()), default))
}

/// How much a cable holding `mine` hands a neighbouring cable holding `theirs`: half the difference,
/// so energy runs down the cable line and never back and forth.
pub fn cable_share(mine: u64, theirs: u64) -> u64 {
    mine.saturating_sub(theirs) / 2
}

/// `TubeTier`: a pressurized tube's transfer rate in mB a tick (750 to 256,000). A tube here holds one
/// tick of it, the way a cable holds one tick of energy.
pub fn pressurized_tube(tier: BaseTier) -> Option<u64> {
    let default = match tier {
        BaseTier::Basic => 750,
        BaseTier::Advanced => 2_000,
        BaseTier::Elite => 64_000,
        BaseTier::Ultimate => 256_000,
        BaseTier::Creative => return None,
    };
    Some(active("tiers", "transmitters.chemical", &format!("{}PullAmount", tier.name()), default))
}

/// `FluidTankTier`: a fluid tank's capacity and the mB a tick it gives out of its bottom.
pub fn fluid_tank(tier: BaseTier) -> (u64, u64) {
    let (capacity, output) = fluid_tank_default(tier);
    let name = tier.name();
    (
        active("tiers", "fluid_tanks", &format!("{name}Capacity"), capacity),
        active("tiers", "fluid_tanks", &format!("{name}Output"), output),
    )
}

fn fluid_tank_default(tier: BaseTier) -> (u64, u64) {
    match tier {
        BaseTier::Basic => (32_000, 1_000),
        BaseTier::Advanced => (64_000, 4_000),
        BaseTier::Elite => (128_000, 16_000),
        BaseTier::Ultimate => (256_000, 64_000),
        BaseTier::Creative => (i64::MAX as u64, i32::MAX as u64),
    }
}

/// `PipeTier`: the mB a mechanical pipe pulls in a tick (250 to 32,000). A pipe here holds one tick of
/// it, as a pressurized tube does.
pub fn mechanical_pipe(tier: BaseTier) -> Option<u64> {
    let default = match tier {
        BaseTier::Basic => 250,
        BaseTier::Advanced => 1_000,
        BaseTier::Elite => 8_000,
        BaseTier::Ultimate => 32_000,
        BaseTier::Creative => return None,
    };
    Some(active("tiers", "transmitters.fluid", &format!("{}PullAmount", tier.name()), default))
}

/// `TransporterTier`: items a logistical transporter pulls at a time (1, 16, 32, 64).
pub fn transporter_pull(tier: BaseTier) -> Option<u32> {
    let default = match tier {
        BaseTier::Basic => 1u32,
        BaseTier::Advanced => 16,
        BaseTier::Elite => 32,
        BaseTier::Ultimate => 64,
        BaseTier::Creative => return None,
    };
    Some(active("tiers", "transmitters.items", &format!("{}PullAmount", tier.name()), u64::from(default)).min(u64::from(u32::MAX)) as u32)
}

#[cfg(test)]
mod cable_tests {
    use super::*;

    #[test]
    fn cables_carry_their_tier_and_level_out() {
        assert_eq!(universal_cable(BaseTier::Elite), Some(1_024_000));
        assert_eq!(cable_share(100, 20), 40);
        assert_eq!(cable_share(20, 100), 0);
        assert_eq!(pressurized_tube(BaseTier::Advanced), Some(2_000));
        assert_eq!(transporter_pull(BaseTier::Elite), Some(32));
    }

    #[test]
    fn fluid_tiers_match_mekanism() {
        assert_eq!(fluid_tank(BaseTier::Basic), (32_000, 1_000));
        assert_eq!(fluid_tank(BaseTier::Ultimate), (256_000, 64_000));
        assert_eq!(mechanical_pipe(BaseTier::Elite), Some(8_000));
        assert_eq!(mechanical_pipe(BaseTier::Creative), None);
    }
}
