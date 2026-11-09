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

fn buffer(key: &str, default: u64) -> u64 {
    crate::config_files::active_int("general", "quantum_entangloporter", key, default)
}

/// The buffers of the open world's `GeneralConfig`.
pub fn energy_buffer() -> u64 {
    buffer("energyBuffer", ENERGY_BUFFER)
}

pub fn fluid_buffer() -> u64 {
    buffer("fluidBuffer", FLUID_BUFFER)
}

pub fn chemical_buffer() -> u64 {
    buffer("chemicalBuffer", CHEMICAL_BUFFER)
}

/// One frequency's contents.
/// `InventoryFrequency.storedHeat`: a capacitor of capacity 1 (`HeatAPI.DEFAULT_HEAT_CAPACITY`), so its heat is
/// its temperature in kelvin.
pub const HEAT_CAPACITY: f64 = 1.0;
pub const HEAT_INVERSE_CONDUCTION: f64 = 1.0;
pub const HEAT_INVERSE_INSULATION: f64 = 1_000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Frequency {
    pub energy: u64,
    pub heat: f64,
    pub fluid: ChemicalTank,
    pub chemical: ChemicalTank,
}

impl Default for Frequency {
    fn default() -> Self {
        Self {
            energy: 0,
            heat: crate::heat::AMBIENT_TEMP,
            fluid: ChemicalTank::new(fluid_buffer()),
            chemical: ChemicalTank::new(chemical_buffer()),
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
        let e = (*energy).min(energy_buffer() - self.energy);
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
        energy_buffer() - self.energy
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
        let mut fluid = ChemicalTank::new(fluid_buffer());
        fluid.insert("minecraft:water", 500);
        let mut chem = ChemicalTank::new(chemical_buffer());
        f.absorb(&mut energy, &mut fluid, &mut chem);
        assert_eq!((f.energy, energy), (ENERGY_BUFFER, 15));
        assert_eq!((f.fluid.amount, fluid.amount), (500, 0));
        assert_eq!(f.energy_room(), 0);
    }

    #[test]
    fn keeps_a_different_fluid_out() {
        let mut f = Frequency::default();
        f.fluid.insert("minecraft:water", 1_000);
        let mut lava = ChemicalTank::new(fluid_buffer());
        lava.insert("minecraft:lava", 1_000);
        f.absorb(&mut 0, &mut lava, &mut ChemicalTank::new(chemical_buffer()));
        assert_eq!(lava.amount, 1_000);
        assert_eq!(f.fluid.chemical.as_deref(), Some("minecraft:water"));
    }
}
