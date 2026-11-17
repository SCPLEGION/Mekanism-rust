//! Energy between neighbouring blocks: Mekanism's own capability [`ENERGY`] (amounts are 64-bit, which the game's
//! 32-bit `horizon:energy` cannot carry), the handler a block entity family provides for it, and [`push`], which a
//! generator, a cube or a cable calls to hand energy to the blocks around it. Port of `receiver`, `push_energy` and
//! `energy_accept` of `runtime.rs`.
//!
//! A block entity keeps its energy in its saved compound under `energy`, so a neighbour that is not ticking is
//! read and written through `block_entity_data` / `block_entity_merge_data`. The block entity that is ticking is
//! out of the world meanwhile and keeps its energy in its own `EnergyContainer`.
//!
//! How a family joins in:
//! ```ignore
//! energy::provide(r, "mekanism:basic_energy_cube", |nbt, side| Some(Port { capacity, insert: rate, extract: rate }));
//! ```
//! and in its tick: `energy::push(host, event.pos, &mut container, Faces::ALL, rate)`.
use super::nbt::{get_long, put_long};
use std::sync::Arc;
use mekanism_core::energy::{Action, EnergyContainer};
use mod_api::HostBlockStates as _;
use mod_api::{
    BlockEntityRef, BlockSide, CapabilityKey, Compound, Direction, LevelRef, Registrar, ServerHost,
};

/// What a block does with energy through one face: the buffer, the most one call takes in and the most it gives
/// out (0: it gives none, as a machine).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Port {
    pub capacity: u64,
    pub insert: u64,
    pub extract: u64,
}

/// A block's energy as one face reaches it. Writes take effect at once; with `execute` false they only report.
pub(crate) trait MekEnergy: Send + Sync {
    fn amount(&self, host: &mut dyn ServerHost) -> u64;
    fn capacity(&self, host: &mut dyn ServerHost) -> u64;
    /// Puts up to `amount` in: how much went.
    fn insert(&self, host: &mut dyn ServerHost, amount: u64, execute: bool) -> u64;
    /// Takes up to `amount` out: how much came.
    fn extract(&self, host: &mut dyn ServerHost, amount: u64, execute: bool) -> u64;
}

/// `mekanism:energy`, the capability of blocks that hold energy.
pub(crate) const ENERGY: CapabilityKey<dyn MekEnergy> = CapabilityKey::new("mekanism:energy");

/// Port of a block: from the block entity's compound, the face asked through (None: no face in particular) and the
/// way the block faces (None: it does not).
pub(crate) type PortFn = Arc<dyn Fn(&Compound, Option<Direction>, Option<Direction>) -> Option<Port> + Send + Sync>;

struct NbtEnergy {
    entity: BlockEntityRef,
    side: Option<Direction>,
    facing: Option<Direction>,
    port: PortFn,
}

impl NbtEnergy {
    fn read(&self, host: &mut dyn ServerHost) -> Option<(Compound, Port)> {
        let nbt = self.entity.data(host)?;
        let port = (self.port)(&nbt, self.side, self.facing)?;
        Some((nbt, port))
    }
}

impl MekEnergy for NbtEnergy {
    fn amount(&self, host: &mut dyn ServerHost) -> u64 {
        self.read(host).map_or(0, |(nbt, port)| get_long(&nbt, "energy").min(port.capacity))
    }
    fn capacity(&self, host: &mut dyn ServerHost) -> u64 {
        self.read(host).map_or(0, |(_, port)| port.capacity)
    }
    fn insert(&self, host: &mut dyn ServerHost, amount: u64, execute: bool) -> u64 {
        let Some((nbt, port)) = self.read(host) else {
            return 0;
        };
        let mut energy = EnergyContainer::new(port.capacity, port.insert);
        energy.stored = get_long(&nbt, "energy").min(port.capacity);
        let taken = amount - energy.insert(amount, if execute { Action::Execute } else { Action::Simulate });
        if execute && taken > 0 {
            let mut change = Compound::new();
            put_long(&mut change, "energy", energy.stored);
            let _ = self.entity.merge_data(host, change);
        }
        taken
    }
    fn extract(&self, host: &mut dyn ServerHost, amount: u64, execute: bool) -> u64 {
        let Some((nbt, port)) = self.read(host) else {
            return 0;
        };
        let stored = get_long(&nbt, "energy").min(port.capacity);
        let given = amount.min(port.extract).min(stored);
        if execute && given > 0 {
            let mut change = Compound::new();
            put_long(&mut change, "energy", stored - given);
            let _ = self.entity.merge_data(host, change);
        }
        given
    }
}

/// Makes the block `block` answer [`ENERGY`] with `port`.
pub(crate) fn provide(
    r: &mut dyn Registrar,
    block: &str,
    port: impl Fn(&Compound, Option<Direction>, Option<Direction>) -> Option<Port> + Send + Sync + 'static,
) {
    let key = r.block_key(block);
    let port: PortFn = Arc::new(port);
    r.block_capability(&ENERGY, &key, move |host, at, state, entity| {
        let entity = entity?;
        let facing = state.property(&host.block_states(), "facing").ok().flatten().and_then(direction);
        Some(Box::new(NbtEnergy { entity, side: at.side, facing, port: port.clone() }) as Box<dyn MekEnergy>)
    });
}

/// A `facing` property's value as a direction.
pub(crate) fn direction(name: &str) -> Option<Direction> {
    Direction::ALL.into_iter().find(|d| format!("{d:?}").eq_ignore_ascii_case(name))
}

/// The faces of a block energy may leave through.
#[derive(Clone, Copy)]
pub(crate) struct Faces(u8);

impl Faces {
    pub(crate) const ALL: Faces = Faces(0b11_1111);
    pub(crate) const NONE: Faces = Faces(0);

    pub(crate) fn only(face: Direction) -> Faces {
        Faces(1 << index(face))
    }
    pub(crate) fn has(self, face: Direction) -> bool {
        self.0 & (1 << index(face)) != 0
    }
}

fn index(d: Direction) -> u8 {
    match d {
        Direction::Down => 0,
        Direction::Up => 1,
        Direction::North => 2,
        Direction::South => 3,
        Direction::West => 4,
        Direction::East => 5,
    }
}

/// `EmitUtils.sendToAcceptors`: gives `from` (at most `rate` in all) to the neighbours of `pos` through `faces`,
/// splitting what they take evenly among those that take any. Returns what left.
pub(crate) fn push(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    from: &mut EnergyContainer,
    faces: Faces,
    rate: u64,
) -> u64 {
    let mut budget = from.stored.min(rate);
    if budget == 0 {
        return 0;
    }
    // What each acceptor takes, simulated, then the even split.
    let mut targets = Vec::new();
    for face in Direction::ALL {
        if !faces.has(face) {
            continue;
        }
        let n = face.normal();
        let at = [pos[0] + n[0], pos[1] + n[1], pos[2] + n[2]];
        let Some(cap) = level.capability(host, &ENERGY, at, Some(face.opposite())) else {
            continue;
        };
        let room = cap.insert(host, budget, false);
        if room > 0 {
            targets.push((at, face, room));
        }
    }
    let mut left = targets.len() as u64;
    targets.sort_by_key(|t| t.2);
    let mut sent = 0;
    for (at, face, room) in targets {
        let share = (budget / left.max(1)).min(room);
        left -= 1;
        if share == 0 {
            continue;
        }
        let Some(cap) = level.capability(host, &ENERGY, at, Some(face.opposite())) else {
            continue;
        };
        let took = cap.insert(host, share, true);
        budget -= took;
        sent += took;
    }
    from.extract(sent, Action::Execute);
    sent
}

/// The energy a neighbour at `face` of `pos` can take of `amount` (nothing moves).
#[allow(dead_code)]
pub(crate) fn accepts(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], face: Direction, amount: u64) -> u64 {
    let n = face.normal();
    let at = [pos[0] + n[0], pos[1] + n[1], pos[2] + n[2]];
    level.capability(host, &ENERGY, at, Some(face.opposite())).map_or(0, |c| c.insert(host, amount, false))
}

#[allow(dead_code)]
fn _unused(_: BlockSide) {}
