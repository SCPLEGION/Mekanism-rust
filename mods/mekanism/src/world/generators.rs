//! The generators and the energy cubes: heat, solar, advanced solar, wind, gas-burning and bio generators
//! (`mekanismgenerators:`), and the four tiers of energy cube. Port of `tick_generator`, `tick_solar`,
//! `tick_wind`, `tick_gas_generator`, `tick_bio_generator` and `tick_cube` of `runtime.rs`, with their
//! containers and the numbers of their screens.
//!
//! Energy leaves through `world/energy.rs` (a cube's front, every face of a generator).
use super::chem::{fill_tank_from_item, get_tank, put_tank};
use super::transmit::{self, Own, DIRECTIONS};
use super::common::{charge_slot, discharge_slot, put_client, set_active};
use super::nbt::{get_double, get_int, get_long, put_int, put_long};
use crate::gear::data as item_data;
use mekanism_core::chemical::{self as chem, ChemicalTank};
use mekanism_core::energy::{Action, EnergyContainer};
use mekanism_core::generator::{self as g, HeatGenerator};
use mekanism_core::sides::Transmission;
use mekanism_core::tier::{self, BaseTier};
use mod_api::HostBlockStates as _;
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    ItemStack, ModBlockEntityView, ModContainer, Registrar, ServerHost, Tag,
};

/// The id of the block entity type of the generators and energy cubes (`mekanism:generator`).
pub(crate) const TYPE: &str = "generator";

const HEAT: &str = "mekanismgenerators:heat_generator";
const SOLAR: &str = "mekanismgenerators:solar_generator";
const ADVANCED_SOLAR: &str = "mekanismgenerators:advanced_solar_generator";
const WIND: &str = "mekanismgenerators:wind_generator";
const GAS: &str = "mekanismgenerators:gas_burning_generator";

/// What a block of this family is.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Heat,
    /// Peak generation and storage.
    Solar(u64, u64),
    Wind,
    Gas,
    Bio,
    Cube(BaseTier),
}

fn kind(id: &str) -> Option<Kind> {
    match id {
        HEAT => return Some(Kind::Heat),
        SOLAR => return Some(Kind::Solar(g::SOLAR_GENERATION, g::SOLAR_STORAGE)),
        ADVANCED_SOLAR => return Some(Kind::Solar(g::ADVANCED_SOLAR_GENERATION, g::ADVANCED_SOLAR_STORAGE)),
        WIND => return Some(Kind::Wind),
        GAS => return Some(Kind::Gas),
        g::BIO_GENERATOR => return Some(Kind::Bio),
        _ => {}
    }
    let tier = id.strip_prefix("mekanism:")?.strip_suffix("_energy_cube")?;
    BaseTier::ALL.into_iter().find(|t| t.name() == tier && *t != BaseTier::Creative).map(Kind::Cube)
}

/// An energy cube takes energy through every face but its front.
fn cube_receiver(id: &str, _: &Compound, facing_away: bool, _: bool) -> Option<transmit::Receiver> {
    let Some(Kind::Cube(tier)) = kind(id) else {
        return None;
    };
    facing_away.then(|| {
        let (capacity, rate) = tier::energy_cube(tier);
        transmit::Receiver::new(capacity, rate)
    })
}

/// The heat generator is warmed by the heat beside it (`HEAT_CAPACITY` and the generator's coefficients).
fn heat_sink(id: &str, nbt: &mut Compound, _: [i32; 3], heat: &mut mekanism_core::heat::HeatCapacitor) -> Option<f64> {
    (id == HEAT).then(|| {
        transmit::plain_heat_sink(nbt, (g::HEAT_CAPACITY, g::INVERSE_CONDUCTION_COEFFICIENT, g::INVERSE_INSULATION_COEFFICIENT), heat)
    })
}

/// Bit 0: a mechanical pipe reaches the block (lava); bit 1: a pressurized tube does (bioethanol, fuel gases).
fn serves(id: &str) -> u8 {
    match kind(id) {
        Some(Kind::Heat) => 1,
        Some(Kind::Bio | Kind::Gas) => 2,
        _ => 0,
    }
}

/// The tank a pipe or tube fills: the heat generator's `lava_in`, the bio generator's bioethanol and the gas
/// generator's fuel gas, both in `fuel`.
fn chemical_receiver(id: &str, _: &Compound, chemical: &str, _: bool, _: [i32; 3]) -> Option<(&'static str, u64)> {
    match kind(id)? {
        Kind::Heat if chemical == "minecraft:lava" => Some(("lava_in", u64::from(g::TANK_CAPACITY))),
        Kind::Bio if chemical == g::BIOETHANOL => Some(("fuel", g::BIO_TANK)),
        Kind::Gas if chem::fuel(chemical).is_some() => Some(("fuel", chem::GAS_GENERATOR_TANK)),
        _ => None,
    }
}

fn cube_id(tier: BaseTier) -> String {
    format!("mekanism:{}_energy_cube", tier.name())
}

pub(crate) fn register(r: &mut dyn Registrar) {
    transmit::register_energy_receiver(cube_receiver);
    transmit::register_heat_sink(heat_sink);
    transmit::register_chemical_family(transmit::ChemicalFamily { serves, receiver: chemical_receiver, source: |_, _| None });
    let mut ids: Vec<String> = [HEAT, SOLAR, ADVANCED_SOLAR, WIND, GAS, g::BIO_GENERATOR].map(String::from).into();
    ids.extend(BaseTier::ALL.into_iter().filter(|t| *t != BaseTier::Creative).map(cube_id));
    let blocks: Vec<&str> = ids.iter().map(String::as_str).collect();
    let entity_type = r.block_entity_type(TYPE).blocks(&blocks).build();
    r.block_entity_part(&entity_type, GeneratorPart).expect("native block entity type");
    for id in &ids {
        // The plain solar generator is an iron block with no menu.
        if let Some(c) = container(id) {
            let block = r.block_key(id);
            r.block_container(&block, c);
        }
    }
}

/// Generators charge an item in the first slot; the heat, bio and gas generators take fuel in the second;
/// the cube also empties an item into itself from the second. The wind and advanced solar generators have the
/// charge slot only.
fn container(id: &str) -> Option<ModContainer> {
    let (slots, inputs): (usize, Vec<usize>) = match kind(id)? {
        Kind::Solar(..) if id != ADVANCED_SOLAR => return None,
        Kind::Solar(..) | Kind::Wind => (1, vec![0]),
        Kind::Heat | Kind::Bio | Kind::Gas | Kind::Cube(_) => (2, vec![0, 1]),
    };
    Some(ModContainer {
        slots,
        faces: std::array::from_fn(|_| inputs.clone()),
        inputs,
        outputs: vec![],
        menu: Some(if slots == 1 { "minecraft:hopper" } else { "minecraft:furnace" }.into()),
    })
}

struct GeneratorPart;

#[mod_api::horizon_part]
impl BlockPart for GeneratorPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for GeneratorPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(kind) = kind(event.entity.id()) else {
            return;
        };
        let pos = event.pos;
        let active = match kind {
            Kind::Heat => Some(tick_heat(host, pos, &mut event.entity)),
            Kind::Solar(peak, storage) => Some(tick_solar(host, pos, &mut event.entity, peak, storage)),
            Kind::Wind => Some(tick_wind(host, pos, &mut event.entity)),
            Kind::Gas => Some(tick_gas(host, &mut event.entity)),
            Kind::Bio => Some(tick_bio(host, &mut event.entity)),
            Kind::Cube(tier) => {
                tick_cube(host, &mut event.entity, tier);
                None
            }
        };
        emit(host, pos, &mut event.entity, kind);
        if let Some(active) = active {
            set_active(host, pos, active);
        }
    }
}

/// `push_energy`: hands the buffer to the neighbours that take it (`transmit::energy_receiver`): a generator out of
/// every face its side configuration lets energy out of, a cube out of its front only.
fn emit(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>, kind: Kind) {
    let (capacity, only_front) = match kind {
        Kind::Heat => (g::STORAGE, false),
        Kind::Solar(_, storage) => (storage, false),
        Kind::Wind => (g::WIND_STORAGE, false),
        Kind::Gas => (chem::GAS_GENERATOR_STORAGE, false),
        Kind::Bio => (g::BIO_STORAGE, false),
        Kind::Cube(tier) => (tier::energy_cube(tier).0, true),
    };
    let Some(level) = transmit::level(&*host) else {
        return;
    };
    let front = only_front
        .then(|| level.block_property(&*host, pos, "facing").and_then(|f| transmit::direction(&f)))
        .flatten();
    if only_front && front.is_none() {
        return;
    }
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let before = energy.stored;
    let gives = transmit::sides_of(&entity.nbt, Transmission::Energy);
    let mut own = Own { pos, entity };
    let faces: Vec<[i32; 3]> = DIRECTIONS
        .iter()
        .map(|(_, d)| *d)
        .filter(|d| front.is_none_or(|f| f == *d) && gives.get(transmit::face_index(*d)).gives_out())
        .collect();
    for (i, d) in faces.iter().enumerate() {
        let offer = energy.extract(u64::MAX, Action::Simulate) / (faces.len() - i) as u64;
        if offer == 0 {
            continue;
        }
        let at = transmit::offset(pos, *d);
        let back = transmit::neg(*d);
        let accepted = level.block_property(&*host, at, "facing").and_then(|f| transmit::direction(&f)).is_none_or(|f| f != back);
        let taken = transmit::update(host, &mut own, level, at, |id, nbt| {
            if !transmit::takes_in(nbt, back, Transmission::Energy) {
                return 0;
            }
            let Some(mut receiver) = transmit::energy_receiver(id, nbt, accepted, false) else {
                return 0;
            };
            if let Some(key) = receiver.counted {
                receiver.rate = receiver.rate.saturating_sub(get_long(nbt, key));
            }
            let mut to = EnergyContainer::new(receiver.capacity, receiver.rate);
            to.stored = get_long(nbt, "energy").min(receiver.capacity);
            let refused = to.insert(offer, Action::Execute);
            put_long(nbt, "energy", to.stored);
            if let Some(key) = receiver.counted {
                let counted = get_long(nbt, key) + (offer - refused);
                put_long(nbt, key, counted);
            }
            offer - refused
        });
        if let Some(taken) = taken {
            energy.extract(taken, Action::Execute);
        }
    }
    if energy.stored != before {
        put_long(&mut own.entity.nbt, "energy", energy.stored);
    }
}

// --- Helpers -------------------------------------------------------------------------------------------

fn put_double(nbt: &mut Compound, key: &str, v: f64) {
    nbt.insert(key.into(), Tag::Double(v.to_bits()));
}

/// The data a Mekanism screen reads past the furnace's four slots: energy and its capacity (four 15-bit slots
/// each), the upgrades (generators take none), the usage slot (the screens show what the block makes there).
fn screen(entity: &mut ModBlockEntityView<'_>, head: [i32; 4], stored: u64, capacity: u64, produced: u64) {
    super::common::MenuData::new(stored, capacity, produced).furnace(head).write(entity);
}

/// `Level.getSkyDarken`, as a brightness: the sun's share of the sky light factor with the weather
/// (`LightLevelUtils.getSunBrightness`), worked out from the clock like vanilla's own sky.
fn sun_brightness(host: &dyn ServerHost, level: mod_api::LevelRef) -> f32 {
    let time = level.day_time(host).unwrap_or(6000);
    let d = (time.rem_euclid(24_000) as f64 / 24_000.0 - 0.25).rem_euclid(1.0);
    let e = 0.5 - (d * std::f64::consts::PI).cos() / 2.0;
    let of_day = (d * 2.0 + e) / 3.0;
    let mut f: f64 = 1.0 - ((of_day * std::f64::consts::TAU).cos() * 2.0 + 0.2);
    f = 1.0 - f.clamp(0.0, 1.0);
    let (rain, thunder) = level.weather(host).unwrap_or((0.0, 0.0));
    (f * (1.0 - f64::from(rain) * 5.0 / 16.0) * (1.0 - f64::from(thunder) * 5.0 / 16.0)) as f32
}

/// `LightLevelUtils.canSeeSun`: a level with sky light, `getSkyDarken` under 4 and a clear view of the sky.
fn can_see_sun(host: &dyn ServerHost, level: mod_api::LevelRef, pos: [i32; 3]) -> bool {
    level.dimension_shape(host).is_some_and(|s| s.sky_light)
        && level.get_sky_darken(host).is_some_and(|d| d < 4)
        && level.can_see_sky(host, pos).unwrap_or(false)
}

/// `Biome.getTemperature(pos, seaLevel)` of the biome at `pos` (without vanilla's patch of noise): the base
/// temperature, less 0.05 per 40 blocks over the sea level plus 17. 0.8 where the biome is not known.
fn biome_temperature(host: &dyn ServerHost, level: mod_api::LevelRef, pos: [i32; 3]) -> f32 {
    let Some(props) = level.biome_at(host, pos).and_then(|b| level.biome_properties(host, &b)) else {
        return 0.8;
    };
    let sea = level.sea_level(host).unwrap_or(63);
    let mut t = props.temperature;
    if pos[1] > sea + 17 {
        t -= (pos[1] - (sea + 17)) as f32 * 0.05 / 40.0;
    }
    t
}

fn level_of(host: &dyn ServerHost) -> Option<mod_api::LevelRef> {
    host.level(host.dimension())
}

/// `ITileHeatHandler.simulateAdjacent`: gives `heat` to each neighbour of `pos` that is a heat sink (a family
/// registered it with `transmit::register_heat_sink`). Returns the temperature moved.
fn adjacent_heat(host: &mut dyn ServerHost, own: &mut Own<'_, '_>, level: mod_api::LevelRef, pos: [i32; 3], heat: &mut mekanism_core::heat::HeatCapacitor) -> f64 {
    let mut total = 0.0;
    for (_, d) in DIRECTIONS {
        let at = transmit::offset(pos, d);
        let back = transmit::neg(d);
        let moved = transmit::update(host, own, level, at, |id, nbt| {
            transmit::takes_in(nbt, back, Transmission::Heat).then(|| transmit::heat_into(id, nbt, back, heat)).flatten()
        })
        .flatten();
        if let Some(moved) = moved
            && moved != 0.0
        {
            total += moved;
        }
    }
    total
}

// --- Heat generator ------------------------------------------------------------------------------------

fn tick_heat(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let level = level_of(&*host);
    let mut gen_ = HeatGenerator::new();
    if let Some(level) = level {
        gen_.set_ambient(mekanism_core::heat::ambient_temp(f64::from(biome_temperature(&*host, level, pos))));
    }
    gen_.energy.stored = get_long(&entity.nbt, "energy").min(gen_.energy.capacity);
    gen_.lava = get_int(&entity.nbt, "lava");
    if let Some(heat) = get_double(&entity.nbt, "heat") {
        gen_.heat.heat = heat;
    }
    // FluidFuelInventorySlot.fillOrBurn: one furnace fuel item at a time becomes lava, when it fits.
    if let Some(Some(fuel)) = entity.items.get(1).cloned() {
        let burn = super::item_key(&*host, &super::item_id(&*host, &fuel))
            .and_then(|k| host.item_burn_time(k))
            .unwrap_or(0)
            .max(0) as u32;
        if gen_.accepts_fuel(burn) {
            gen_.add_fuel(burn);
            let left = fuel.count() - 1;
            entity.items[1] = if left > 0 {
                let mut rest = fuel;
                rest.set_count(left).ok().map(|()| rest)
            } else if super::item_id(&*host, &fuel) == "minecraft:lava_bucket" {
                super::item_key(&*host, "minecraft:bucket").and_then(|k| ItemStack::new(k, 1))
            } else {
                None
            };
        }
    }
    // Lava the pipes left in the tank beside it (`lava_in`) joins the generator's own.
    let mut piped = get_tank(&entity.nbt, "lava_in", u64::from(g::TANK_CAPACITY));
    let room = u64::from(g::TANK_CAPACITY.saturating_sub(gen_.lava));
    let moved = piped.amount.min(room);
    if moved > 0 {
        piped.extract(moved);
        gen_.lava += moved as u32;
        put_tank(&mut entity.nbt, "lava_in", &piped);
    }
    // `ITileHeatHandler.simulateAdjacent`: heat flows into every colder neighbour that holds heat.
    let mut own = Own { pos, entity: &mut *entity };
    gen_.tick_with(&mut |heat| match level {
        Some(level) => adjacent_heat(host, &mut own, level, pos, heat),
        None => 0.0,
    });
    charge_slot(&*host, entity, 0, &mut gen_.energy);
    put_long(&mut entity.nbt, "energy", gen_.energy.stored);
    put_int(&mut entity.nbt, "lava", gen_.lava);
    put_double(&mut entity.nbt, "heat", gen_.heat.heat);
    put_long(&mut entity.nbt, "prod", gen_.producing);
    put_double(&mut entity.nbt, "temp", gen_.heat.temperature());
    put_double(&mut entity.nbt, "env", gen_.last_env);
    put_double(&mut entity.nbt, "transfer", gen_.last_transfer);
    // The flame shows the lava left, the arrow the temperature above ambient (up to 200 K); the screen reads the
    // temperature in tenths of a kelvin in slot 2, what the air takes in slot 3 and what it passes on in slot 1.
    let tenths = |v: f64| (v * 10.0).clamp(0.0, 32_767.0) as i32;
    screen(
        entity,
        [gen_.lava as i32, tenths(gen_.last_transfer), tenths(gen_.heat.temperature()), tenths(gen_.last_env)],
        gen_.energy.stored,
        gen_.energy.capacity,
        gen_.producing,
    );
    gen_.active
}

// --- Solar and wind ------------------------------------------------------------------------------------

/// `TileEntitySolarGenerator`: the peak, scaled by the sun's brightness and the biome's multiplier.
fn tick_solar(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>, peak: u64, storage: u64) -> bool {
    let host: &dyn ServerHost = &*host;
    let mut energy = EnergyContainer::new(storage, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(storage);
    let (sees, produced) = match level_of(host) {
        Some(level) if can_see_sun(host, level, pos) => {
            let props = level.biome_at(host, pos).and_then(|b| level.biome_properties(host, &b));
            let (down, rains) = props.map_or((0.4, true), |p| (p.downfall, p.has_precipitation));
            let multiplier = g::solar_biome_multiplier(biome_temperature(host, level, pos), down, rains);
            (true, g::solar_production(peak, true, sun_brightness(host, level), multiplier))
        }
        _ => (false, 0),
    };
    energy.insert(produced, Action::Execute);
    charge_slot(host, entity, 0, &mut energy);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_long(&mut entity.nbt, "prod", produced);
    put_int(&mut entity.nbt, "sees", u32::from(sees));
    screen(entity, [(energy.stored * 1000 / storage.max(1)) as i32, i32::from(sees), produced as i32, peak as i32], energy.stored, storage, produced);
    produced > 0
}

/// `TileEntityWindGenerator`: its multiplier is worked out once a second, from the height of its top.
fn tick_wind(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let host: &dyn ServerHost = &*host;
    let mut energy = EnergyContainer::new(g::WIND_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(g::WIND_STORAGE);
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    let level = level_of(host);
    let shape = level.and_then(|l| l.dimension_shape(host));
    let (min_y, top_y) = shape.map_or((-64, 319), |s| (s.min_y, s.min_y + s.height - 1));
    let mut multiplier = get_double(&entity.nbt, "multiplier").unwrap_or(0.0);
    if ticker == 0 {
        let top = [pos[0], pos[1] + 4, pos[2]];
        let clear = level.is_some_and(|l| l.can_see_sky(host, top).unwrap_or(false));
        multiplier = if clear { g::wind_multiplier(top[1], min_y, top_y) } else { 0.0 };
        put_double(&mut entity.nbt, "multiplier", multiplier);
    }
    let produced = if multiplier > 0.0 { g::wind_generation(multiplier) } else { 0 };
    energy.insert(produced, Action::Execute);
    charge_slot(host, entity, 0, &mut energy);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_long(&mut entity.nbt, "prod", produced);
    // `GuiWindGenerator`: why it makes nothing. Mekanism ships no dimension in `c:no_wind`, so it is always the sky.
    let wstate = if produced > 0 { 0 } else { 2 };
    put_int(&mut entity.nbt, "wstate", wstate);
    screen(
        entity,
        [(energy.stored * 1000 / g::WIND_STORAGE) as i32, wstate as i32, produced as i32, g::WIND_GENERATION_MAX as i32],
        energy.stored,
        g::WIND_STORAGE,
        produced,
    );
    // What the rotor needs in the world (`getHeightSpeedRatio`, in thousandths of a degree a tick).
    let height = (pos[1] + 4 - min_y) as f32;
    let speed = 32.0 * height / (top_y + 1 - min_y).max(1) as f32;
    let mut client = Compound::new();
    put_int(&mut client, "rotor_active", u32::from(produced > 0));
    put_int(&mut client, "rotor_speed", (speed * 1000.0).max(0.0) as u32);
    put_client(entity, &["rotor_active", "rotor_speed"], client);
    produced > 0
}

// --- Gas-burning and bio generators --------------------------------------------------------------------

/// `TileEntityGasGenerator`: burns the fuel gas in its tank.
fn tick_gas(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>) -> bool {
    let host: &dyn ServerHost = &*host;
    let mut energy = EnergyContainer::new(chem::GAS_GENERATOR_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(chem::GAS_GENERATOR_STORAGE);
    let mut fuel = get_tank(&entity.nbt, "fuel", chem::GAS_GENERATOR_TANK);
    let (made, used) = chem::burn_fuel(&mut fuel, energy.capacity - energy.stored);
    energy.insert(made, Action::Execute);
    charge_slot(host, entity, 0, &mut energy);
    // `ChemicalInventorySlot.fill`: a tank item in the fuel slot gives the generator its gas.
    fill_tank_from_item(host, entity, 1, &mut fuel, |c| chem::fuel(c).is_some());
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fuel", &fuel);
    put_int(&mut entity.nbt, "used", used as u32);
    // `fuelDensity * getUsed` for the energy tab.
    let density = fuel.chemical.as_deref().and_then(chem::fuel).map_or(0, |(d, _)| d);
    let prod = density * used;
    put_long(&mut entity.nbt, "prod", prod);
    let used_shown = used.min(32_767) as i32;
    screen(entity, [fuel.amount.min(32_767) as i32, fuel.capacity as i32, 0, used_shown], energy.stored, energy.capacity, prod);
    made > 0
}

/// Bio fuel items and what each gives (`c:fuels/bio`, `c:fuels/block/bio`), in mB of bioethanol.
fn bio_fuel(host: &dyn Host, stack: &ItemStack) -> u64 {
    let id = super::item_id(host, stack);
    if super::in_tag(host, &id, "c:fuels/bio") {
        g::BIO_FUEL_PER_ITEM
    } else if super::in_tag(host, &id, "c:fuels/block/bio") {
        9 * g::BIO_FUEL_PER_ITEM
    } else {
        0
    }
}

/// `TileEntityBioGenerator`: a bio fuel in the second slot becomes bioethanol when it all fits, and 1 mB a tick
/// makes 25 FE while there is room for it.
fn tick_bio(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>) -> bool {
    let host: &dyn ServerHost = &*host;
    let mut energy = EnergyContainer::new(g::BIO_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(g::BIO_STORAGE);
    let mut fuel = get_tank(&entity.nbt, "fuel", g::BIO_TANK);
    if let Some(Some(item)) = entity.items.get(1).cloned() {
        let amount = bio_fuel(host, &item);
        if amount > 0 && fuel.accepts(g::BIOETHANOL) && fuel.capacity - fuel.amount >= amount {
            fuel.insert(g::BIOETHANOL, amount);
            entity.items[1] = if item.count() > 1 {
                let mut rest = item.clone();
                rest.set_count(item.count() - 1).ok().map(|()| rest)
            } else {
                None
            };
        }
    }
    let (used, made) = g::bio_tick(fuel.amount, energy.capacity - energy.stored);
    fuel.extract(used);
    energy.insert(made, Action::Execute);
    charge_slot(host, entity, 0, &mut energy);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fuel", &fuel);
    put_long(&mut entity.nbt, "prod", made);
    screen(entity, [fuel.amount as i32, fuel.capacity as i32, 0, 0], energy.stored, energy.capacity, made);
    made > 0
}

// --- Energy cube ---------------------------------------------------------------------------------------

/// `TileEntityEnergyCube`: the top slot charges the item in it from the cube; the bottom one empties its item
/// into it. The core the world draws shows the tier and how full the cube is.
fn tick_cube(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>, tier: BaseTier) {
    let host: &dyn ServerHost = &*host;
    let (capacity, _rate) = tier::energy_cube(tier);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    charge_slot(host, entity, 0, &mut energy);
    discharge_slot(host, entity, 1, &mut energy);
    put_long(&mut entity.nbt, "energy", energy.stored);
    screen(entity, [(energy.stored * 1000 / capacity.max(1)) as i32, 1000, 0, 0], energy.stored, capacity, 0);
    let mut core = Compound::new();
    put_int(&mut core, "core_alpha", (energy.stored as f64 / capacity.max(1) as f64 * 255.0) as u32);
    put_int(&mut core, "core_tier", tier as u32);
    put_client(entity, &["core_alpha", "core_tier"], core);
}
