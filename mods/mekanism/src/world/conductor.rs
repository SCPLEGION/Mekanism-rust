//! Thermodynamic conductors (`TileEntityThermodynamicConductor`). Port of `tick_conductor` of `runtime.rs`: heat flows
//! into the colder conductors and heat handlers beside it through the connections that are on, then it loses some to
//! the air through all six sides. What it gave to acceptors (not to other conductors) and lost is kept for the
//! Network Reader.
use super::nbt::{get_double, put_double};
use super::transmit::{self, conn_of, face_index, neg, offset, Own, DIRECTIONS};
use mekanism_core::heat::HeatCapacitor;
use mekanism_core::tier::{self, BaseTier};
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    LevelRef, Registrar, ServerHost,
};

/// The id of the block entity type of the conductors (`mekanism:conductor`).
pub(crate) const TYPE: &str = "conductor";

const SUFFIX: &str = "_thermodynamic_conductor";

fn blocks() -> Vec<String> {
    BaseTier::ALL
        .into_iter()
        .filter(|t| tier::conductor(*t).is_some())
        .map(|t| format!("mekanism:{}{SUFFIX}", t.name()))
        .collect()
}

/// (heat capacity, inverse conduction, inverse insulation) of the conductor block `id`.
pub(crate) fn params_of(id: &str) -> Option<(f64, f64, f64)> {
    let tier = id.strip_prefix("mekanism:")?.strip_suffix(SUFFIX)?;
    BaseTier::ALL.into_iter().find(|t| t.name() == tier).and_then(tier::conductor)
}

/// A conductor takes heat from another one like any heat handler, through its own capacitor.
fn conductor_sink(id: &str, nbt: &mut Compound, _: [i32; 3], heat: &mut HeatCapacitor) -> Option<f64> {
    Some(transmit::plain_heat_sink(nbt, params_of(id)?, heat))
}

pub(crate) fn register(r: &mut dyn Registrar) {
    transmit::register_heat_sink(conductor_sink);
    let blocks = blocks();
    let blocks: Vec<&str> = blocks.iter().map(String::as_str).collect();
    let kind = r.block_entity_type(TYPE).blocks(&blocks).build();
    r.block_entity_part(&kind, ConductorPart).expect("native block entity type");
}

struct ConductorPart;

#[mod_api::horizon_part]
impl BlockPart for ConductorPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for ConductorPart {
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
        let Some((capacity, conduction, insulation)) = params_of(event.entity.id()) else {
            return;
        };
        let mut own = Own { pos: event.pos, entity: &mut event.entity };
        tick(host, &mut own, level, (capacity, conduction, insulation));
    }
}

/// `ThermodynamicConductor.simulate`.
fn tick(host: &mut dyn ServerHost, own: &mut Own<'_, '_>, level: LevelRef, params: (f64, f64, f64)) {
    let (capacity, conduction, insulation) = params;
    let pos = own.pos;
    let ambient = transmit::ambient_at(&*host, level, pos);
    let mut heat = HeatCapacitor::with_ambient(capacity, conduction, insulation, ambient);
    if let Some(stored) = get_double(&own.entity.nbt, "heat") {
        heat.heat = stored;
    }
    // Which faces lead to another conductor and which to something else.
    let (mut conductors, mut acceptors) = (0u8, 0u8);
    let nbt = own.entity.nbt.clone();
    for (face, (_, d)) in DIRECTIONS.into_iter().enumerate() {
        if conn_of(&nbt, face) == 3 {
            continue;
        }
        let at = offset(pos, d);
        let Some(name) = transmit::block_id(&*host, level, at) else {
            continue;
        };
        if params_of(&name).is_some() {
            let back = face_index(neg(d));
            if transmit::read(&*host, own, level, at).is_some_and(|(_, n)| conn_of(&n, back) != 3) {
                conductors |= 1 << face;
            }
        } else {
            acceptors |= 1 << face;
        }
    }
    // Heat between two conductors does not count as a transfer (`countsAsAdjacent`).
    adjacent(host, own, level, &mut heat, conductors);
    let moved = adjacent(host, own, level, &mut heat, acceptors);
    let mut lost = 0.0;
    for _ in 0..6 {
        lost += heat.lose_to_air(insulation);
    }
    put_double(&mut own.entity.nbt, "heat", heat.heat);
    put_double(&mut own.entity.nbt, "moved", moved);
    put_double(&mut own.entity.nbt, "lost", lost);
}

/// `adjacent_heat_faces`: into each colder neighbour through the faces in `faces` (a bit per face); the temperature moved.
fn adjacent(host: &mut dyn ServerHost, own: &mut Own<'_, '_>, level: LevelRef, heat: &mut HeatCapacitor, faces: u8) -> f64 {
    let mut total = 0.0;
    for (face, (_, d)) in DIRECTIONS.into_iter().enumerate() {
        if faces & (1 << face) == 0 {
            continue;
        }
        let at = offset(own.pos, d);
        let back = neg(d);
        let moved = transmit::update(host, own, level, at, |id, nbt| transmit::heat_into(id, nbt, back, heat)).flatten();
        if let Some(moved) = moved
            && moved != 0.0
        {
            total += moved;
        }
    }
    total
}
