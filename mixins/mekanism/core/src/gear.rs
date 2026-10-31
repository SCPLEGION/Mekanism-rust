// Items that hold energy (mekanism.common.registries.MekanismItems' energy containers, GearConfig), the
// energy slots that charge and discharge them (EnergyInventorySlot), and the Atomic Disassembler's
// mining (ItemAtomicDisassembler).

use crate::energy::{Action, EnergyContainer};
use crate::tier::{self, BaseTier};

/// An item's energy container: how much it holds, how fast it charges, and whether machines may
/// take energy back out of it (`canExtract`: the tablet and energy cubes; tools only spend theirs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemEnergy {
    pub capacity: u64,
    pub rate: u64,
    pub can_discharge: bool,
}

/// `GearConfig` defaults: energy tablet and Atomic Disassembler 1 MFE at 5 kFE/t, electric bow
/// 120 kFE at 600 FE/t. An energy cube item holds what its block does, at its output rate.
pub fn item_energy(id: &str) -> Option<ItemEnergy> {
    let (capacity, rate, can_discharge) = match id {
        "mekanism:energy_tablet" => (1_000_000, 5_000, true),
        "mekanism:atomic_disassembler" => (1_000_000, 5_000, false),
        "mekanism:electric_bow" => (120_000, 600, false),
        _ => {
            let name = id.strip_prefix("mekanism:")?.strip_suffix("_energy_cube")?;
            let tier = BaseTier::ALL.into_iter().find(|t| t.name() == name)?;
            let (capacity, rate) = tier::energy_cube(tier);
            (capacity, rate, true)
        }
    };
    Some(ItemEnergy {
        capacity,
        rate,
        can_discharge,
    })
}

/// A charge slot (an energy cube's left slot) fills the item from `source`, at most the item's rate
/// a tick. Returns the item's new energy.
pub fn charge(item: ItemEnergy, stored: u64, source: &mut EnergyContainer) -> u64 {
    let room = item.capacity.saturating_sub(stored).min(item.rate);
    let moved = source.extract(room, Action::Execute);
    stored + moved
}

/// A discharge slot (a machine's energy slot, a cube's right slot) moves the item's energy into
/// `target`, at most the item's rate a tick. Returns the item's new energy.
pub fn discharge(item: ItemEnergy, stored: u64, target: &mut EnergyContainer) -> u64 {
    if !item.can_discharge {
        return stored;
    }
    let offered = stored.min(item.rate);
    let left = target.insert(offered, Action::Execute);
    stored - (offered - left)
}

/// `DisassemblerMode.NORMAL`'s efficiency, the only mode this port has (Mekanism switches modes with
/// the mode key).
pub const DISASSEMBLER_EFFICIENCY: f32 = 20.0;
/// `GearConfig.disassemblerEnergyUsage`, FE per point of efficiency a block costs.
pub const DISASSEMBLER_ENERGY_USAGE: u64 = 10;

/// The energy breaking a block costs (`getDestroyEnergy`): half for a block that breaks instantly.
pub fn destroy_energy(hardness: f32) -> u64 {
    let base = DISASSEMBLER_ENERGY_USAGE * DISASSEMBLER_EFFICIENCY as u64;
    if hardness == 0.0 {
        (base / 2).max(1)
    } else {
        base
    }
}

/// `ItemAtomicDisassembler.getDestroySpeed`: the mode's efficiency while the energy for the block is
/// there, scaled down by how much of it is.
pub fn destroy_speed(stored: u64, hardness: f32) -> f32 {
    let required = destroy_energy(hardness);
    if stored < required {
        return DISASSEMBLER_EFFICIENCY * (stored as f32 / required as f32);
    }
    DISASSEMBLER_EFFICIENCY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_hold_what_gear_config_says() {
        let t = item_energy("mekanism:energy_tablet").unwrap();
        assert_eq!(
            (t.capacity, t.rate, t.can_discharge),
            (1_000_000, 5_000, true)
        );
        assert!(
            !item_energy("mekanism:atomic_disassembler")
                .unwrap()
                .can_discharge
        );
        let cube = item_energy("mekanism:elite_energy_cube").unwrap();
        assert_eq!((cube.capacity, cube.rate), (64_000_000, 64_000));
        assert_eq!(item_energy("minecraft:stick"), None);
    }

    #[test]
    fn a_cube_charges_a_tablet_at_the_tablets_rate() {
        let tablet = item_energy("mekanism:energy_tablet").unwrap();
        let mut cube = EnergyContainer::new(4_000_000, 4_000);
        cube.stored = 100_000;
        let after = charge(tablet, 998_000, &mut cube);
        assert_eq!(after, 1_000_000, "only the room left");
        assert_eq!(cube.stored, 98_000);
        let mut cube = EnergyContainer::new(4_000_000, 4_000);
        cube.stored = 100_000;
        let after = charge(tablet, 0, &mut cube);
        assert_eq!(after, 4_000, "the cube's own output rate limits it");
    }

    #[test]
    fn a_machine_runs_off_a_tablet_but_not_off_a_disassembler() {
        let mut machine = EnergyContainer::new(20_000, u64::MAX);
        let tablet = item_energy("mekanism:energy_tablet").unwrap();
        assert_eq!(discharge(tablet, 12_000, &mut machine), 7_000);
        assert_eq!(machine.stored, 5_000);
        let drill = item_energy("mekanism:atomic_disassembler").unwrap();
        assert_eq!(discharge(drill, 12_000, &mut machine), 12_000);
    }

    #[test]
    fn the_disassembler_slows_down_as_it_runs_out() {
        assert_eq!(destroy_energy(1.5), 200);
        assert_eq!(destroy_energy(0.0), 100);
        assert_eq!(destroy_speed(1_000, 50.0), 20.0);
        assert_eq!(destroy_speed(50, 3.0), 5.0);
        assert_eq!(destroy_speed(0, 3.0), 0.0);
    }
}
