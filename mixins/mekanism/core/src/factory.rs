// Factories (mekanism.common.tile.factory.TileEntityFactory): a tiered machine running several
// operations at once from one energy store, each process on its own input and output slot.

use crate::machine::{ElectricMachine, Operation, STORAGE_MULTIPLIER, Slot};
use crate::upgrades::Upgrades;

/// `FactoryTier.processes`.
pub const TIERS: [(&str, usize); 4] =
    [("basic", 3), ("advanced", 5), ("elite", 7), ("ultimate", 9)];

/// The factory kinds this port runs: (`FactoryType` name, the machine's recipe kind, energy per tick).
pub const KINDS: [(&str, &str, u64); 3] = [
    ("enriching", "enriching", 5),
    ("crushing", "crushing", 8),
    ("smelting", "smelting", 5),
];

/// A factory block id (`mekanism:elite_crushing_factory`): its processes, recipe kind and usage.
pub fn factory(block: &str) -> Option<(usize, &'static str, u64)> {
    let name = block.strip_prefix("mekanism:")?.strip_suffix("_factory")?;
    let (tier, kind) = name.split_once('_')?;
    let processes = TIERS.iter().find(|(t, _)| *t == tier)?.1;
    let (_, recipe, usage) = KINDS.iter().find(|(k, _, _)| *k == kind)?;
    Some((processes, recipe, *usage))
}

/// The x of process `i`'s slots and arrow column in a factory of `processes` (`baseX + i * baseXMult`
/// in `TileEntityFactory.addSlots` and `GuiFactory`).
pub fn slot_x(processes: usize, i: usize) -> i32 {
    let (base, step) = match processes {
        3 => (55, 38),
        5 => (35, 26),
        7 => (29, 19),
        _ => (27, 19),
    };
    base + step * i as i32
}

#[derive(Clone, Debug, PartialEq)]
pub struct Factory {
    /// The shared energy and the ticks per operation; its own `progress` is unused.
    pub machine: ElectricMachine,
    pub progress: Vec<u32>,
    pub active: bool,
}

impl Factory {
    /// `updateMaxEnergy`: one operation of energy for every process.
    pub fn new(processes: usize, usage: u64, upgrades: Upgrades) -> Self {
        let mut machine = ElectricMachine::with_upgrades(usage, upgrades);
        let buffer = STORAGE_MULTIPLIER.max(u64::from(machine.ticks_required)) * processes as u64;
        machine.energy.capacity = machine.usage.saturating_mul(buffer).max(machine.usage);
        Self {
            machine,
            progress: vec![0; processes],
            active: false,
        }
    }

    /// One tick: each process in order, as `recipeCacheLookupMonitors` run, drawing on the shared
    /// energy. `operations[i]` is the recipe for input `i` and its output's stack size.
    pub fn tick(
        &mut self,
        inputs: &mut [Slot],
        outputs: &mut [Slot],
        operations: &[Option<(Operation, u32)>],
    ) {
        self.active = false;
        for i in 0..self.progress.len() {
            self.machine.progress = self.progress[i];
            let op = operations.get(i).and_then(|o| o.as_ref());
            self.machine.tick(
                &mut inputs[i],
                &mut outputs[i],
                None,
                op.map(|(o, _)| o),
                op.map_or(64, |(_, max)| *max),
            );
            self.progress[i] = self.machine.progress;
            self.active |= self.machine.active;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_spread_across_the_screen() {
        assert_eq!(
            (0..3).map(|i| slot_x(3, i)).collect::<Vec<_>>(),
            [55, 93, 131]
        );
        assert_eq!(slot_x(9, 8), 27 + 8 * 19);
        // The ultimate factory's last slot fits its 210 wide screen.
        assert!(slot_x(9, 8) + 18 < 210 - 12);
    }

    #[test]
    fn factory_ids_name_their_tier_and_kind() {
        assert_eq!(
            factory("mekanism:basic_enriching_factory"),
            Some((3, "enriching", 5))
        );
        assert_eq!(
            factory("mekanism:ultimate_crushing_factory"),
            Some((9, "crushing", 8))
        );
        assert_eq!(factory("mekanism:basic_infusing_factory"), None);
        assert_eq!(factory("mekanism:crusher"), None);
    }

    #[test]
    fn three_processes_run_side_by_side_on_one_store() {
        let mut f = Factory::new(3, 5, Upgrades::default());
        assert_eq!(f.machine.energy.capacity, 3000);
        let ore = || Slot {
            item: Some("mekanism:osmium_ore".into()),
            count: 1,
        };
        let mut inputs = vec![ore(), ore(), Slot::default()];
        let mut outputs = vec![Slot::default(); 3];
        let op = Operation {
            input_count: 1,
            output: "mekanism:dust_osmium".into(),
            output_count: 2,
            ..Operation::default()
        };
        let ops = vec![Some((op.clone(), 64)), Some((op, 64)), None];
        for _ in 0..200 {
            f.machine.energy.stored = f.machine.energy.capacity;
            f.tick(&mut inputs, &mut outputs, &ops);
        }
        assert_eq!(
            (outputs[0].count, outputs[1].count, outputs[2].count),
            (2, 2, 0)
        );
        // Two processes ran: 10 FE a tick.
        f.machine.energy.stored = 10;
        let mut more = vec![ore(), ore(), Slot::default()];
        f.tick(&mut more, &mut outputs, &ops);
        assert_eq!(f.machine.energy.stored, 0);
        assert!(f.active);
    }
}
