// Chemicals (mekanism.api.chemical): tanks holding one chemical, the Gas-Burning Generator's fuel, and
// the Electrolytic Separator's electrolysis of water.

/// A tank of one chemical at a time (`BasicChemicalTank`), in mB.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChemicalTank {
    pub chemical: Option<String>,
    pub amount: u64,
    pub capacity: u64,
}

impl ChemicalTank {
    pub fn new(capacity: u64) -> Self {
        Self {
            chemical: None,
            amount: 0,
            capacity,
        }
    }

    /// Whether `chemical` may go in: the tank is empty or holds it already.
    pub fn accepts(&self, chemical: &str) -> bool {
        self.amount == 0 || self.chemical.as_deref() == Some(chemical)
    }

    /// Puts up to `amount` of `chemical` in; returns how much went in.
    pub fn insert(&mut self, chemical: &str, amount: u64) -> u64 {
        if !self.accepts(chemical) {
            return 0;
        }
        let n = amount.min(self.capacity.saturating_sub(self.amount));
        if n > 0 {
            self.chemical = Some(chemical.to_owned());
            self.amount += n;
        }
        n
    }

    /// Takes up to `amount` out; returns how much came out.
    pub fn extract(&mut self, amount: u64) -> u64 {
        let n = amount.min(self.amount);
        self.amount -= n;
        if self.amount == 0 {
            self.chemical = None;
        }
        n
    }
}

/// `chemical_attribute_fuel.json`: (FE per mB, max mB burnt a tick).
pub fn fuel(chemical: &str) -> Option<(u64, u64)> {
    match chemical {
        "mekanism:hydrogen" => Some((2, 10)),
        "mekanism:ethene" => Some((8, 10)),
        _ => None,
    }
}

/// `GeneratorsConfig.gbgTankCapacity` and the generator's storage (1000 mB of hydrogen's worth).
pub const GAS_GENERATOR_TANK: u64 = 1_000;
pub const GAS_GENERATOR_STORAGE: u64 = 1_000 * 2;

/// One tick of `TileEntityGasGenerator`: burns more the fuller the tank is (`maxBurnPerTick` times the
/// fill), as much as `room` FE allows, rounding the fuel used up. Returns (FE made, mB burnt).
pub fn burn_fuel(tank: &mut ChemicalTank, room: u64) -> (u64, u64) {
    let Some((density, max_burn)) = tank.chemical.as_deref().and_then(fuel) else {
        return (0, 0);
    };
    if tank.amount == 0 {
        return (0, 0);
    }
    let fullness = tank.amount as f64 / tank.capacity.max(1) as f64;
    let burn = ((max_burn as f64 * fullness).ceil() as u64).min(tank.amount);
    let made = (density * burn).min(room);
    if made == 0 {
        return (0, 0);
    }
    let used = made.div_ceil(density);
    tank.extract(used);
    (made, used)
}

/// `TileEntityElectrolyticSeparator`: its tanks, and what one operation on water takes and makes.
pub const SEPARATOR_FLUID: u64 = 24_000;
pub const SEPARATOR_GAS: u64 = 2_400;
/// Energy per operation: twice hydrogen's energy density.
pub const SEPARATOR_USAGE: u64 = 4;
pub const WATER_PER_OPERATION: u64 = 2;
pub const HYDROGEN_PER_OPERATION: u64 = 2;
pub const OXYGEN_PER_OPERATION: u64 = 1;

/// One separator tick on water: one operation when there is water, room for both gases and energy.
/// Returns whether it ran.
pub fn separate_water(
    water: &mut u64,
    hydrogen: &mut ChemicalTank,
    oxygen: &mut ChemicalTank,
    energy: &mut crate::energy::EnergyContainer,
) -> bool {
    let fits = |t: &ChemicalTank, c: &str, n: u64| t.accepts(c) && t.amount + n <= t.capacity;
    if *water < WATER_PER_OPERATION
        || !fits(hydrogen, "mekanism:hydrogen", HYDROGEN_PER_OPERATION)
        || !fits(oxygen, "mekanism:oxygen", OXYGEN_PER_OPERATION)
        || !energy.consume(SEPARATOR_USAGE)
    {
        return false;
    }
    *water -= WATER_PER_OPERATION;
    hydrogen.insert("mekanism:hydrogen", HYDROGEN_PER_OPERATION);
    oxygen.insert("mekanism:oxygen", OXYGEN_PER_OPERATION);
    true
}

/// `ChemicalTankTier`: (capacity, output rate) in mB.
pub fn chemical_tank(tier: crate::tier::BaseTier) -> (u64, u64) {
    use crate::tier::BaseTier::*;
    match tier {
        Basic => (64_000, 1_000),
        Advanced => (256_000, 16_000),
        Elite => (1_024_000, 128_000),
        Ultimate => (8_192_000, 512_000),
        Creative => (u64::MAX / 2, i32::MAX as u64),
    }
}

/// `TileEntityChemicalOxidizer`: 15 FE a tick for 100 ticks, into a 10,000 mB tank.
pub const OXIDIZER_USAGE: u64 = 15;
pub const OXIDIZER_TICKS: u32 = 100;
pub const OXIDIZER_TANK: u64 = 10_000;

/// An advanced electric machine (`TileEntityAdvancedElectricMachine`): an item and a chemical used each
/// tick, 200 ticks, a 210 mB tank (`MAX_GAS`); its energy per tick is `UsageConfig`'s.
#[derive(Debug, PartialEq, Eq)]
pub struct AdvancedMachine {
    pub block: &'static str,
    pub usage: u64,
    pub chemical: &'static str,
    pub table: &'static [crate::recipes::ItemChemicalRecipe],
}

pub const ADVANCED_TICKS: u32 = 200;
pub const ADVANCED_TANK: u64 = 210;

pub static ADVANCED_MACHINES: &[AdvancedMachine] = &[
    AdvancedMachine {
        block: "mekanism:purification_chamber",
        usage: 10,
        chemical: "mekanism:oxygen",
        table: crate::recipe_table::PURIFYING,
    },
    AdvancedMachine {
        block: "mekanism:osmium_compressor",
        usage: 15,
        chemical: "mekanism:osmium",
        table: crate::recipe_table::COMPRESSING,
    },
];

/// `TileEntityChemicalInfuser`: 15 FE a tick, three 10,000 mB tanks.
pub const CHEMICAL_INFUSER_USAGE: u64 = 15;
pub const CHEMICAL_INFUSER_TANK: u64 = 10_000;

/// `TileEntityMetallurgicInfuser`: 5 FE a tick, 200 ticks, a 1,000 mB infuse tank.
pub const INFUSER_USAGE: u64 = 5;
pub const INFUSER_TICKS: u32 = 200;
pub const INFUSER_TANK: u64 = 1_000;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::EnergyContainer;

    #[test]
    fn a_tank_holds_one_chemical() {
        let mut t = ChemicalTank::new(100);
        assert_eq!(t.insert("mekanism:hydrogen", 80), 80);
        assert_eq!(t.insert("mekanism:oxygen", 10), 0);
        assert_eq!(t.insert("mekanism:hydrogen", 80), 20);
        assert_eq!(t.extract(150), 100);
        assert_eq!(t.chemical, None);
    }

    #[test]
    fn the_gas_generator_burns_more_when_full() {
        let mut t = ChemicalTank::new(GAS_GENERATOR_TANK);
        t.insert("mekanism:hydrogen", 1_000);
        assert_eq!(burn_fuel(&mut t, 1_000), (20, 10));
        let mut low = ChemicalTank::new(GAS_GENERATOR_TANK);
        low.insert("mekanism:hydrogen", 100);
        assert_eq!(burn_fuel(&mut low, 1_000), (2, 1));
        // A full energy store takes only what fits, and the fuel for it is rounded up.
        assert_eq!(burn_fuel(&mut t, 3), (3, 2));
        let mut oxygen = ChemicalTank::new(10);
        oxygen.insert("mekanism:oxygen", 10);
        assert_eq!(burn_fuel(&mut oxygen, 100), (0, 0));
    }

    #[test]
    fn water_splits_into_hydrogen_and_oxygen() {
        let (mut water, mut h, mut o) = (
            3,
            ChemicalTank::new(SEPARATOR_GAS),
            ChemicalTank::new(SEPARATOR_GAS),
        );
        let mut e = EnergyContainer::new(100, u64::MAX);
        e.stored = 100;
        assert!(separate_water(&mut water, &mut h, &mut o, &mut e));
        assert!(!separate_water(&mut water, &mut h, &mut o, &mut e));
        assert_eq!((water, h.amount, o.amount, e.stored), (1, 2, 1, 96));
    }
}
