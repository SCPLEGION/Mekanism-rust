// The Industrial Turbine (mekanism.generators.common.content.turbine), simplified: a box of turbine
// casing, valves, vents and structural glass around a rotor shaft, a layer of pressure dispersers and
// electromagnetic coils. Steam piped into a valve turns into energy (and, with saturating condensers,
// water), by `TurbineMultiblockData.tick`'s formula.

pub const MIN_SIZE: i32 = 3;
pub const MAX_SIZE: i32 = 17;
/// `TurbineValidator.MAX_BLADES`; a rotor here carries its two blades.
pub const MAX_BLADES: u64 = 28;
pub const BLADES_PER_ROTOR: u64 = 2;
/// GeneratorsConfig: `bladesPerCoil`, `ventChemicalFlow`, `disperserChemicalFlow`, `condenserRate`,
/// `chemicalPerTank`, `energyCapacityPerVolume`, `joulesPerSteam`, `steamDivisor`.
pub const BLADES_PER_COIL: u64 = 4;
pub const VENT_FLOW: f64 = 32_000.0;
pub const DISPERSER_FLOW: f64 = 1_280.0;
pub const CONDENSER_RATE: u64 = 64_000;
pub const STEAM_PER_VOLUME: u64 = 64_000;
pub const ENERGY_PER_VOLUME: u64 = 100;
pub const JOULES_PER_STEAM: f64 = 5.0;
pub const STEAM_DIVISOR: f64 = 100.0;

pub fn is_frame(name: &str) -> bool {
    matches!(
        name,
        "mekanismgenerators:turbine_casing"
            | "mekanismgenerators:turbine_valve"
            | "mekanismgenerators:turbine_vent"
            | "mekanism:structural_glass"
    )
}

/// What a formed turbine holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Turbine {
    pub rotors: u64,
    pub dispersers: u64,
    pub coils: u64,
    pub condensers: u64,
    pub vents: u64,
    pub volume: u64,
    /// The volume below the disperser layer, where the steam is.
    pub lower_volume: u64,
}

impl Turbine {
    /// Counts an inner block; false for one that may not be inside.
    pub fn add_inside(&mut self, name: &str) -> bool {
        match name {
            "mekanismgenerators:turbine_rotor" => self.rotors += 1,
            "mekanism:pressure_disperser" => self.dispersers += 1,
            "mekanismgenerators:electromagnetic_coil" => self.coils += 1,
            "mekanismgenerators:saturating_condenser" => self.condensers += 1,
            "mekanismgenerators:rotational_complex" | "minecraft:air" | "minecraft:cave_air" => {}
            _ => return false,
        }
        true
    }

    pub fn blades(&self) -> u64 {
        (self.rotors * BLADES_PER_ROTOR).min(MAX_BLADES)
    }

    pub fn steam_capacity(&self) -> u64 {
        self.lower_volume * STEAM_PER_VOLUME
    }

    pub fn energy_capacity(&self) -> u64 {
        self.volume * ENERGY_PER_VOLUME
    }

    /// `getEnergyMultiplier`: energy per `STEAM_DIVISOR` mB of steam.
    pub fn energy_multiplier(&self) -> f64 {
        JOULES_PER_STEAM / MAX_BLADES as f64
            * self.blades().min(self.coils * BLADES_PER_COIL) as f64
    }

    /// `getMaxFlowRateDouble`.
    pub fn max_flow(&self) -> f64 {
        (self.lower_volume as f64 * (self.dispersers as f64 * DISPERSER_FLOW))
            .min(self.vents as f64 * VENT_FLOW)
    }

    /// One tick: (steam used, energy made, water condensed) from `stored` steam with room for `needed`
    /// energy.
    pub fn tick(&self, stored: u64, needed: u64) -> (u64, u64, u64) {
        let multiplier = self.energy_multiplier();
        if stored == 0 || needed == 0 || multiplier < 1e-5 || self.steam_capacity() == 0 {
            return (0, 0, 0);
        }
        let proportion = stored as f64 / self.steam_capacity() as f64;
        let rate = (stored as f64)
            .min(self.max_flow())
            .min(needed as f64 / multiplier * STEAM_DIVISOR)
            * proportion;
        let energy = (multiplier * (rate / STEAM_DIVISOR)) as u64;
        if rate <= 1e-5 || energy == 0 {
            return (0, 0, 0);
        }
        let flow = rate as u64;
        (flow, energy, flow.min(self.condensers * CONDENSER_RATE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_turbine_turns_steam_into_energy() {
        let t = Turbine {
            rotors: 2,
            dispersers: 8,
            coils: 1,
            condensers: 1,
            vents: 4,
            volume: 125,
            lower_volume: 50,
        };
        assert_eq!(t.blades(), 4);
        assert!((t.energy_multiplier() - 5.0 / 28.0 * 4.0).abs() < 1e-12);
        assert_eq!(t.max_flow(), 128_000.0);
        let (steam, energy, water) = t.tick(t.steam_capacity(), u64::MAX);
        assert_eq!(steam, 128_000);
        assert_eq!(energy, (5.0 / 28.0 * 4.0 * 1_280.0) as u64);
        assert_eq!(water, 64_000);
        // Half full, half the flow; no coils, nothing.
        assert_eq!(t.tick(t.steam_capacity() / 2, u64::MAX).0, 64_000);
        assert_eq!(Turbine { coils: 0, ..t }.tick(1_000, u64::MAX), (0, 0, 0));
    }
}
