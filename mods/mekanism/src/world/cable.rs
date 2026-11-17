//! Universal cables (`TileEntityUniversalCable`) and their energy network (`EnergyNetwork`). Port of `tick_cable`,
//! `energy_network_step`, `network_claim` and `discover` of `runtime.rs`.
//!
//! The cables of a network are one buffer: the pulling faces fill it from the energy cubes beside them, then
//! `EmitUtils` splits it among the acceptors, and what is left is shared back to the cables in proportion to what
//! each holds at most. The first cable of a network to tick in a game tick runs the whole network that tick.
use super::nbt::{get_long, put_long};
use super::transmit::{
    self, conn_of, direction, face_index, neg, offset, sides_of, takes_in, Discovered, Own, Receiver, DIRECTIONS,
};
use mekanism_core::distribution;
use mekanism_core::energy::{Action, EnergyContainer};
use mekanism_core::sides::Transmission;
use mekanism_core::tier::{self, BaseTier};
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    LevelRef, Registrar, ServerHost,
};
use std::collections::HashSet;

/// The id of the block entity type of the cables (`mekanism:cable`).
pub(crate) const TYPE: &str = "cable";

const SUFFIX: &str = "_universal_cable";

/// The block ids of the cables, one a tier.
fn blocks() -> Vec<String> {
    BaseTier::ALL
        .into_iter()
        .filter(|t| tier::universal_cable(*t).is_some())
        .map(|t| format!("mekanism:{}{SUFFIX}", t.name()))
        .collect()
}

/// What a cable of block `id` carries in a tick.
pub(crate) fn rate_of(id: &str) -> Option<u64> {
    let tier = id.strip_prefix("mekanism:")?.strip_suffix(SUFFIX)?;
    BaseTier::ALL.into_iter().find(|t| t.name() == tier).and_then(tier::universal_cable)
}

/// The energy a block takes from a cable network: another cable only when it is not the giver.
fn cable_receiver(id: &str, _: &Compound, _: bool, from_cable: bool) -> Option<Receiver> {
    if from_cable {
        return None;
    }
    rate_of(id).map(|rate| Receiver::new(rate, rate))
}

/// An energy cube's tier from its block id.
fn cube_tier(id: &str) -> Option<BaseTier> {
    let tier = id.strip_prefix("mekanism:")?.strip_suffix("_energy_cube")?;
    BaseTier::ALL.into_iter().find(|t| t.name() == tier && *t != BaseTier::Creative)
}

/// The five basic electric machines take energy by their capacity with the upgrades installed; energy cubes by
/// their tier, but not on their front.
fn machine_receiver(id: &str, nbt: &Compound, facing_away: bool, _: bool) -> Option<Receiver> {
    if let Some((_, usage)) = mekanism_core::machine::machine(id) {
        let m = mekanism_core::machine::ElectricMachine::with_upgrades(usage, super::common::upgrades(nbt));
        return Some(Receiver::new(m.energy.capacity, u64::MAX));
    }
    let (capacity, rate) = tier::energy_cube(cube_tier(id).filter(|_| facing_away)?);
    Some(Receiver::new(capacity, rate))
}

pub(crate) fn register(r: &mut dyn Registrar) {
    transmit::register_energy_receiver(cable_receiver);
    transmit::register_energy_receiver(machine_receiver);
    let blocks = blocks();
    let blocks: Vec<&str> = blocks.iter().map(String::as_str).collect();
    let kind = r.block_entity_type(TYPE).blocks(&blocks).build();
    r.block_entity_part(&kind, CablePart).expect("native block entity type");
}

struct CablePart;

#[mod_api::horizon_part]
impl BlockPart for CablePart {}

#[mod_api::horizon_part]
impl BlockEntityPart for CablePart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(level) = transmit::level(&*host) else {
            return;
        };
        let mut own = Own { pos: event.pos, entity: &mut event.entity };
        let Some(net) = transmit::claim(host, &mut own, level, &discover) else {
            return;
        };
        step(host, &mut own, level, net);
    }
}

/// The cables joined to the one at `start` through connections neither side disabled, and the acceptors they reach
/// (`NetworkAcceptorCache`). A pulling face only takes from its acceptor; the others are emitted to.
fn discover(host: &dyn Host, own: &mut Own<'_, '_>, level: LevelRef, start: [i32; 3]) -> Discovered {
    let mut members = Vec::new();
    let mut targets = Vec::new();
    let mut seen = HashSet::from([start]);
    let mut queue = vec![start];
    while let Some(pos) = queue.pop() {
        let Some((_, nbt)) = transmit::read(host, own, level, pos) else {
            continue;
        };
        members.push(pos);
        for (face, (_, d)) in DIRECTIONS.into_iter().enumerate() {
            let conn = conn_of(&nbt, face);
            if conn == 3 {
                continue;
            }
            let at = offset(pos, d);
            let Some(name) = transmit::block_id(host, level, at) else {
                continue;
            };
            if rate_of(&name).is_some() {
                let back = face_index(neg(d));
                let joined = transmit::read(host, own, level, at).is_some_and(|(_, n)| conn_of(&n, back) != 3);
                if joined && seen.insert(at) {
                    queue.push(at);
                }
                continue;
            }
            let Some((id, other)) = transmit::read(host, own, level, at) else {
                continue;
            };
            if transmit::energy_receiver(&id, &other, true, true).is_some() && conn != 2 {
                targets.push((at, neg(d), d));
            }
        }
    }
    Discovered { members, targets }
}

/// What the block at `at` takes of `amount` from a cable network through its face `back` (the one toward the
/// network), and with `execute` takes it (`EnergyNetwork.tickEmit`'s `insert`).
fn accept(host: &mut dyn ServerHost, own: &mut Own<'_, '_>, level: LevelRef, at: [i32; 3], back: [i32; 3], amount: u64, execute: bool) -> u64 {
    let accepted = level.block_property(&*host, at, "facing").and_then(|f| direction(&f)).is_none_or(|front| front != back);
    transmit::update(host, own, level, at, |id, nbt| {
        if !takes_in(nbt, back, Transmission::Energy) {
            return 0;
        }
        let Some(mut receiver) = transmit::energy_receiver(id, nbt, accepted, true) else {
            return 0;
        };
        if let Some(key) = receiver.counted {
            receiver.rate = receiver.rate.saturating_sub(get_long(nbt, key));
        }
        let mut to = EnergyContainer::new(receiver.capacity, receiver.rate);
        to.stored = get_long(nbt, "energy").min(receiver.capacity);
        let refused = to.insert(amount, if execute { Action::Execute } else { Action::Simulate });
        if execute {
            put_long(nbt, "energy", to.stored);
            if let Some(key) = receiver.counted {
                let counted = get_long(nbt, key) + (amount - refused);
                put_long(nbt, key, counted);
            }
        }
        amount - refused
    })
    .unwrap_or(0)
}

/// One tick of a cable network (`EnergyNetwork.onUpdate`).
fn step(host: &mut dyn ServerHost, own: &mut Own<'_, '_>, level: LevelRef, net: u64) {
    let Some((members, targets)) = transmit::with_registry(|r| r.nets.get(&net).map(|n| (n.members.clone(), n.targets.clone()))) else {
        return;
    };
    let mut rates: Vec<u64> = Vec::new();
    let mut buffer = 0u64;
    let mut pulls: Vec<([i32; 3], usize, usize)> = Vec::new();
    for (i, p) in members.iter().enumerate() {
        let (stored, rate, pulling) = match transmit::read(&*host, own, level, *p) {
            Some((id, nbt)) => {
                let rate = rate_of(&id).unwrap_or(0);
                let pulling: Vec<usize> = (0..6).filter(|f| conn_of(&nbt, *f) == 2).collect();
                (get_long(&nbt, "energy").min(rate), rate, pulling)
            }
            None => (0, 0, Vec::new()),
        };
        buffer += stored;
        rates.push(rate);
        pulls.extend(pulling.into_iter().map(|f| (*p, f, i)));
    }
    let capacity: u64 = rates.iter().sum();
    // A pull face takes energy out of the energy cube beside it (`ConnectionType.PULL`).
    for (p, face, i) in pulls {
        let d = DIRECTIONS[face].1;
        let at = offset(p, d);
        let back = neg(d);
        let room = rates[i].min(capacity.saturating_sub(buffer));
        let taken = transmit::update(host, own, level, at, |id, nbt| {
            let Some(t) = cube_tier(id) else {
                return 0;
            };
            if !sides_of(nbt, Transmission::Energy).get(face_index(back)).gives_out() {
                return 0;
            }
            let stored = get_long(nbt, "energy");
            let give = stored.min(tier::energy_cube(t).1).min(room);
            put_long(nbt, "energy", stored - give);
            give
        });
        buffer += taken.unwrap_or(0);
    }
    // `tickEmit`: an even split among the acceptors, those that want less leaving more for the rest.
    let mut sent = 0;
    if buffer > 0 && !targets.is_empty() {
        let caps: Vec<u64> = targets.iter().map(|(at, back, _)| accept(host, own, level, *at, *back, buffer, false)).collect();
        let amounts = distribution::split(&caps, buffer);
        for ((at, back, _), amount) in targets.iter().zip(amounts) {
            if amount > 0 {
                sent += accept(host, own, level, *at, *back, amount, true);
            }
        }
    }
    buffer -= sent.min(buffer);
    // `updateSaveShares`: each cable keeps its part of what is left.
    let shares = share_out(buffer, &rates);
    for (p, share) in members.iter().zip(shares) {
        transmit::update(host, own, level, *p, |_, nbt| {
            put_long(nbt, "energy", share);
            put_long(nbt, "flow", sent);
        });
    }
}

/// How `total` is shared among holders that can hold `caps` each, in proportion to those, the remainder going one
/// unit at a time to the first that have room (`EmitUtils` on the save targets).
pub(crate) fn share_out(total: u64, caps: &[u64]) -> Vec<u64> {
    let sum: u64 = caps.iter().sum();
    if sum == 0 {
        return vec![0; caps.len()];
    }
    let total = total.min(sum);
    let mut out: Vec<u64> = caps.iter().map(|c| (u128::from(total) * u128::from(*c) / u128::from(sum)) as u64).collect();
    let mut left = total - out.iter().sum::<u64>();
    for (o, c) in out.iter_mut().zip(caps) {
        if left == 0 {
            break;
        }
        if *o < *c {
            *o += 1;
            left -= 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_left_is_shared_by_capacity_and_never_exceeds_it() {
        // 100 over holders of 10, 30 and 60: 10, 30, 60.
        assert_eq!(share_out(100, &[10, 30, 60]), vec![10, 30, 60]);
        // 50 over the same: half of each, 5, 15, 30.
        assert_eq!(share_out(50, &[10, 30, 60]), vec![5, 15, 30]);
        // 7 over three equal holders: 2 each and the remainder to the first.
        assert_eq!(share_out(7, &[10, 10, 10]), vec![3, 2, 2]);
        assert_eq!(share_out(5, &[0, 0]), vec![0, 0]);
    }
}
