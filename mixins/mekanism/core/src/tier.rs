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
    match tier {
        BaseTier::Basic => Some(8_000),
        BaseTier::Advanced => Some(128_000),
        BaseTier::Elite => Some(1_024_000),
        BaseTier::Ultimate => Some(8_192_000),
        BaseTier::Creative => None,
    }
}

/// How much a cable holding `mine` hands a neighbouring cable holding `theirs`: half the difference,
/// so energy runs down the cable line and never back and forth.
pub fn cable_share(mine: u64, theirs: u64) -> u64 {
    mine.saturating_sub(theirs) / 2
}

#[cfg(test)]
mod cable_tests {
    use super::*;

    #[test]
    fn cables_carry_their_tier_and_level_out() {
        assert_eq!(universal_cable(BaseTier::Elite), Some(1_024_000));
        assert_eq!(cable_share(100, 20), 40);
        assert_eq!(cable_share(20, 100), 0);
    }
}
