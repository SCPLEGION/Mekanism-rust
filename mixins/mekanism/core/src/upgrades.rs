// Machine upgrades (mekanism.common.util.UpgradeUtils, IUpgradeTile): speed and energy upgrades change
// a machine's ticks per operation, energy per tick and storage.

/// `GeneralConfig.maxUpgradeMultiplier`'s default: a full set of one upgrade multiplies by this.
pub const MAX_UPGRADE_MULTIPLIER: f64 = 10.0;

/// `maxUpgradeMultiplier` of the open world's config (`MAX_UPGRADE_MULTIPLIER` is its default).
pub fn max_upgrade_multiplier() -> f64 {
    crate::config_files::active_int("general", "", "maxUpgradeMultiplier", MAX_UPGRADE_MULTIPLIER as u64) as f64
}

/// The upgrades this port installs, with their `max` from `data/mekanism/mekanism/upgrade/*.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upgrade {
    Speed,
    Energy,
    /// The Electric Pump's: water pumps as heavy water.
    Filter,
    /// The Digital Miner's: keeps the chunks it works in loaded.
    Anchor,
    /// The advanced machines': less chemical used.
    Chemical,
}

impl Upgrade {
    pub const ALL: [Upgrade; 5] = [Upgrade::Speed, Upgrade::Energy, Upgrade::Filter, Upgrade::Anchor, Upgrade::Chemical];

    /// The `mekanism:upgrade_type` value of the upgrade item.
    pub fn id(self) -> &'static str {
        match self {
            Upgrade::Speed => "mekanism:speed",
            Upgrade::Energy => "mekanism:energy",
            Upgrade::Filter => "mekanism:filter",
            Upgrade::Anchor => "mekanism:anchor",
            Upgrade::Chemical => "mekanism:chemical",
        }
    }

    pub fn name(self) -> &'static str {
        &self.id()["mekanism:".len()..]
    }

    pub fn max(self) -> u32 {
        match self {
            Upgrade::Filter | Upgrade::Anchor => 1,
            _ => 8,
        }
    }

    pub fn from_id(id: &str) -> Option<Upgrade> {
        Upgrade::ALL.into_iter().find(|u| u.id() == id)
    }
}

/// The upgrades installed in a machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Upgrades {
    pub speed: u32,
    pub energy: u32,
    pub filter: u32,
    pub anchor: u32,
    pub chemical: u32,
}

impl Upgrades {
    pub fn get(&self, u: Upgrade) -> u32 {
        match u {
            Upgrade::Speed => self.speed,
            Upgrade::Energy => self.energy,
            Upgrade::Filter => self.filter,
            Upgrade::Anchor => self.anchor,
            Upgrade::Chemical => self.chemical,
        }
    }

    /// Installs up to `count` of `u`; returns how many went in (`IUpgradeTile` stops at the max).
    pub fn install(&mut self, u: Upgrade, count: u32) -> u32 {
        let room = u.max() - self.get(u).min(u.max());
        let n = count.min(room);
        match u {
            Upgrade::Speed => self.speed += n,
            Upgrade::Energy => self.energy += n,
            Upgrade::Filter => self.filter += n,
            Upgrade::Anchor => self.anchor += n,
            Upgrade::Chemical => self.chemical += n,
        }
        n
    }

    /// `UpgradeUtils.fractionUpgrades`: how full of `u` the machine is.
    pub fn fraction_of(&self, u: Upgrade) -> f64 {
        self.fraction(u)
    }

    fn fraction(&self, u: Upgrade) -> f64 {
        f64::from(self.get(u).min(u.max())) / f64::from(u.max())
    }

    /// `UpgradeUtils.getTicksD`: the ticks an operation takes, not rounded.
    pub fn ticks_f64(&self, base: u32) -> f64 {
        f64::from(base) * max_upgrade_multiplier().powf(-self.fraction(Upgrade::Speed))
    }

    /// `UpgradeUtils.getTicks`: at least one tick.
    pub fn ticks(&self, base: u32) -> u32 {
        clamp_to_int(self.ticks_f64(base)).max(1) as u32
    }

    /// `UpgradeUtils.getEnergyPerTick`.
    pub fn energy_per_tick(&self, base: u64) -> u64 {
        let exp = 2.0 * self.fraction(Upgrade::Speed) - self.fraction(Upgrade::Energy);
        (base as f64 * max_upgrade_multiplier().powf(exp)).ceil() as u64
    }

    /// `UpgradeUtils.getGasPerTickMeanMultiplier` for a machine that takes chemical upgrades: how much more (or less)
    /// chemical an operation takes each tick than at the start, `maxUpgradeMultiplier ^ (2 speed - chemical)`.
    pub fn chemical_multiplier(&self) -> f64 {
        max_upgrade_multiplier().powf(2.0 * self.fraction(Upgrade::Speed) - self.fraction(Upgrade::Chemical))
    }

    /// `UpgradeUtils.getMaxEnergy`.
    pub fn max_energy(&self, base: u64) -> u64 {
        let e = base as f64 * max_upgrade_multiplier().powf(self.fraction(Upgrade::Energy));
        if e >= i64::MAX as f64 {
            i64::MAX as u64
        } else {
            e as u64
        }
    }
}

/// `StatUtils.inversePoisson(mean)`: a whole number of units drawn for a uniform `u` in [0, 1), about `mean` on average.
pub fn inverse_poisson(mean: f64, u: f64) -> u64 {
    let stirling_coeff = 1.0 / (2.0 * std::f64::consts::PI).sqrt();
    let r = u * mean.exp();
    let mut m = 0u64;
    let mut p = 1.0;
    let stirling_value = mean * std::f64::consts::E;
    let bound = 3.0 * mean.ceil();
    while p < r && (m as f64) < bound {
        m += 1;
        let mf = m as f64;
        p += stirling_coeff / mf.sqrt() * (stirling_value / mf).powf(mf);
    }
    m
}

/// `MathUtils.clampToInt(double)`.
fn clamp_to_int(v: f64) -> i32 {
    if v < f64::from(i32::MAX) {
        v as i32
    } else {
        i32::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_upgrades_change_nothing() {
        let u = Upgrades::default();
        assert_eq!(
            (u.ticks(200), u.energy_per_tick(50), u.max_energy(200)),
            (200, 50, 200)
        );
    }

    #[test]
    fn eight_speed_upgrades_make_it_ten_times_faster_at_a_hundred_times_the_power() {
        let u = Upgrades {
            speed: 8,
            energy: 0,
            filter: 0,
            anchor: 0,
            chemical: 0,
        };
        assert_eq!(u.ticks(200), 20);
        assert_eq!(u.energy_per_tick(50), 5000);
        // Four: 200 / 10^0.5 = 63.2 ticks, 50 * 10^1 per tick.
        let half = Upgrades {
            speed: 4,
            energy: 0,
            filter: 0,
            anchor: 0,
            chemical: 0,
        };
        assert_eq!((half.ticks(200), half.energy_per_tick(50)), (63, 500));
    }

    #[test]
    fn energy_upgrades_cut_usage_and_grow_storage() {
        let u = Upgrades {
            speed: 0,
            energy: 8,
            filter: 0,
            anchor: 0,
            chemical: 0,
        };
        assert_eq!((u.energy_per_tick(50), u.max_energy(200)), (5, 2000));
        let both = Upgrades {
            speed: 8,
            energy: 8,
            filter: 0,
            anchor: 0,
            chemical: 0,
        };
        assert_eq!(both.energy_per_tick(50), 500);
    }

    #[test]
    fn poisson_draws_average_the_mean() {
        // Evenly spaced uniforms stand in for the random draw.
        for mean in [1.0, 2.5, 10.0] {
            let n = 10_000;
            let total: u64 = (0..n).map(|i| inverse_poisson(mean, (f64::from(i) + 0.5) / f64::from(n))).sum();
            let average = total as f64 / f64::from(n);
            assert!((average - mean).abs() < mean * 0.25, "{mean}: {average}");
        }
    }

    #[test]
    fn installing_stops_at_the_max() {
        let mut u = Upgrades::default();
        assert_eq!(u.install(Upgrade::Speed, 5), 5);
        assert_eq!(u.install(Upgrade::Speed, 5), 3);
        assert_eq!(u.install(Upgrade::Speed, 1), 0);
        assert_eq!(Upgrade::from_id("mekanism:energy"), Some(Upgrade::Energy));
        assert_eq!(Upgrade::from_id("mekanism:anchor"), Some(Upgrade::Anchor));
        assert_eq!(Upgrade::from_id("mekanism:muffling"), None);
    }
}
