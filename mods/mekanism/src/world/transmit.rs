//! What every transmitter family (cables, tubes, pipes, conductors, transporters) shares: the six faces, the
//! connection bits a transmitter keeps in its compound, reading and writing the block entity beside it, and the
//! registry of energy acceptors the other families fill in as they are ported.
//!
//! A ticking block entity is out of the world (its position reads as an empty block entity), so a network that
//! runs from one of its transmitters reaches that one through [`Own`] and the others through the host, exactly as
//! `member` did on the fork (`native_with_mod_block_entity`).
use super::nbt::{get_int, get_long};
use mekanism_core::sides::{Sides, Transmission};
use mod_api::{Compound, Host, LevelRef, ModBlockEntityView, ServerHost};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// The six faces in the order the saved bits use (down, up, north, south, west, east).
pub(crate) const DIRECTIONS: [(&str, [i32; 3]); 6] = [
    ("down", [0, -1, 0]),
    ("up", [0, 1, 0]),
    ("north", [0, 0, -1]),
    ("south", [0, 0, 1]),
    ("west", [-1, 0, 0]),
    ("east", [1, 0, 0]),
];

pub(crate) fn face_index(d: [i32; 3]) -> usize {
    DIRECTIONS.iter().position(|(_, v)| *v == d).unwrap_or(0)
}

pub(crate) fn direction(name: &str) -> Option<[i32; 3]> {
    DIRECTIONS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

pub(crate) fn offset(pos: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]
}

pub(crate) fn neg(d: [i32; 3]) -> [i32; 3] {
    [-d[0], -d[1], -d[2]]
}

/// A transmitter's connection on one face (`ConnectionType`): 0 normal, 1 push, 2 pull, 3 none; two bits a face in `conn`.
pub(crate) fn conn_of(nbt: &Compound, face: usize) -> u32 {
    (get_int(nbt, "conn") >> (2 * face)) & 3
}

/// The faces' modes of a block for one transmission type.
pub(crate) fn sides_of(nbt: &Compound, t: Transmission) -> Sides {
    Sides(get_long(nbt, t.key()) as u32)
}

/// Whether a receiver takes in `t` through its face `back` (the one facing the giver).
pub(crate) fn takes_in(nbt: &Compound, back: [i32; 3], t: Transmission) -> bool {
    sides_of(nbt, t).get(face_index(back)).takes_in()
}

/// The level the host runs in.
pub(crate) fn level(host: &dyn Host) -> Option<LevelRef> {
    host.level(host.dimension())
}

/// The block id at `pos` (`mekanism:basic_universal_cable`), None for an unloaded chunk.
pub(crate) fn block_id(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<String> {
    level.block(host, pos).map(|key| key.id(host))
}

/// The block entity the caller is ticking, which is out of the world.
pub(crate) struct Own<'a, 'b> {
    pub(crate) pos: [i32; 3],
    pub(crate) entity: &'a mut ModBlockEntityView<'b>,
}

/// The block id and saved data of the block entity at `pos`; the ticking one answers for its own position.
pub(crate) fn read(host: &dyn Host, own: &Own<'_, '_>, level: LevelRef, pos: [i32; 3]) -> Option<(String, Compound)> {
    if pos == own.pos {
        return Some((own.entity.id().to_owned(), own.entity.nbt.clone()));
    }
    let entity = host.block_entity(level, pos)?;
    let id = block_id(host, level, pos)?;
    Some((id, entity.data(host)?))
}

/// Changes the saved data of the block entity at `pos` with `change`, as `native_with_mod_block_entity` did. Nothing
/// is written (and no block update goes out) when `change` left the data as it was. None when there is no block entity.
pub(crate) fn update<R>(
    host: &mut dyn ServerHost,
    own: &mut Own<'_, '_>,
    level: LevelRef,
    pos: [i32; 3],
    change: impl FnOnce(&str, &mut Compound) -> R,
) -> Option<R> {
    if pos == own.pos {
        let id = own.entity.id().to_owned();
        return Some(change(&id, &mut own.entity.nbt));
    }
    let entity = host.block_entity(level, pos)?;
    let id = block_id(host, level, pos)?;
    let before = entity.data(host)?;
    let mut data = before.clone();
    let result = change(&id, &mut data);
    if data != before {
        let _ = entity.merge_data(host, data);
    }
    Some(result)
}

/// Bumped whenever a block that may join or leave a network changes; a network built under an older number is rebuilt.
static TOPOLOGY: AtomicU64 = AtomicU64::new(1);

/// A transmitter or something it joins has been placed, broken or reconnected: networks form again from what is there
/// (`TransmitterNetworkRegistry` merging and splitting on `onNeighborChange`). The Configurator calls it too, after
/// it changes a connection.
pub(crate) fn network_changed() {
    TOPOLOGY.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn topology() -> u64 {
    TOPOLOGY.load(Ordering::Relaxed)
}

/// A block became another: networks form again when either was a Mekanism block (a transmitter or what it joins).
pub(crate) fn block_changed(level: &dyn mod_api::LevelHost, event: &mod_api::BlockChangedEvent) {
    let mekanism = |s: mod_api::BlockState| level.state_string(s).is_some_and(|n| n.starts_with("mekanism"));
    if mekanism(event.old) || mekanism(event.new) {
        network_changed();
    }
}

/// What a block takes of energy on one face: its capacity, the most one insert moves, and the compound key that
/// counts what came in this tick when the block limits the rate per tick (the matrix's first port, `queuedInput`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Receiver {
    pub(crate) capacity: u64,
    pub(crate) rate: u64,
    pub(crate) counted: Option<&'static str>,
}

impl Receiver {
    pub(crate) fn new(capacity: u64, rate: u64) -> Self {
        Self { capacity, rate, counted: None }
    }
}

/// What a family answers for a block it owns: `(block id, saved data, facing_away, from_cable)` to its [`Receiver`],
/// None when the block is not its or takes no energy. `facing_away` is false on the face an energy cube or laser
/// refuses (its front); `from_cable` is true when a cable is the giver (cables take from each other another way).
pub(crate) type EnergyReceiver = fn(&str, &Compound, bool, bool) -> Option<Receiver>;

static ENERGY_RECEIVERS: Mutex<Vec<EnergyReceiver>> = Mutex::new(Vec::new());

/// Declares a family's energy acceptors, during `register`. Every family that takes energy adds one line.
pub(crate) fn register_energy_receiver(receiver: EnergyReceiver) {
    ENERGY_RECEIVERS.lock().unwrap().push(receiver);
}

/// `receiver` of `runtime.rs`: what the block `id` with `nbt` takes of energy.
pub(crate) fn energy_receiver(id: &str, nbt: &Compound, facing_away: bool, from_cable: bool) -> Option<Receiver> {
    ENERGY_RECEIVERS.lock().unwrap().iter().find_map(|f| f(id, nbt, facing_away, from_cable))
}

/// A block that takes heat from a thermodynamic conductor: `(block id, its saved data, the face toward the giver,
/// the giver's heat)` to the temperature moved, None when it is not this family's or holds no heat. The sink changes
/// its own data (`heat`); the giver's heat is changed by `give_to`.
pub(crate) type HeatSink = fn(&str, &mut Compound, [i32; 3], &mut mekanism_core::heat::HeatCapacitor) -> Option<f64>;

static HEAT_SINKS: Mutex<Vec<HeatSink>> = Mutex::new(Vec::new());

/// Declares a family's heat acceptors, during `register`. A family whose heat is a plain capacitor in `heat` with
/// fixed coefficients passes [`plain_heat_sink`] through its own closure; the boiler and the entangloporter write theirs.
pub(crate) fn register_heat_sink(sink: HeatSink) {
    HEAT_SINKS.lock().unwrap().push(sink);
}

/// Gives `heat` to the block `id` with `nbt` beside a conductor through its face `back`: the first family that owns it.
pub(crate) fn heat_into(id: &str, nbt: &mut Compound, back: [i32; 3], heat: &mut mekanism_core::heat::HeatCapacitor) -> Option<f64> {
    let sinks = HEAT_SINKS.lock().unwrap().clone();
    sinks.into_iter().find_map(|f| f(id, nbt, back, heat))
}

/// `ITileHeatHandler.simulateAdjacent` into a block with one capacitor of fixed coefficients stored as `heat`.
pub(crate) fn plain_heat_sink(
    nbt: &mut Compound,
    params: (f64, f64, f64),
    heat: &mut mekanism_core::heat::HeatCapacitor,
) -> f64 {
    let (capacity, conduction, insulation) = params;
    let mut sink = mekanism_core::heat::HeatCapacitor::with_ambient(capacity, conduction, insulation, heat.ambient);
    if let Some(stored) = super::nbt::get_double(nbt, "heat") {
        sink.heat = stored;
    }
    let moved = heat.give_to(&mut sink);
    nbt.insert("heat".into(), mod_api::Tag::Double(sink.heat.to_bits()));
    moved
}

/// `HeatAPI.getAmbientTemp(level, pos)`: the biome's temperature there (the frozen modifier's noise is not applied).
pub(crate) fn ambient_at(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> f64 {
    let temperature = level
        .biome_at(host, pos)
        .and_then(|biome| level.biome_properties(host, &biome))
        .map_or(0.8, |p| f64::from(p.temperature));
    mekanism_core::heat::ambient_temp(temperature)
}
