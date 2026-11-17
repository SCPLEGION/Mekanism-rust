//! Chemical tanks, fluid tanks and the radioactive waste barrel. Port of `tick_chemical_tank`, `tick_fluid_tank`,
//! `container_slots` and `tick_waste_barrel` of `runtime.rs`. Tanks also answer the pipes and tubes: they take from
//! every side but their top and can be emptied by a pulling face.
use super::chem::{
    bucket_fluid, drain_tank_into_bucket, drain_tank_into_item, fill_tank_from_bucket, fill_tank_from_item, get_tank, put_item,
    push_chemical, put_tank, take_one,
};
use super::common::{MenuData, put_client};
use super::multiblock::chemical_index;
use super::nbt::{get_int, put_int};
use super::transmit::{self, ChemicalFamily, is_fluid};
use mekanism_core::chemical::{self as chem, ChemicalTank};
use mekanism_core::tier::{self, BaseTier};
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    ModBlockEntityView, ModContainer, Registrar, ServerHost,
};

pub(crate) const CHEMICAL_TYPE: &str = "chemical_tank";
pub(crate) const FLUID_TYPE: &str = "fluid_tank";
pub(crate) const WASTE_TYPE: &str = "waste_barrel";
pub(crate) const WASTE_BARREL: &str = "mekanism:radioactive_waste_barrel";

fn chemical_tier(id: &str) -> Option<BaseTier> {
    let t = id.strip_prefix("mekanism:")?.strip_suffix("_chemical_tank")?;
    BaseTier::ALL.into_iter().find(|b| b.name() == t)
}

fn fluid_tier(id: &str) -> Option<BaseTier> {
    let t = id.strip_prefix("mekanism:")?.strip_suffix("_fluid_tank")?;
    BaseTier::ALL.into_iter().find(|b| b.name() == t)
}

/// The radioactivity of a chemical (`mekanism:data_maps/radioactivity`), per mB.
pub(crate) fn radioactivity(chemical: &str) -> f64 {
    match chemical {
        "mekanism:nuclear_waste" | "mekanism:spent_nuclear_waste" => 0.01,
        "mekanism:plutonium" => 0.02,
        "mekanism:polonium" => 0.05,
        _ => 0.0,
    }
}

/// `GeneralConfig.radioactiveWasteBarrelMaxChemical`.
fn waste_barrel_capacity() -> u64 {
    mekanism_core::config_files::active_int("general", "radiation", "wasteBarrelCapacity", 512_000).max(1)
}

pub(crate) fn register(r: &mut dyn Registrar) {
    transmit::register_chemical_family(ChemicalFamily { serves: serves, receiver: receiver, source: source });
    let chemical: Vec<String> = BaseTier::ALL.into_iter().map(|t| format!("mekanism:{}_chemical_tank", t.name())).collect();
    let chemical: Vec<&str> = chemical.iter().map(String::as_str).collect();
    let entity = r.block_entity_type(CHEMICAL_TYPE).blocks(&chemical).build();
    r.block_entity_part(&entity, ChemicalTankPart).expect("native block entity type");
    for id in &chemical {
        let block = r.block_key(id);
        // `TileEntityChemicalTank`: the drain slot (a tank item takes the chemical) and the fill slot (it gives it).
        r.block_container(
            &block,
            ModContainer {
                slots: 5,
                faces: std::array::from_fn(|_| vec![0, 1]),
                inputs: vec![1],
                outputs: vec![0],
                menu: Some("minecraft:hopper".into()),
            },
        );
    }
    let fluid: Vec<String> = BaseTier::ALL.into_iter().map(|t| format!("mekanism:{}_fluid_tank", t.name())).collect();
    let fluid: Vec<&str> = fluid.iter().map(String::as_str).collect();
    let entity = r.block_entity_type(FLUID_TYPE).blocks(&fluid).build();
    r.block_entity_part(&entity, FluidTankPart).expect("native block entity type");
    for id in &fluid {
        let block = r.block_key(id);
        // The bucket slot and the output slot under it.
        r.block_container(
            &block,
            ModContainer {
                slots: 5,
                faces: [vec![1], vec![0], vec![1], vec![1], vec![1], vec![1]],
                inputs: vec![0],
                outputs: vec![1],
                menu: Some("minecraft:hopper".into()),
            },
        );
    }
    let entity = r.block_entity_type(WASTE_TYPE).blocks(&[WASTE_BARREL]).build();
    r.block_entity_part(&entity, WasteBarrelPart).expect("native block entity type");
}

/// Pipes reach fluid tanks, tubes reach chemical tanks.
fn serves(id: &str) -> u8 {
    u8::from(fluid_tier(id).is_some()) | u8::from(chemical_tier(id).is_some()) << 1 | u8::from(id == WASTE_BARREL) << 1
}

/// Tanks take what is piped into them on every side but the top (their bottom is their output side).
fn receiver(id: &str, nbt: &Compound, chemical: &str, from_tube: bool, d: [i32; 3]) -> Option<(&'static str, u64)> {
    if is_fluid(chemical) {
        return fluid_tier(id).filter(|_| d != [0, 1, 0]).map(|t| ("fluid", tier::fluid_tank(t).0));
    }
    if let Some(t) = chemical_tier(id).filter(|_| d != [0, 1, 0]) {
        return Some(("chemical", chem::chemical_tank(t).0));
    }
    let _ = from_tube;
    (id == WASTE_BARREL
        && radioactivity(chemical) > 0.0
        && get_tank(nbt, "chemical", waste_barrel_capacity()).accepts(chemical))
    .then(|| ("chemical", waste_barrel_capacity()))
}

/// The tank a pulling face of a pipe or tube may empty.
fn source(id: &str, fluid: bool) -> Option<(&'static str, u64)> {
    if fluid {
        fluid_tier(id).map(|t| ("fluid", tier::fluid_tank(t).0))
    } else {
        chemical_tier(id).map(|t| ("chemical", chem::chemical_tank(t).0))
    }
}

fn gauge_data(entity: &mut ModBlockEntityView<'_>, tank: &ChemicalTank) {
    MenuData::new(0, 0, 0).gauge(tank.amount, tank.capacity, chemical_index(tank.chemical.as_deref())).write(entity);
}

struct ChemicalTankPart;

#[mod_api::horizon_part]
impl BlockPart for ChemicalTankPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for ChemicalTankPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(tier) = chemical_tier(event.entity.id()) else {
            return;
        };
        tick_chemical_tank(host, event.pos, &mut event.entity, tier);
    }
}

fn tick_chemical_tank(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>, tier: BaseTier) {
    let (capacity, rate) = chem::chemical_tank(tier);
    let mut tank = get_tank(&entity.nbt, "chemical", capacity);
    let dumping = get_int(&entity.nbt, "dump") % 3;
    let creative = tier == BaseTier::Creative;
    {
        // `drainTankIntoSlot`, `fillTankFromSlot`.
        let h: &dyn Host = &*host;
        drain_tank_into_item(h, entity, 0, &mut tank);
        fill_tank_from_item(h, entity, 1, &mut tank, |_| true);
    }
    // `ChemicalUtils.dump(tank, mode, capacity / 400, transfer rate)`: dumping removes a share, dumping excess what is
    // over 85% of the tank.
    if dumping != 0 && !creative && tank.amount > 0 {
        let stored = tank.amount;
        let to_dump = if dumping == 2 {
            capacity / 400
        } else {
            let keep = (capacity as f64 * 0.85) as u64;
            if keep < stored { (stored - keep).min(rate) } else { 0 }
        };
        tank.extract(to_dump);
    }
    // Dumping does not eject (`setCanEject`).
    if tank.amount > 0 && (creative || dumping != 2) {
        push_chemical(host, pos, &entity.nbt.clone(), &mut tank, Some([0, -1, 0]), rate);
    }
    put_tank(&mut entity.nbt, "chemical", &tank);
    gauge_data(entity, &tank);
}

struct FluidTankPart;

#[mod_api::horizon_part]
impl BlockPart for FluidTankPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for FluidTankPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(tier) = fluid_tier(event.entity.id()) else {
            return;
        };
        let (capacity, rate) = tier::fluid_tank(tier);
        let mut tank = get_tank(&event.entity.nbt, "fluid", capacity);
        container_slots(&*host, &mut event.entity, &mut tank);
        if tank.amount > 0 {
            let nbt = event.entity.nbt.clone();
            push_chemical(host, event.pos, &nbt, &mut tank, Some([0, -1, 0]), rate);
        }
        put_tank(&mut event.entity.nbt, "fluid", &tank);
        // What the tank shows in the world (`RenderFluidTank`): its fluid and how full it is, in 200ths.
        let scale = if tank.amount == 0 { 0 } else { (tank.amount.saturating_mul(200) / tank.capacity.max(1)).clamp(1, 200) };
        let mut client = Compound::new();
        put_int(&mut client, "tank_scale", scale as u32);
        put_int(&mut client, "tank_fluid", chemical_index(tank.chemical.as_deref()) as u32);
        put_client(&mut event.entity, &["tank_scale", "tank_fluid"], client);
        gauge_data(&mut event.entity, &tank);
    }
}

/// `FluidInventorySlot.input` with the container edit mode (`handleContainer`, `ResourceHandlerSlot`): Fill only drains
/// the tank into the container and Empty only fills the tank from it. In Both the slot remembers its last transfer
/// direction (`lastDirectionJournal`, key `dirn`: 0 unknown, 1 draining into the item, 2 filling from it): unknown tries
/// to drain first and else to fill, moving an item the tank could never take anything from to the output slot; once
/// filling it keeps filling or moving, once draining it keeps draining (a draining item never moves on).
fn container_slots(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, tank: &mut ChemicalTank) {
    let edit = get_int(&entity.nbt, "emode") % 3;
    let mut dir = get_int(&entity.nbt, "dirn");
    let Some(held) = entity.items.first().cloned().flatten() else {
        put_int(&mut entity.nbt, "dirn", 0);
        return;
    };
    let held_id = super::item_id(host, &held);
    let drain = |entity: &mut ModBlockEntityView<'_>, tank: &mut ChemicalTank| -> bool {
        held_id == "minecraft:bucket" && drain_tank_into_bucket(host, entity, 0, Some(1), tank)
    };
    let fill = |entity: &mut ModBlockEntityView<'_>, tank: &mut ChemicalTank| -> bool {
        held_id != "minecraft:bucket" && fill_tank_from_bucket(host, entity, 0, Some(1), tank, is_fluid)
    };
    // `tryFillOrMove`'s test: the tank holds nothing, or the item holds none of what the tank does.
    let invalid = |tank: &ChemicalTank| -> bool {
        let Some(stored) = tank.chemical.as_deref().filter(|_| tank.amount > 0) else {
            return true;
        };
        bucket_fluid(&held_id).is_none_or(|f| f != stored)
    };
    let fill_or_move = |entity: &mut ModBlockEntityView<'_>, tank: &mut ChemicalTank| -> bool {
        if fill(entity, tank) {
            return true;
        }
        if invalid(tank) && put_item(host, entity, 1, &held_id) {
            take_one(entity, 0);
            return true;
        }
        false
    };
    match edit {
        1 => {
            dir = 1;
            drain(entity, tank);
        }
        2 => {
            dir = 2;
            fill(entity, tank);
        }
        _ => match dir {
            2 => {
                if !fill_or_move(entity, tank) {
                    dir = 0;
                }
            }
            1 => {
                drain(entity, tank);
            }
            _ => {
                if drain(entity, tank) {
                    dir = 1;
                } else {
                    dir = 2;
                    if !fill_or_move(entity, tank) {
                        dir = 0;
                    }
                }
            }
        },
    }
    // An emptied slot forgets its direction (`onContentsChanged`).
    if entity.items.first().is_none_or(Option::is_none) {
        dir = 0;
    }
    put_int(&mut entity.nbt, "dirn", dir);
}

struct WasteBarrelPart;

#[mod_api::horizon_part]
impl BlockPart for WasteBarrelPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for WasteBarrelPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    /// `TileEntityRadioactiveWasteBarrel.onUpdateServer`: the stored chemical decays by `wasteBarrelDecayAmount` every
    /// `wasteBarrelProcessTicks` (not plutonium or polonium), and an active barrel emits what it holds into the
    /// container below.
    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let entity = &mut event.entity;
        let mut tank = get_tank(&entity.nbt, "chemical", waste_barrel_capacity());
        if let Some(chemical) = tank.chemical.clone() {
            let decay = mekanism_core::config_files::active_int("general", "radiation", "wasteBarrelDecayAmount", 1);
            if decay > 0 && !matches!(chemical.as_str(), "mekanism:plutonium" | "mekanism:polonium") {
                let ticks = get_int(&entity.nbt, "progress") + 1;
                if u64::from(ticks) >= mekanism_core::config_files::active_int("general", "radiation", "wasteBarrelProcessTicks", 20).max(1) {
                    put_int(&mut entity.nbt, "progress", 0);
                    tank.extract(decay);
                } else {
                    put_int(&mut entity.nbt, "progress", ticks);
                }
            }
        }
        if get_int(&entity.nbt, "active") == 1 && tank.amount > 0 {
            let amount = tank.amount;
            let nbt = entity.nbt.clone();
            push_chemical(host, event.pos, &nbt, &mut tank, Some([0, -1, 0]), amount);
        }
        put_tank(&mut entity.nbt, "chemical", &tank);
    }
}
