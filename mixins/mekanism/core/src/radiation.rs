// Radiation (`mekanism.common.lib.radiation`): sources that decay, the dose an entity takes from the level it stands in,
// and the scale, colours and display the Geiger Counter, Dosimeter and the HUD use.

use std::collections::HashMap;

/// `RadiationManager.BASELINE`: 100 nSv/h.
pub const BASELINE: f64 = 0.000_000_100;
/// `RadiationManager.MIN_MAGNITUDE`: 10 uSv/h.
pub const MIN_MAGNITUDE: f64 = 0.000_010;
/// `GeneralConfig.radiationSourceDecayRate` and `radiationTargetDecayRate`.
pub const SOURCE_DECAY_RATE: f64 = 0.9995;
pub const TARGET_DECAY_RATE: f64 = 0.9995;
/// `GeneralConfig.radiationNegativeEffectsMinSeverity`.
pub const NEGATIVE_EFFECTS_MIN_SEVERITY: f64 = 0.1;
/// `GeneralConfig.radiationChunkCheckRadius`, in blocks (a source is looked for within 5 chunks).
pub const MAX_BLOCK_RANGE: i32 = 5 * 16;

/// `RadiationScale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scale {
    None,
    Low,
    Medium,
    Elevated,
    High,
    Extreme,
}

impl Scale {
    /// `RadiationScale.get`: from a dose rate in Sv/h.
    pub fn get(magnitude: f64) -> Self {
        if magnitude < 0.00001 {
            Self::None
        } else if magnitude < 0.001 {
            Self::Low
        } else if magnitude < 0.1 {
            Self::Medium
        } else if magnitude < 10.0 {
            Self::Elevated
        } else if magnitude < 100.0 {
            Self::High
        } else {
            Self::Extreme
        }
    }

    /// The Geiger sound event of the scale (`getSoundEvent`); none for no radiation.
    pub fn sound(self) -> Option<&'static str> {
        Some(match self {
            Self::None => return None,
            Self::Low => "mekanism:item.geiger_slow",
            Self::Medium => "mekanism:item.geiger_medium",
            Self::Elevated | Self::High => "mekanism:item.geiger_elevated",
            Self::Extreme => "mekanism:item.geiger_fast",
        })
    }
}

/// `RadiationScale.getSeverityColor`, as `EnumColor` RGB.
pub fn severity_color(magnitude: f64) -> u32 {
    if magnitude <= BASELINE {
        0x75FF89
    } else if magnitude < 0.00001 {
        0xCFCFCF
    } else if magnitude < 0.001 {
        0xFFDD4F
    } else if magnitude < 0.1 {
        0xFFA160
    } else if magnitude < 10.0 {
        0xFF383C
    } else {
        0xC9071F
    }
}

/// `RadiationScale.getScaledDoseSeverity`: a dose in Sv as 0 to 1 on a log scale from 10 uSv to 100 Sv.
pub fn scaled_dose_severity(magnitude: f64) -> f64 {
    if magnitude < MIN_MAGNITUDE {
        return 0.0;
    }
    let log_baseline = MIN_MAGNITUDE.log10();
    let scale = 100f64.log10() - log_baseline;
    ((-log_baseline + magnitude.log10()) / scale).clamp(0.0, 1.0)
}

/// `RadiationUtil.computeExposure`: the magnitude over the squared distance (at least 1).
pub fn exposure(magnitude: f64, distance_sqr: i64) -> f64 {
    magnitude / distance_sqr.max(1) as f64
}

/// `RadiationUtil.getDecayTime`: ticks until `magnitude` decays below the minimum, a second at a time.
pub fn decay_ticks(magnitude: f64, rate: f64) -> u64 {
    let mut ticks = 0;
    let mut magnitude = magnitude;
    while magnitude > MIN_MAGNITUDE && ticks < 20 * 3600 * 24 * 365 {
        magnitude *= rate;
        ticks += 20;
    }
    ticks
}

/// The shielding of a worn hazmat piece (`ItemHazmatSuitArmor.getShieldingByArmor`), by armor slot 0 head to 3 feet.
pub fn shielding(slot: usize) -> f64 {
    [0.25, 0.4, 0.2, 0.15][slot.min(3)]
}

/// `RadiationManager.radiate(entity, ..)`: the share of a dose that gets through the armor's shielding.
pub fn through_shielding(resistance: f64) -> f64 {
    1.0 - resistance.min(1.0)
}

/// `UnitDisplayUtils.getDisplayShort(value, RadiationUnit, decimals)`: an SI prefix from femto to yotta, the number
/// cut (not rounded) to `decimals`, then a space and the unit.
pub fn display(value: f64, unit: &str, decimals: i32) -> String {
    if value == 0.0 {
        return format!("0 {unit}");
    }
    const PREFIXES: [(&str, f64); 14] = [
        ("f", 1e-15),
        ("p", 1e-12),
        ("n", 1e-9),
        ("µ", 1e-6),
        ("m", 1e-3),
        ("", 1.0),
        ("k", 1e3),
        ("M", 1e6),
        ("G", 1e9),
        ("T", 1e12),
        ("P", 1e15),
        ("E", 1e18),
        ("Z", 1e21),
        ("Y", 1e24),
    ];
    let negative = value < 0.0;
    let value = value.abs();
    let mut chosen = PREFIXES.len() - 1;
    for i in 0..PREFIXES.len() {
        if (i == 0 && value < PREFIXES[0].1) || i + 1 >= PREFIXES.len() || (value >= PREFIXES[i].1 && value < PREFIXES[i + 1].1) {
            chosen = i;
            break;
        }
    }
    let (symbol, scale) = PREFIXES[chosen];
    let multiplier = 10f64.powi(decimals);
    let mut shown = ((value / scale) * multiplier).trunc() / multiplier;
    if negative {
        shown = -shown;
    }
    let mut text = format!("{shown}");
    if text.ends_with(".0") {
        text.truncate(text.len() - 2);
    }
    format!("{text} {symbol}{unit}")
}

/// A level's radiation sources (`RadiationLevelData`): a dose rate on a block that decays.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    map: HashMap<[i32; 3], f64>,
}

impl Sources {
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `radiate(pos, magnitude)`: adds to the source on the block, or starts one.
    pub fn radiate(&mut self, pos: [i32; 3], magnitude: f64) {
        *self.map.entry(pos).or_insert(0.0) += magnitude;
    }

    /// `getRadiationLevelAndMaxMagnitude`: the level at `pos` (the baseline and every source in range, each over the
    /// squared distance) and the largest magnitude of those sources.
    pub fn level_at(&self, pos: [i32; 3]) -> (f64, f64) {
        let mut level = BASELINE;
        let mut max = BASELINE;
        for (at, magnitude) in &self.map {
            if (0..3).any(|i| (at[i] - pos[i]).abs() > MAX_BLOCK_RANGE) {
                continue;
            }
            let d: i64 = (0..3).map(|i| i64::from(at[i] - pos[i]).pow(2)).sum();
            level += exposure(*magnitude, d);
            max = max.max(*magnitude);
        }
        (level, max)
    }

    /// `decay`: every source loses a little; the ones below the minimum go. True when any source changed.
    pub fn decay(&mut self) -> bool {
        for magnitude in self.map.values_mut() {
            *magnitude *= SOURCE_DECAY_RATE;
        }
        self.map.retain(|_, m| *m >= MIN_MAGNITUDE);
        !self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = ([i32; 3], f64)> + '_ {
        self.map.iter().map(|(p, m)| (*p, *m))
    }

    pub fn remove_at(&mut self, pos: [i32; 3]) -> bool {
        self.map.remove(&pos).is_some()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

/// `TextUtils.getHoursMinutes` at 20 ticks per second: "1h 5m", or "5m" under an hour.
pub fn hours_minutes(ticks: u64) -> String {
    let minutes = ticks.div_ceil(60 * 20);
    if minutes >= 60 {
        format!("{}h {}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

/// `PlayerExposure.PreviousRadiationData`: what was last sent to a player, so a level is sent again only when the
/// digits it shows would change.
#[derive(Clone, Copy, Debug)]
pub struct Previous {
    magnitude: f64,
    power: i32,
    base: f64,
}

impl Previous {
    fn new(magnitude: f64) -> Self {
        let power = magnitude.log10().floor() as i32;
        // SI units go in factors of 1000; two decimals are shown.
        let si_power = power.div_euclid(3) * 3;
        Self { magnitude, power, base: 10f64.powi(si_power - 2) }
    }

    /// `compareTo`: the data to remember and send, or none when the player already has this level.
    pub fn compare(previous: Option<Previous>, magnitude: f64) -> Option<Previous> {
        match previous {
            None => Some(Self::new(magnitude)),
            Some(p) if (magnitude - p.magnitude).abs() >= p.base => Some(Self::new(magnitude)),
            Some(p) if magnitude < p.magnitude && (magnitude.log10().floor() as i32) < p.power => Some(Self::new(magnitude)),
            Some(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_follows_the_dose_rate() {
        assert_eq!(Scale::get(0.000_001), Scale::None);
        assert_eq!(Scale::get(0.0005), Scale::Low);
        assert_eq!(Scale::get(0.05), Scale::Medium);
        assert_eq!(Scale::get(5.0), Scale::Elevated);
        assert_eq!(Scale::get(50.0), Scale::High);
        assert_eq!(Scale::get(500.0), Scale::Extreme);
        assert_eq!(Scale::None.sound(), None);
    }

    #[test]
    fn severity_is_logarithmic_between_ten_micro_and_a_hundred_sieverts() {
        assert_eq!(scaled_dose_severity(0.000_001), 0.0);
        assert!((scaled_dose_severity(MIN_MAGNITUDE)).abs() < 1e-12);
        assert!((scaled_dose_severity(100.0) - 1.0).abs() < 1e-12);
        assert!((scaled_dose_severity(0.01) - 3.0 / 7.0).abs() < 1e-9);
    }

    #[test]
    fn displays_with_si_prefixes() {
        assert_eq!(display(0.0, "Sv", 2), "0 Sv");
        // As Java: 1e-7 / 1e-9 is 99.99999999999999, and the cut is a truncation.
        assert_eq!(display(BASELINE, "Sv/h", 2), "99.99 nSv/h");
        assert_eq!(display(0.012_345, "Sv", 2), "12.34 mSv");
        assert_eq!(display(3.0, "Sv", 3), "3 Sv");
        assert_eq!(display(1500.0, "Sv/h", 2), "1.5 kSv/h");
    }

    #[test]
    fn sources_add_up_over_distance_and_decay_away() {
        let mut s = Sources::default();
        s.radiate([0, 0, 0], 1.0);
        s.radiate([0, 0, 0], 1.0);
        let (level, max) = s.level_at([2, 0, 0]);
        assert!((level - (BASELINE + 2.0 / 4.0)).abs() < 1e-12);
        assert_eq!(max, 2.0);
        assert_eq!(s.level_at([100, 0, 0]), (BASELINE, BASELINE));
        let mut ticks = 0;
        while s.decay() {
            ticks += 1;
        }
        assert!(ticks > 10_000 && s.is_empty());
    }

    #[test]
    fn decay_time_counts_seconds_in_ticks() {
        assert_eq!(decay_ticks(MIN_MAGNITUDE, SOURCE_DECAY_RATE), 0);
        assert!(decay_ticks(1.0, SOURCE_DECAY_RATE) > 20 * 10_000);
    }

    #[test]
    fn hazmat_shields_in_pieces() {
        let total: f64 = (0..4).map(shielding).sum();
        assert!((total - 1.0).abs() < 1e-12);
        assert_eq!(through_shielding(total), 0.0);
        assert!((through_shielding(0.25) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn levels_are_sent_when_the_shown_digits_change() {
        let first = Previous::compare(None, 1e-7).unwrap();
        assert!(Previous::compare(Some(first), 1e-7).is_none());
        assert!(Previous::compare(Some(first), 1e-3).is_some());
        let big = Previous::compare(None, 5e-3).unwrap();
        assert!(Previous::compare(Some(big), 5.000_000_1e-3).is_none());
        assert!(Previous::compare(Some(big), 9e-4).is_some());
    }

    #[test]
    fn hours_and_minutes() {
        assert_eq!(hours_minutes(20 * 60 * 5), "5m");
        assert_eq!(hours_minutes(20 * 60 * 65), "1h 5m");
        assert_eq!(hours_minutes(1), "1m");
    }
}
