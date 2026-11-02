// The Quantum Entangloporter (`TileEntityQuantumEntangloporter`, `InventoryFrequency`): every
// entangloporter on one frequency shares one energy buffer, one fluid tank and one chemical tank,
// in any dimension.

use crate::chemical::ChemicalTank;

pub const ENTANGLOPORTER: &str = "mekanism:quantum_entangloporter";
/// `general.qe.energyBuffer`: an ultimate energy cube's capacity.
pub const ENERGY_BUFFER: u64 = 256_000_000;
/// `general.qe.fluidBuffer`: an ultimate fluid tank's.
pub const FLUID_BUFFER: u64 = 256_000;
/// `general.qe.chemicalBuffer`: an ultimate chemical tank's.
pub const CHEMICAL_BUFFER: u64 = 8_192_000;

/// One frequency's contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frequency {
    pub energy: u64,
    pub fluid: ChemicalTank,
    pub chemical: ChemicalTank,
}

impl Default for Frequency {
    fn default() -> Self {
        Self {
            energy: 0,
            fluid: ChemicalTank::new(FLUID_BUFFER),
            chemical: ChemicalTank::new(CHEMICAL_BUFFER),
        }
    }
}

impl Frequency {
    /// Moves what one entangloporter took in since its last tick into the frequency; what doesn't fit
    /// stays where it was.
    pub fn absorb(
        &mut self,
        energy: &mut u64,
        fluid: &mut ChemicalTank,
        chemical: &mut ChemicalTank,
    ) {
        let e = (*energy).min(ENERGY_BUFFER - self.energy);
        self.energy += e;
        *energy -= e;
        for (from, to) in [(fluid, &mut self.fluid), (chemical, &mut self.chemical)] {
            if let Some(id) = from.chemical.clone() {
                let moved = to.insert(&id, from.amount);
                from.extract(moved);
            }
        }
    }

    /// The room left for energy.
    pub fn energy_room(&self) -> u64 {
        ENERGY_BUFFER - self.energy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absorbs_up_to_the_buffers() {
        let mut f = Frequency {
            energy: ENERGY_BUFFER - 10,
            ..Default::default()
        };
        let mut energy = 25;
        let mut fluid = ChemicalTank::new(FLUID_BUFFER);
        fluid.insert("minecraft:water", 500);
        let mut chem = ChemicalTank::new(CHEMICAL_BUFFER);
        f.absorb(&mut energy, &mut fluid, &mut chem);
        assert_eq!((f.energy, energy), (ENERGY_BUFFER, 15));
        assert_eq!((f.fluid.amount, fluid.amount), (500, 0));
        assert_eq!(f.energy_room(), 0);
    }

    #[test]
    fn keeps_a_different_fluid_out() {
        let mut f = Frequency::default();
        f.fluid.insert("minecraft:water", 1_000);
        let mut lava = ChemicalTank::new(FLUID_BUFFER);
        lava.insert("minecraft:lava", 1_000);
        f.absorb(&mut 0, &mut lava, &mut ChemicalTank::new(CHEMICAL_BUFFER));
        assert_eq!(lava.amount, 1_000);
        assert_eq!(f.fluid.chemical.as_deref(), Some("minecraft:water"));
    }
}
