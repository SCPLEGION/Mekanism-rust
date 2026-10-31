// Electric machines with one item input and one item output
// (mekanism.common.tile.prefab.TileEntityElectricMachine, TileEntityProgressMachine).

use crate::energy::EnergyContainer;
use crate::upgrades::Upgrades;

/// `TileEntityElectricMachine.BASE_TICKS_REQUIRED`: 10 seconds.
pub const BASE_TICKS_REQUIRED: u32 = 200;
/// `AttributeEnergy.STORAGE_MULTIPLIER`: a machine stores this many ticks of its usage.
pub const STORAGE_MULTIPLIER: u64 = 4;

/// The machines this port runs: (block id, recipe kind, energy per tick from `UsageConfig`).
pub const MACHINES: [(&str, &str, u64); 5] = [
    ("mekanism:enrichment_chamber", "enriching", 5),
    ("mekanism:crusher", "crushing", 8),
    ("mekanism:energized_smelter", "smelting", 5),
    ("mekanism:precision_sawmill", "sawing", 5),
    ("mekanism:combiner", "combining", 5),
];

pub fn machine(block: &str) -> Option<(&'static str, u64)> {
    MACHINES
        .iter()
        .find(|(id, _, _)| *id == block)
        .map(|(_, kind, usage)| (*kind, *usage))
}

/// A recipe as the machine sees it: what it takes and what it gives.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Operation {
    pub input_count: u32,
    /// The Combiner's extra input, taken from the middle slot.
    pub extra_count: u32,
    pub output: String,
    pub output_count: u32,
}

/// The machine's slots: input, energy, output (the slot order of the furnace menu it shows in).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Slot {
    pub item: Option<String>,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElectricMachine {
    pub energy: EnergyContainer,
    pub usage: u64,
    pub progress: u32,
    pub ticks_required: u32,
    /// Whether the last tick ran (the block's `active` state, shown as the furnace's `lit`).
    pub active: bool,
}

impl ElectricMachine {
    /// A machine without upgrades.
    pub fn new(usage: u64) -> Self {
        Self::with_upgrades(usage, Upgrades::default())
    }

    /// `MachineEnergyContainer.updateEnergyPerTick` and `updateMaxEnergy` for a progress machine:
    /// upgrades set the ticks per operation and the energy per tick, and the machine stores
    /// `max(STORAGE_MULTIPLIER, ticks)` ticks of that.
    pub fn with_upgrades(base_usage: u64, upgrades: Upgrades) -> Self {
        let usage = upgrades.energy_per_tick(base_usage);
        let ticks_required = upgrades.ticks(BASE_TICKS_REQUIRED);
        let buffer = STORAGE_MULTIPLIER.max(u64::from(ticks_required));
        let energy = EnergyContainer::new(usage.saturating_mul(buffer).max(usage), u64::MAX);
        Self {
            energy,
            usage,
            progress: 0,
            ticks_required,
            active: false,
        }
    }

    /// One server tick. `operation` is the recipe for what is in the input slot (none: nothing to do);
    /// `max_stack` is the output item's stack size.
    pub fn tick(
        &mut self,
        input: &mut Slot,
        output: &mut Slot,
        extra: Option<&mut Slot>,
        operation: Option<&Operation>,
        max_stack: u32,
    ) {
        self.energy.start_tick();
        let Some(op) = operation else {
            self.progress = 0;
            self.active = false;
            return;
        };
        let fits = match &output.item {
            None => op.output_count <= max_stack,
            Some(item) => *item == op.output && output.count + op.output_count <= max_stack,
        };
        if !fits
            || input.count < op.input_count
            || extra.as_ref().map_or(0, |e| e.count) < op.extra_count
        {
            self.active = false;
            return;
        }
        // Not enough energy pauses the operation; it keeps its progress.
        if !self.energy.consume(self.usage) {
            self.active = false;
            return;
        }
        self.active = true;
        self.progress += 1;
        if self.progress >= self.ticks_required {
            self.progress = 0;
            take(input, op.input_count);
            if let Some(extra) = extra {
                take(extra, op.extra_count);
            }
            output.item = Some(op.output.clone());
            output.count += op.output_count;
        }
    }
}

fn take(slot: &mut Slot, n: u32) {
    slot.count -= n.min(slot.count);
    if slot.count == 0 {
        slot.item = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ore() -> Slot {
        Slot {
            item: Some("mekanism:osmium_ore".into()),
            count: 2,
        }
    }

    fn doubling() -> Operation {
        Operation {
            input_count: 1,
            output: "mekanism:dust_osmium".into(),
            output_count: 2,
            ..Operation::default()
        }
    }

    #[test]
    fn a_progress_machine_stores_one_operation_of_energy() {
        // 5 FE a tick for 200 ticks: 1 kFE, as Mekanism's own TODO about the Energized Smelter says.
        let m = ElectricMachine::new(5);
        assert_eq!((m.energy.capacity, m.ticks_required), (1000, 200));
        // Eight speed upgrades: 20 ticks at 500 FE, buffered for 20 ticks.
        let fast = ElectricMachine::with_upgrades(
            5,
            Upgrades {
                speed: 8,
                energy: 0,
            },
        );
        assert_eq!(
            (fast.usage, fast.ticks_required, fast.energy.capacity),
            (500, 20, 10_000)
        );
    }

    #[test]
    fn a_combiner_takes_its_extra_input_too() {
        let mut m = ElectricMachine::with_upgrades(
            5,
            Upgrades {
                speed: 8,
                energy: 0,
            },
        );
        let mut dust = Slot {
            item: Some("mekanism:dust_diamond".into()),
            count: 5,
        };
        let mut stone = Slot {
            item: Some("minecraft:cobblestone".into()),
            count: 1,
        };
        let mut out = Slot::default();
        let op = Operation {
            input_count: 5,
            extra_count: 1,
            output: "minecraft:diamond_ore".into(),
            output_count: 1,
        };
        for _ in 0..20 {
            m.energy.stored = m.energy.capacity;
            m.tick(&mut dust, &mut out, Some(&mut stone), Some(&op), 64);
        }
        assert_eq!((dust.item, stone.item, out.count), (None, None, 1));
        // Without the extra input, nothing runs.
        let mut dust = Slot {
            item: Some("mekanism:dust_diamond".into()),
            count: 5,
        };
        let mut none = Slot::default();
        m.tick(&mut dust, &mut out, Some(&mut none), Some(&op), 64);
        assert!(!m.active);
    }

    #[test]
    fn two_hundred_powered_ticks_double_one_ore() {
        let mut m = ElectricMachine::new(5);
        let (mut input, mut output) = (ore(), Slot::default());
        for _ in 0..200 {
            m.energy.stored = m.energy.capacity;
            m.tick(&mut input, &mut output, None, Some(&doubling()), 64);
        }
        assert_eq!(input.count, 1);
        assert_eq!(
            (output.item.as_deref(), output.count),
            (Some("mekanism:dust_osmium"), 2)
        );
        assert_eq!(m.progress, 0);
    }

    #[test]
    fn without_energy_it_pauses_and_keeps_progress() {
        let mut m = ElectricMachine::new(5);
        let (mut input, mut output) = (ore(), Slot::default());
        m.energy.stored = 10;
        for _ in 0..5 {
            m.tick(&mut input, &mut output, None, Some(&doubling()), 64);
        }
        assert_eq!((m.progress, m.active, m.energy.stored), (2, false, 0));
    }

    #[test]
    fn a_full_or_different_output_blocks_the_operation() {
        let mut m = ElectricMachine::new(5);
        m.energy.stored = 20;
        let mut input = ore();
        let mut other = Slot {
            item: Some("minecraft:dirt".into()),
            count: 1,
        };
        m.tick(&mut input, &mut other, None, Some(&doubling()), 64);
        let mut full = Slot {
            item: Some("mekanism:dust_osmium".into()),
            count: 63,
        };
        m.tick(&mut input, &mut full, None, Some(&doubling()), 64);
        assert_eq!((m.progress, m.energy.stored), (0, 20));
    }
}
