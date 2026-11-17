//! Pressurized tubes (`TileEntityPressurizedTube`, chemicals) and mechanical pipes (`TileEntityMechanicalPipe`,
//! fluids) with their networks (`ChemicalNetwork`, `FluidNetwork`). Port of `tick_tube`, `tick_pipe` and
//! `carrier_network_step` of `runtime.rs`: what the transmitters hold is one buffer of one chemical; pulling faces
//! fill it from the tanks beside them, `EmitUtils` splits it among the acceptors, and what is left is shared back to
//! the transmitters that hold nothing else.
use super::chem::{get_tank, put_tank};
use super::nbt::put_long;
use super::transmit::{
    self, chemical_receiver, chemical_source, conn_of, face_index, is_fluid, neg, offset, sides_of, takes_in, Discovered, Own,
    DIRECTIONS,
};
use mekanism_core::chemical::ChemicalTank;
use mekanism_core::distribution;
use mekanism_core::sides::Transmission;
use mekanism_core::tier::{self, BaseTier};
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    LevelRef, Registrar, ServerHost,
};
use std::collections::HashSet;

/// The block entity types of the tubes and of the pipes.
pub(crate) const TUBE_TYPE: &str = "tube";
pub(crate) const PIPE_TYPE: &str = "pipe";

/// What the two kinds of carrier differ by.
#[derive(Clone, Copy)]
struct Flavor {
    suffix: &'static str,
    /// The compound key of the tank the transmitter keeps.
    key: &'static str,
    fluid: bool,
    rate: fn(BaseTier) -> Option<u64>,
}

const TUBE: Flavor = Flavor { suffix: "_pressurized_tube", key: "chemical", fluid: false, rate: tier::pressurized_tube };
const PIPE: Flavor = Flavor { suffix: "_mechanical_pipe", key: "fluid", fluid: true, rate: tier::mechanical_pipe };

impl Flavor {
    fn blocks(self) -> Vec<String> {
        BaseTier::ALL
            .into_iter()
            .filter(|t| (self.rate)(*t).is_some())
            .map(|t| format!("mekanism:{}{}", t.name(), self.suffix))
            .collect()
    }

    /// What a transmitter of block `id` moves in a tick, None for another block.
    fn rate_of(self, id: &str) -> Option<u64> {
        let tier = id.strip_prefix("mekanism:")?.strip_suffix(self.suffix)?;
        BaseTier::ALL.into_iter().find(|t| t.name() == tier).and_then(self.rate)
    }
}

/// The transmitter's rate of a pipe or a tube block, for the other families.
pub(crate) fn rate_of(id: &str) -> Option<u64> {
    TUBE.rate_of(id).or_else(|| PIPE.rate_of(id))
}

/// Pipes and tubes take from each other only the way a network does (never as a plain receiver), so no family entry is
/// needed for them here: a tube is a receiver of a tube's `push_chemical` only when the giver is not a tube.
fn carrier_receiver(id: &str, _: &Compound, chemical: &str, from_tube: bool, _: [i32; 3]) -> Option<(&'static str, u64)> {
    if from_tube {
        return None;
    }
    if is_fluid(chemical) { PIPE.rate_of(id).map(|r| ("fluid", r)) } else { TUBE.rate_of(id).map(|r| ("chemical", r)) }
}

pub(crate) fn register(r: &mut dyn Registrar) {
    transmit::register_chemical_family(transmit::ChemicalFamily { serves: |_| 0, receiver: carrier_receiver, source: |_, _| None });
    for (type_id, flavor) in [(TUBE_TYPE, TUBE), (PIPE_TYPE, PIPE)] {
        let blocks = flavor.blocks();
        let blocks: Vec<&str> = blocks.iter().map(String::as_str).collect();
        let kind = r.block_entity_type(type_id).blocks(&blocks).build();
        r.block_entity_part(&kind, CarrierPart(flavor)).expect("native block entity type");
    }
}

struct CarrierPart(Flavor);

#[mod_api::horizon_part]
impl BlockPart for CarrierPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for CarrierPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let flavor = self.0;
        let Some(level) = transmit::level(&*host) else {
            return;
        };
        let mut own = Own { pos: event.pos, entity: &mut event.entity };
        let Some(net) = transmit::claim(&*host, &mut own, level, &|h, o, l, p| discover(flavor, h, o, l, p)) else {
            return;
        };
        step(host, &mut own, level, flavor, net);
    }
}

/// The transmitters of `flavor` joined to the one at `start` through connections neither side disabled, and the
/// acceptors they reach. A tube or pipe holding another chemical than the network's does not join it
/// (`CompatibleChemicalTransmitterValidator` / `CompatibleFluidTransmitterValidator`).
fn discover(flavor: Flavor, host: &dyn Host, own: &mut Own<'_, '_>, level: LevelRef, start: [i32; 3]) -> Discovered {
    let mut members = Vec::new();
    let mut targets = Vec::new();
    let mut resource: Option<String> = None;
    let mut seen = HashSet::from([start]);
    let mut queue = vec![start];
    while let Some(pos) = queue.pop() {
        let Some((_, nbt)) = transmit::read(host, own, level, pos) else {
            continue;
        };
        let held = held_chemical(&nbt, flavor);
        if pos != start && matches!((&resource, &held), (Some(a), Some(b)) if a != b) {
            continue;
        }
        members.push(pos);
        if resource.is_none() {
            resource = held;
        }
        for (face, (_, d)) in DIRECTIONS.into_iter().enumerate() {
            let conn = conn_of(&nbt, face);
            if conn == 3 {
                continue;
            }
            let at = offset(pos, d);
            let Some(name) = transmit::block_id(host, level, at) else {
                continue;
            };
            if flavor.rate_of(&name).is_some() {
                let back = face_index(neg(d));
                let joined = transmit::read(host, own, level, at).is_some_and(|(_, n)| conn_of(&n, back) != 3);
                if joined && seen.insert(at) {
                    queue.push(at);
                }
                continue;
            }
            // `NetworkAcceptorCache`: the acceptor's side that faces the transmitter; a pulling face only takes from it.
            if transmit::carrier_serves(&name, flavor.fluid) && conn != 2 {
                targets.push((at, neg(d), d));
            }
        }
    }
    Discovered { members, targets }
}

/// The chemical a transmitter holds, if any.
fn held_chemical(nbt: &Compound, flavor: Flavor) -> Option<String> {
    let tank = get_tank(nbt, flavor.key, u64::MAX);
    tank.chemical.filter(|_| tank.amount > 0)
}

/// What the block at `at` takes of `amount` of `chemical` from the network through its face `back`, and with
/// `execute` takes it.
fn accept(
    host: &mut dyn ServerHost,
    own: &mut Own<'_, '_>,
    level: LevelRef,
    at: [i32; 3],
    back: [i32; 3],
    d: [i32; 3],
    chemical: &str,
    amount: u64,
    execute: bool,
) -> u64 {
    let kind = if is_fluid(chemical) { Transmission::Fluid } else { Transmission::Chemical };
    transmit::update(host, own, level, at, |id, nbt| {
        if !takes_in(nbt, back, kind) {
            return 0;
        }
        let Some((slot, capacity)) = chemical_receiver(id, nbt, chemical, true, d) else {
            return 0;
        };
        let mut to = get_tank(nbt, slot, capacity);
        let n = to.insert(chemical, amount);
        if execute {
            put_tank(nbt, slot, &to);
        }
        n
    })
    .unwrap_or(0)
}

/// One tick of a tube or pipe network.
fn step(host: &mut dyn ServerHost, own: &mut Own<'_, '_>, level: LevelRef, flavor: Flavor, net: u64) {
    let Some((members, targets)) = transmit::with_registry(|r| r.nets.get(&net).map(|n| (n.members.clone(), n.targets.clone())))
    else {
        return;
    };
    let key = flavor.key;
    let mut rates: Vec<u64> = Vec::new();
    let mut tanks: Vec<ChemicalTank> = Vec::new();
    let mut pulls: Vec<([i32; 3], usize, usize)> = Vec::new();
    for (i, p) in members.iter().enumerate() {
        let (tank, rate, pulling) = match transmit::read(&*host, own, level, *p) {
            Some((id, nbt)) => {
                let rate = flavor.rate_of(&id).unwrap_or(0);
                let pulling: Vec<usize> = (0..6).filter(|f| conn_of(&nbt, *f) == 2).collect();
                (get_tank(&nbt, key, rate), rate, pulling)
            }
            None => (ChemicalTank::new(0), 0, Vec::new()),
        };
        rates.push(rate);
        tanks.push(tank);
        pulls.extend(pulling.into_iter().map(|f| (*p, f, i)));
    }
    let capacity: u64 = rates.iter().sum();
    // The network holds the chemical of the first transmitter that has one.
    let mut chemical = tanks.iter().find(|t| t.amount > 0).and_then(|t| t.chemical.clone());
    let holds = |t: &ChemicalTank, c: &Option<String>| t.amount > 0 && t.chemical == *c;
    let mut buffer: u64 = tanks.iter().filter(|t| holds(t, &chemical)).map(|t| t.amount).sum();
    // A pull face takes from the tank beside it (`ConnectionType.PULL`).
    for (p, face, i) in pulls {
        let d = DIRECTIONS[face].1;
        let at = offset(p, d);
        let back = neg(d);
        let room = rates[i].min(capacity.saturating_sub(buffer));
        if room == 0 {
            continue;
        }
        let wanted = chemical.clone();
        let kind = if flavor.fluid { Transmission::Fluid } else { Transmission::Chemical };
        let taken = transmit::update(host, own, level, at, |id, nbt| {
            let (slot, tank_capacity) = chemical_source(id, flavor.fluid)?;
            if !sides_of(nbt, kind).get(face_index(back)).gives_out() {
                return None;
            }
            let mut theirs = get_tank(nbt, slot, tank_capacity);
            let found = theirs.chemical.clone()?;
            if wanted.as_ref().is_some_and(|w| *w != found) {
                return None;
            }
            let n = theirs.extract(room);
            put_tank(nbt, slot, &theirs);
            Some((found, n))
        });
        if let Some(Some((found, n))) = taken {
            chemical = Some(found);
            buffer += n;
        }
    }
    let mut sent = 0;
    if let Some(id) = chemical.clone()
        && buffer > 0
        && !targets.is_empty()
    {
        let caps: Vec<u64> =
            targets.iter().map(|(at, back, d)| accept(host, own, level, *at, *back, *d, &id, buffer, false)).collect();
        let amounts = distribution::split(&caps, buffer);
        for ((at, back, d), amount) in targets.iter().zip(amounts) {
            if amount > 0 {
                sent += accept(host, own, level, *at, *back, *d, &id, amount, true);
            }
        }
    }
    buffer -= sent.min(buffer);
    // The transmitters that hold another chemical keep it and take no share.
    let free: Vec<u64> =
        rates.iter().zip(&tanks).map(|(r, t)| if t.amount == 0 || holds(t, &chemical) { *r } else { 0 }).collect();
    let shares = super::cable::share_out(buffer, &free);
    for ((p, share), skip) in members.iter().zip(shares).zip(free.iter().map(|f| *f == 0)) {
        transmit::update(host, own, level, *p, |id, nbt| {
            put_long(nbt, "flow", sent);
            if skip {
                return;
            }
            let rate = flavor.rate_of(id).unwrap_or(0);
            let mut tank = ChemicalTank::new(rate);
            if let Some(c) = chemical.as_deref().filter(|_| share > 0) {
                tank.insert(c, share);
            }
            put_tank(nbt, key, &tank);
        });
    }
}
