// The Fission Reactor (mekanism.generators.common.content.fission), simplified: a box of casing and
// reactor glass with ports in its walls and fuel assemblies inside. It burns fissile fuel at a set rate,
// turns the heat into steam from its water and leaves nuclear waste.

pub const MIN_SIZE: i32 = 3;
pub const MAX_SIZE: i32 = 18;
/// `GeneratorsConfig.maxFuelPerAssembly` (8 buckets) and `burnPerAssembly` (1 mB a tick).
pub const FUEL_PER_ASSEMBLY: u64 = 8_000;
/// `cooledCoolantPerTank` and `heatedCoolantPerTank`: buckets per block of the reactor's volume.
pub const WATER_PER_BLOCK: u64 = 100_000;
pub const STEAM_PER_BLOCK: u64 = 1_000_000;
/// `energyPerFissionFuel` (1 MJ a mB) over water's enthalpy as coolant (`maxEnergyPerSteam` / steam
/// efficiency = 50 J a mB): 20,000 mB of water boils for each mB of fuel, here per tenth of a mB.
pub const STEAM_PER_TENTH: u64 = 2_000;
/// `defaultBurnRate`, in tenths of a mB a tick.
pub const DEFAULT_BURN_TENTHS: u64 = 1;
/// The burn rates a click on a port steps through, in tenths of a mB a tick, up to the reactor's maximum.
pub const BURN_STEPS: [u64; 7] = [1, 5, 10, 20, 50, 100, 200];

pub fn is_frame(name: &str) -> bool {
    matches!(
        name,
        "mekanismgenerators:fission_reactor_casing"
            | "mekanismgenerators:reactor_glass"
            | "mekanismgenerators:fission_reactor_port"
            | "mekanismgenerators:fission_reactor_logic_adapter"
    )
}

/// What a formed reactor holds inside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reactor {
    pub assemblies: u64,
    pub volume: u64,
    /// The fuel assemblies' surface area, every exposed face (`surfaceArea`).
    pub surface: u64,
    /// How many blocks the structure's shell holds (`locations`), which the heat capacity follows.
    pub locations: u64,
}

impl Reactor {
    /// Counts an inner block; false for one that may not be inside.
    pub fn add_inside(&mut self, name: &str) -> bool {
        match name {
            "mekanismgenerators:fission_fuel_assembly" => self.assemblies += 1,
            "mekanismgenerators:control_rod_assembly" | "minecraft:air" | "minecraft:cave_air" => {}
            _ => return false,
        }
        true
    }

    pub fn fuel_capacity(&self) -> u64 {
        self.assemblies * FUEL_PER_ASSEMBLY
    }

    pub fn water_capacity(&self) -> u64 {
        self.volume * WATER_PER_BLOCK
    }

    pub fn steam_capacity(&self) -> u64 {
        self.volume * STEAM_PER_BLOCK
    }

    /// `getMaxBurnRate`, in tenths.
    pub fn max_burn_tenths(&self) -> u64 {
        self.assemblies * 10
    }
}

/// The next burn rate after `now`, wrapping to the lowest past the reactor's maximum.
pub fn next_burn(now: u64, max: u64) -> u64 {
    BURN_STEPS
        .iter()
        .copied()
        .find(|&s| s > now && s <= max)
        .unwrap_or(DEFAULT_BURN_TENTHS.min(max.max(1)))
}

/// One tick's burn: tenths burned, mB of fuel taken (whole mB, as `partial` tenths add up), water
/// boiled and waste made. None when the water cannot take the heat: the reactor stops (SCRAM) instead
/// of heating up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burn {
    pub fuel: u64,
    pub steam: u64,
    pub waste: u64,
}

pub fn burn(
    rate_tenths: u64,
    partial: &mut u64,
    fuel: u64,
    water: u64,
    steam_room: u64,
    waste_room: u64,
) -> Option<Burn> {
    let tenths = rate_tenths.min(fuel * 10 + *partial);
    if tenths == 0 {
        return Some(Burn {
            fuel: 0,
            steam: 0,
            waste: 0,
        });
    }
    let steam = tenths * STEAM_PER_TENTH;
    if water < steam || steam_room < steam {
        return None;
    }
    // Take whole mB of fuel as the tenths need them, keeping the rest for later ticks.
    let need = tenths.saturating_sub(*partial);
    let taken = need.div_ceil(10);
    *partial = *partial + taken * 10 - tenths;
    if waste_room < taken {
        return None;
    }
    Some(Burn {
        fuel: taken,
        steam,
        waste: taken,
    })
}

/// `FissionReactorMultiblockData`'s heat and damage constants.
pub const MIN_DAMAGE_TEMPERATURE: f64 = 1_200.0;
pub const MAX_DAMAGE_TEMPERATURE: f64 = 1_800.0;
pub const MAX_DAMAGE: f64 = 100.0;
/// `fissionCasingHeatCapacity`: what each casing block adds to the reactor's heat capacity.
pub const CASING_HEAT_CAPACITY: f64 = 1_000.0;
const INVERSE_INSULATION: f64 = 10_000.0;
const INVERSE_CONDUCTION: f64 = 10.0;
/// `waterConductivity`.
pub const WATER_CONDUCTIVITY: f64 = 0.5;
/// `energyPerFissionFuel`: the heat of one whole mB of fuel.
pub const ENERGY_PER_FUEL: f64 = 1_000_000.0;
/// `fissionSurfaceAreaTarget`.
pub const SURFACE_AREA_TARGET: f64 = 4.0;
/// `defaultBurnRate`.
pub const DEFAULT_RATE_LIMIT: f64 = 0.1;
/// `HeatUtils.BASE_BOIL_TEMP`: 100 C in kelvin.
pub const BASE_BOIL_TEMP: f64 = 373.15;
/// Heat a mB of water takes to turn to steam (`getWaterThermalEnthalpy / getSteamEnergyEfficiency`).
pub const COOLANT_ENTHALPY: f64 = 10.0 / 0.2;
/// `fissionPostMeltdownDamage`: 75 percent of `MAX_DAMAGE`.
pub const POST_MELTDOWN_DAMAGE: f64 = 0.75 * MAX_DAMAGE;
/// `fissionMeltdownChance`.
pub const MELTDOWN_CHANCE: f64 = 0.001;
/// `fissionMeltdownRadius`.
pub const MELTDOWN_RADIUS: f32 = 8.0;
/// `fissionMeltdownsEnabled`.
pub const MELTDOWNS_ENABLED: bool = true;
/// `EXPLOSION_CHANCE` of the meltdown.
pub const EXPLOSION_CHANCE: f64 = 1.0 / 512_000.0;

/// What one tick of the damage check decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    None,
    /// The reactor melts down: the explosions begin and `meltdown_reset` follows.
    Meltdown,
}

/// A reactor's heat, damage and burn bookkeeping (the fields of `FissionReactorMultiblockData` that
/// are not tanks).
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub heat: f64,
    pub capacity: f64,
    pub ambient: f64,
    pub damage: f64,
    pub burn_remaining: f64,
    pub partial_waste: f64,
    pub rate_limit: f64,
    pub active: bool,
    pub force_disable: bool,
    pub last_burn: f64,
    pub last_boil: u64,
    pub env_loss: f64,
}

impl State {
    /// A new reactor sits at the ambient temperature.
    pub fn new(capacity: f64, ambient: f64) -> State {
        State {
            heat: capacity * ambient,
            capacity,
            ambient,
            damage: 0.0,
            burn_remaining: 0.0,
            partial_waste: 0.0,
            rate_limit: DEFAULT_RATE_LIMIT,
            active: false,
            force_disable: false,
            last_burn: 0.0,
            last_boil: 0,
            env_loss: 0.0,
        }
    }

    pub fn temperature(&self) -> f64 {
        if self.capacity > 0.0 { self.heat / self.capacity } else { self.ambient }
    }

    /// `setActive`: not while forcibly disabled.
    pub fn set_active(&mut self, active: bool) {
        if self.active != active && (!active || !self.force_disable) {
            self.active = active;
        }
    }

    /// `setRateLimit`: clamped to 0 up to the maximum burn rate.
    pub fn set_rate_limit(&mut self, rate: f64, max_burn: u64) {
        self.rate_limit = rate.clamp(0.0, max_burn as f64);
    }

    /// `burnFuel`: burns what the rate limit allows, heats the reactor and makes waste (what does not fit
    /// in the waste tank is lost). Without fuel burning or while off, the last burn rate is 0.
    pub fn burn(&mut self, fuel: &mut u64, fuel_capacity: u64, max_burn: u64, waste: &mut u64, waste_capacity: u64) {
        if !self.active {
            self.last_burn = 0.0;
            return;
        }
        let stored = *fuel as f64 + self.burn_remaining;
        let to_burn = self.rate_limit.min(stored).min(max_burn as f64);
        let stored = stored - to_burn;
        *fuel = (stored as u64).min(fuel_capacity);
        self.burn_remaining = stored % 1.0;
        self.heat += to_burn * ENERGY_PER_FUEL;
        self.partial_waste += to_burn;
        let new_waste = self.partial_waste.floor() as u64;
        if new_waste > 0 {
            self.partial_waste %= 1.0;
            *waste += new_waste.min(waste_capacity.saturating_sub(*waste));
        }
        self.last_burn = to_burn;
    }

    /// `handleCoolant` for water: the heat above boiling, by the boil efficiency and the water's
    /// conductivity, turns water to steam; the steam the tank has no room for is vented.
    pub fn coolant(&mut self, water: &mut u64, steam: &mut u64, steam_capacity: u64, efficiency: f64) {
        if *water == 0 {
            self.last_boil = 0;
            return;
        }
        let heat = efficiency * (self.heat - BASE_BOIL_TEMP * self.capacity);
        let heated = heat * WATER_CONDUCTIVITY / COOLANT_ENTHALPY;
        let stored = (*water).min(i32::MAX as u64) as i64;
        let boil = i64::from(heated as i32).clamp(0, stored) as u64;
        self.last_boil = boil;
        if boil > 0 {
            *water -= boil;
            *steam += boil.min(steam_capacity.saturating_sub(*steam));
            self.heat -= boil as f64 * COOLANT_ENTHALPY;
        }
    }

    /// `simulateEnvironment`: the casing gives heat to the air.
    pub fn environment(&mut self) {
        let inverse = crate::heat::AIR_INVERSE_COEFFICIENT + (INVERSE_INSULATION + INVERSE_CONDUCTION);
        let transfer = (self.temperature() - self.ambient) / inverse;
        self.heat -= transfer * self.capacity;
        self.env_loss = transfer.max(0.0);
    }

    /// `handleDamage`: above 1,200 K the reactor takes damage, below it repairs; past 100 percent damage
    /// and still hot, `roll` (a uniform 0 to 1) decides a meltdown.
    pub fn damage_step(&mut self, roll: f64) -> Damage {
        let temp = self.temperature();
        if temp > MIN_DAMAGE_TEMPERATURE {
            self.damage += temp.min(MAX_DAMAGE_TEMPERATURE) / (MIN_DAMAGE_TEMPERATURE * 10.0);
        } else {
            self.damage = (self.damage - (MIN_DAMAGE_TEMPERATURE - temp) / (MIN_DAMAGE_TEMPERATURE * 100.0)).max(0.0);
        }
        if self.damage >= MAX_DAMAGE && temp >= MIN_DAMAGE_TEMPERATURE {
            if self.force_disable && MELTDOWNS_ENABLED {
                self.force_disable = false;
                return Damage::Meltdown;
            }
            if roll < (self.damage / MAX_DAMAGE) * MELTDOWN_CHANCE {
                if MELTDOWNS_ENABLED {
                    return Damage::Meltdown;
                }
                self.force_disable = true;
                self.active = false;
            }
        } else if self.damage < MAX_DAMAGE && temp < MIN_DAMAGE_TEMPERATURE {
            self.force_disable = false;
        }
        Damage::None
    }

    /// `meltdownHappened`: the reactor is off, damaged to the post-meltdown level and back to ambient.
    pub fn meltdown_reset(&mut self) {
        self.active = false;
        self.damage = POST_MELTDOWN_DAMAGE;
        self.burn_remaining = 0.0;
        self.partial_waste = 0.0;
        self.heat = (self.capacity * self.ambient).max(0.0);
    }
}

/// `getBoilEfficiency`: the fuel assemblies' average surface area over the target, at most 1.
pub fn boil_efficiency(surface: u64, assemblies: u64) -> f64 {
    if assemblies == 0 {
        return 0.0;
    }
    (surface as f64 / assemblies as f64 / SURFACE_AREA_TARGET).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tenth_a_tick_takes_a_mb_every_ten_ticks() {
        let mut partial = 0;
        let mut fuel = 0;
        for _ in 0..10 {
            let b = burn(1, &mut partial, 5, 1_000_000, 1_000_000, 1_000).unwrap();
            assert_eq!(b.steam, 2_000);
            fuel += b.fuel;
        }
        assert_eq!((fuel, partial), (1, 0));
    }

    #[test]
    fn no_water_stops_the_reactor() {
        let mut partial = 0;
        assert_eq!(burn(10, &mut partial, 5, 19_999, u64::MAX, 100), None);
        assert_eq!(
            burn(10, &mut partial, 0, 0, 0, 0),
            Some(Burn {
                fuel: 0,
                steam: 0,
                waste: 0
            })
        );
    }

    #[test]
    fn reactors_count_assemblies_and_step_rates() {
        let mut r = Reactor {
            volume: 27,
            ..Default::default()
        };
        assert!(r.add_inside("mekanismgenerators:fission_fuel_assembly"));
        assert!(!r.add_inside("minecraft:stone"));
        assert_eq!((r.fuel_capacity(), r.max_burn_tenths()), (8_000, 10));
        assert_eq!(next_burn(1, 10), 5);
        assert_eq!(next_burn(10, 10), 1);
    }

    #[test]
    fn a_tenth_of_fuel_heats_and_boils_as_in_the_original() {
        let mut st = State::new(98_000.0, 300.0);
        st.active = true;
        let (mut fuel, mut waste, mut water, mut steam) = (1_000u64, 0u64, 1_000_000u64, 0u64);
        st.burn(&mut fuel, 8_000, 1, &mut waste, 8_000);
        assert!((st.last_burn - 0.1).abs() < 1e-12);
        assert!((st.heat - (98_000.0 * 300.0 + 100_000.0)).abs() < 1e-6);
        st.heat = 98_000.0 * 374.15;
        st.coolant(&mut water, &mut steam, 10_000_000, 1.0);
        // 98,000 heat over boiling, half of it to the water, 50 per mB.
        assert_eq!(st.last_boil, 980);
        assert_eq!((water, steam), (999_020, 980));
    }

    #[test]
    fn damage_builds_when_hot_and_repairs_when_cool() {
        let mut st = State::new(1_000.0, 300.0);
        st.heat = 1_000.0 * 1_500.0;
        assert_eq!(st.damage_step(1.0), Damage::None);
        assert!((st.damage - 1_500.0 / 12_000.0).abs() < 1e-12);
        st.heat = 1_000.0 * 300.0;
        st.damage_step(1.0);
        assert!(st.damage < 1_500.0 / 12_000.0);
        st.damage = 100.0;
        st.heat = 1_000.0 * 1_800.0;
        assert_eq!(st.damage_step(0.0), Damage::Meltdown);
        st.meltdown_reset();
        assert_eq!((st.damage, st.active), (75.0, false));
    }

    #[test]
    fn efficiency_follows_the_surface_area() {
        assert_eq!(boil_efficiency(24, 4), 1.0);
        assert_eq!(boil_efficiency(8, 4), 0.5);
        assert_eq!(boil_efficiency(0, 0), 0.0);
    }
}
