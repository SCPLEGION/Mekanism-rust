//! The chemical capability (`mekanism:chemical`), Mekanism's own, as the upstream docs describe
//! (concepts/capabilities/own-capability.md): the blocks that hold chemicals answer it, and the ones that move them
//! (tubes, a multiblock's ports, tanks) find their neighbours through it, in place of reading the neighbour's
//! compound as `chemical_receiver` of `runtime.rs` did.
//!
//! A holder keeps its tanks in its compound as before (`chem::get_tank`), so saves carry over; the handler reads and
//! writes them through the block entity, which the transaction journal covers.
use super::chem::{get_tank, put_tank};
use mekanism_core::chemical::ChemicalTank;
use mod_api::{HostTransaction,
    BlockEntityRef, BlockEntityTypeKey, BlockSide, CapabilityKey, Direction, Host, LevelRef, Registrar,
    ResourceHandler, ServerHost, Transaction,
};

/// A chemical by id (`mekanism:hydrogen`). Fluids that share the chemical code (water, lava, brine) are the
/// fluid capability's, not this one's.
#[derive(Clone, Debug, PartialEq)]
pub struct Chemical(pub String);

pub const CHEMICALS: CapabilityKey<dyn ResourceHandler<Chemical>> = CapabilityKey::new("mekanism:chemical");

/// One tank a block exposes on a side: where it lives in the compound, its capacity and which way it may be used.
#[derive(Clone, Debug)]
pub(crate) struct TankSpec {
    pub key: &'static str,
    pub capacity: u64,
    /// Chemicals may be put in through this side.
    pub input: bool,
    /// Chemicals may be taken out through this side.
    pub output: bool,
}

/// The tanks of the block entity `entity` as seen from `side` (none: from no particular side).
pub(crate) type Layout = fn(&dyn Host, BlockEntityRef, Option<Direction>) -> Vec<TankSpec>;

/// Answers [`CHEMICALS`] for every block entity of `kind` with `layout`'s tanks.
pub(crate) fn provide(r: &mut dyn Registrar, kind: &BlockEntityTypeKey, layout: Layout) {
    r.block_entity_capability(&CHEMICALS, kind, move |host: &mut dyn ServerHost, at: BlockSide| {
        let entity = host.block_entity(at.level, at.pos)?;
        Some(Box::new(Tanks { entity, side: at.side, layout }) as Box<dyn ResourceHandler<Chemical>>)
    });
}

struct Tanks {
    entity: BlockEntityRef,
    side: Option<Direction>,
    layout: Layout,
}

impl Tanks {
    fn specs(&self, host: &mut dyn ServerHost) -> Vec<TankSpec> {
        (self.layout)(&*host, self.entity, self.side)
    }

    /// `change` on tank `index`, saved when it returns true; `None` when there is no such tank.
    fn edit<R>(
        &self,
        host: &mut dyn ServerHost,
        index: usize,
        change: impl FnOnce(&TankSpec, &mut ChemicalTank) -> (R, bool),
    ) -> Option<R> {
        let spec = self.specs(host).into_iter().nth(index)?;
        let mut data = self.entity.data(&*host)?;
        let mut tank = get_tank(&data, spec.key, spec.capacity);
        let (result, save) = change(&spec, &mut tank);
        if save {
            put_tank(&mut data, spec.key, &tank);
            let _ = self.entity.replace_data(host, data);
        }
        Some(result)
    }
}

fn clamp(v: u64) -> u32 {
    v.min(u32::MAX as u64) as u32
}

impl ResourceHandler<Chemical> for Tanks {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        self.specs(host).len()
    }

    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<Chemical> {
        self.edit(host, index, |_, t| (t.chemical.clone().filter(|_| t.amount > 0), false)).flatten().map(Chemical)
    }

    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        self.edit(host, index, |_, t| (clamp(t.amount), false)).unwrap_or(0)
    }

    fn capacity(&self, host: &mut dyn ServerHost, index: usize, resource: &Chemical) -> u32 {
        self.edit(host, index, |spec, t| (if t.accepts(&resource.0) { clamp(spec.capacity) } else { 0 }, false))
            .unwrap_or(0)
    }

    fn insert(&self, host: &mut dyn ServerHost, _: &Transaction, index: usize, resource: &Chemical, amount: u32) -> u32 {
        self.edit(host, index, |spec, t| {
            if !spec.input {
                return (0, false);
            }
            let put = t.insert(&resource.0, u64::from(amount));
            (clamp(put), put > 0)
        })
        .unwrap_or(0)
    }

    fn extract(&self, host: &mut dyn ServerHost, _: &Transaction, index: usize, resource: &Chemical, amount: u32) -> u32 {
        self.edit(host, index, |spec, t| {
            if !spec.output || t.chemical.as_deref() != Some(resource.0.as_str()) {
                return (0, false);
            }
            let taken = t.extract(u64::from(amount));
            (clamp(taken), taken > 0)
        })
        .unwrap_or(0)
    }
}

const SIDES: [(Direction, [i32; 3]); 6] = [
    (Direction::Down, [0, -1, 0]),
    (Direction::Up, [0, 1, 0]),
    (Direction::North, [0, 0, -1]),
    (Direction::South, [0, 0, 1]),
    (Direction::West, [-1, 0, 0]),
    (Direction::East, [1, 0, 0]),
];

/// Fills `into` with `chemical` drawn from the neighbours of `pos` (`pull_from_carriers`): how much came.
pub(crate) fn pull(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], into: &mut ChemicalTank, chemical: &str) -> u64 {
    let mut got = 0;
    for (side, d) in SIDES {
        let room = into.capacity.saturating_sub(into.amount);
        if room == 0 || !into.accepts(chemical) {
            break;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let Some(from) = level.capability(host, &CHEMICALS, at, Some(side.opposite())) else {
            continue;
        };
        let resource = Chemical(chemical.to_owned());
        let moved = host.transaction::<u64, ()>(|host, tx| {
            let mut moved = 0u64;
            for index in 0..from.size(host) {
                let want = clamp(room - moved);
                if want == 0 {
                    break;
                }
                moved += u64::from(from.extract(host, tx, index, &resource, want));
            }
            Ok(moved)
        });
        let moved = moved.unwrap_or(0);
        got += into.insert(chemical, moved);
    }
    got
}

/// Gives `from` to the neighbours of `pos` that take it (`push_chemical`), emptying it as far as they accept.
pub(crate) fn push(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], from: &mut ChemicalTank) {
    let Some(chemical) = from.chemical.clone() else {
        return;
    };
    for (side, d) in SIDES {
        if from.amount == 0 {
            break;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let Some(to) = level.capability(host, &CHEMICALS, at, Some(side.opposite())) else {
            continue;
        };
        let resource = Chemical(chemical.clone());
        let offered = from.amount;
        let taken = host.transaction::<u64, ()>(|host, tx| {
            let mut taken = 0u64;
            for index in 0..to.size(host) {
                let rest = clamp(offered - taken);
                if rest == 0 {
                    break;
                }
                taken += u64::from(to.insert(host, tx, index, &resource, rest));
            }
            Ok(taken)
        });
        from.extract(taken.unwrap_or(0));
    }
}
