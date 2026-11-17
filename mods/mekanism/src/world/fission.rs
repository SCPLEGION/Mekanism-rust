//! The Fission Reactor (`FissionReactorMultiblockData`): the structure of casing, ports, fuel assemblies and control
//! rods, the master port that runs it, its tanks, heat and damage, the meltdown, and what clients draw of it.
//! Port of `find_reactor` and `tick_fission_port` of `runtime.rs`; the numbers live in `mekanism_core::fission`.
use super::common::put_client;
use super::multiblock::{
    add, block_name, find_box, get_pos, get_tank, on_box_edge, put_double, put_fills, put_ints, put_tank, remove,
    ticking_level, Fill, DIRECTIONS,
};
use super::nbt::{get_double, get_ints, get_long, put_int, put_long};
use mekanism_core::fission::{self, Reactor, State};
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockPart, Compound, ExplosionInteraction,
    ExplosionSettings, Host, LevelRef, Registrar, ServerHost, Tag,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// The block entity type of the reactor's ports (`mekanismgenerators:fission_reactor_port`).
pub(crate) const TYPE: &str = "fission_port";
pub(crate) const PORT: &str = "mekanismgenerators:fission_reactor_port";
const FISSILE_FUEL: &str = "mekanism:fissile_fuel";
const NUCLEAR_WASTE: &str = "mekanism:nuclear_waste";

pub(crate) fn register(r: &mut dyn Registrar) {
    register_ports();
    let kind = r.block_entity_type(TYPE).blocks(&[PORT]).build();
    r.block_entity_part(&kind, FissionPart).expect("native block entity type");
}

struct FissionPart;

#[mod_api::horizon_part]
impl BlockPart for FissionPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for FissionPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        tick_port(host, event.pos, &mut event.entity);
    }

    fn block_entity_update_tag(&self, event: &mut mod_api::BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }
}

/// `FissionReactorValidator`: the frame blocks joined to `start` must be exactly the shell of a box 3 to 18 on each
/// side, with fuel assemblies, control rods or air inside. The reactor, its ports, the box's low corner and size,
/// and the fuel columns `[x, y, z, height]`.
#[allow(clippy::type_complexity)]
fn find_reactor(host: &dyn Host, level: LevelRef, start: [i32; 3]) -> Option<(Reactor, Vec<[i32; 3]>, [i32; 3], [i32; 3], Vec<[i32; 4]>)> {
    let (lo, hi, seen) = find_box(host, level, start, fission::is_frame, [fission::MIN_SIZE; 3], [fission::MAX_SIZE; 3])?;
    let size = |k: usize| (hi[k] - lo[k] + 1) as u64;
    let mut reactor = Reactor { volume: size(0) * size(1) * size(2), ..Default::default() };
    let mut ports = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let at = [x, y, z];
                let shell = x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2];
                let name = block_name(host, level, at);
                if shell {
                    // The edges are casing or reactor glass; ports and logic adapters sit in the faces.
                    let casing = matches!(name.as_str(), "mekanismgenerators:fission_reactor_casing" | "mekanismgenerators:reactor_glass");
                    if !seen.contains(&at) || (on_box_edge(lo, hi, at) && !casing) {
                        return None;
                    }
                    reactor.locations += 1;
                    if name == PORT {
                        ports.push(at);
                    }
                } else if !reactor.add_inside(&name) {
                    return None;
                }
            }
        }
    }
    // Each column holds fuel assemblies stacked from its lowest block and one control rod directly above them
    // (`FuelAssembly.validate`); a column with the one and not the other, or a gap, does not form.
    let mut columns = Vec::new();
    for x in lo[0] + 1..hi[0] {
        for z in lo[2] + 1..hi[2] {
            let column = |wanted: &str| -> Vec<i32> {
                (lo[1] + 1..hi[1]).filter(|&y| block_name(host, level, [x, y, z]) == wanted).collect()
            };
            let fuel = column("mekanismgenerators:fission_fuel_assembly");
            let rods = column("mekanismgenerators:control_rod_assembly");
            if fuel.is_empty() && rods.is_empty() {
                continue;
            }
            if rods.len() != 1 || fuel.is_empty() || fuel.windows(2).any(|w| w[1] != w[0] + 1) || rods[0] != fuel[fuel.len() - 1] + 1 {
                return None;
            }
            columns.push([x, fuel[0], z, fuel.len() as i32]);
        }
    }
    // `FissionReactorValidator.postcheck`: 6 faces per fuel assembly, 2 fewer for each neighbour it touches.
    let fuel = |at: [i32; 3]| block_name(host, level, at) == "mekanismgenerators:fission_fuel_assembly";
    for x in lo[0] + 1..hi[0] {
        for y in lo[1] + 1..hi[1] {
            for z in lo[2] + 1..hi[2] {
                if fuel([x, y, z]) {
                    reactor.surface += 6;
                    // Each touching pair counts once: look only along the positive directions.
                    for (_, d) in DIRECTIONS {
                        if d[0] + d[1] + d[2] <= 0 {
                            continue;
                        }
                        if fuel(add([x, y, z], d)) {
                            reactor.surface -= 2;
                        }
                    }
                }
            }
        }
    }
    (reactor.assemblies > 0).then_some((reactor, ports, lo, [size(0) as i32, size(1) as i32, size(2) as i32], columns))
}

/// The ambient temperature of a reactor not yet formed (the plains default); a formed one stores the
/// average of its biome (`calculateAverageAmbientTemperature`).
const AMBIENT: f64 = 300.0;

/// `HeatAPI.getAmbientTemp` averaged over the eight corners of the box (`biome.temperature_at(pos, 63)` of the fork:
/// the base temperature, as the height adjustment starts above y 80).
fn ambient_over(host: &dyn Host, level: LevelRef, min: [i32; 3], max: [i32; 3]) -> f64 {
    let mut sum = 0.0;
    for i in 0..8 {
        let at = [
            if i & 1 == 0 { min[0] } else { max[0] },
            if i & 2 == 0 { min[1] } else { max[1] },
            if i & 4 == 0 { min[2] } else { max[2] },
        ];
        let temperature = level
            .biome_at(host, at)
            .and_then(|biome| level.biome_properties(host, &biome))
            .map_or(0.8, |p| f64::from(p.temperature));
        sum += temperature;
    }
    mekanism_core::heat::ambient_temp(sum / 8.0)
}

/// What a master port saved of its reactor's shape.
fn reactor_of(nbt: &Compound) -> Reactor {
    Reactor {
        assemblies: get_long(nbt, "assemblies"),
        volume: get_long(nbt, "volume"),
        surface: get_long(nbt, "surface"),
        locations: get_long(nbt, "locations"),
    }
}

/// `FissionReactorMultiblockData`'s heat, damage and burn fields as the master saved them.
fn state_of(nbt: &Compound, r: &Reactor) -> State {
    let capacity = r.locations as f64 * fission::CASING_HEAT_CAPACITY;
    let ambient = get_double(nbt, "ambient").unwrap_or(AMBIENT);
    let mut st = State::new(capacity, ambient);
    st.heat = get_double(nbt, "heat").unwrap_or(capacity * ambient);
    st.damage = get_double(nbt, "damage").unwrap_or(0.0);
    st.burn_remaining = get_double(nbt, "burn_rem").unwrap_or(0.0);
    st.partial_waste = get_double(nbt, "pwaste").unwrap_or(0.0);
    st.rate_limit = get_double(nbt, "limit").unwrap_or(fission::DEFAULT_RATE_LIMIT);
    st.active = get_long(nbt, "active") == 1;
    st.force_disable = get_long(nbt, "force") == 1;
    st.last_burn = get_double(nbt, "last_burn").unwrap_or(0.0);
    st.last_boil = get_long(nbt, "last_boil");
    st.env_loss = get_double(nbt, "env_loss").unwrap_or(0.0);
    st
}

fn save_state(nbt: &mut Compound, st: &State) {
    put_double(nbt, "heat", st.heat);
    put_double(nbt, "damage", st.damage);
    put_double(nbt, "burn_rem", st.burn_remaining);
    put_double(nbt, "pwaste", st.partial_waste);
    put_double(nbt, "limit", st.rate_limit);
    put_double(nbt, "last_burn", st.last_burn);
    put_double(nbt, "env_loss", st.env_loss);
    put_double(nbt, "ambient", st.ambient);
    put_long(nbt, "active", u64::from(st.active));
    put_long(nbt, "force", u64::from(st.force_disable));
    put_long(nbt, "last_boil", st.last_boil);
}

/// What a fission reactor keeps in its master port's data (`FissionReactorMultiblockData`): the state and the tanks.
const KEPT: [&str; 14] = ["heat", "damage", "burn_rem", "pwaste", "limit", "last_burn", "env_loss", "active", "force", "last_boil", "fuel", "water", "steam", "waste"];

/// `MultiblockCache`: a reactor's state kept by dimension and box, so that a rebuilt reactor, or one whose master
/// port was blown up, picks up where it was. In memory only.
static CACHE: Mutex<Option<HashMap<(String, [i32; 6]), Compound>>> = Mutex::new(None);

fn cache_key(dimension: &str, nbt: &Compound) -> Option<(String, [i32; 6])> {
    let b = get_ints(nbt, "box");
    (b.len() == 6).then(|| (dimension.to_owned(), [b[0], b[1], b[2], b[3], b[4], b[5]]))
}

fn cache_store(dimension: &str, nbt: &Compound) {
    if let Some(key) = cache_key(dimension, nbt) {
        let mut kept = Compound::new();
        for k in KEPT {
            if let Some(v) = crate::gear::data::get(nbt, k) {
                kept.insert(k.into(), v.clone());
            }
        }
        CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, kept);
    }
}

fn cache_take(dimension: &str, nbt: &Compound) -> Option<Compound> {
    let key = cache_key(dimension, nbt)?;
    CACHE.lock().unwrap().as_mut()?.remove(&key)
}

/// `FissionPortMode`: the blockstate value and its name, in the order a sneaking click steps through.
pub(crate) const PORT_MODES: [(&str, &str); 3] = [("input", "input only"), ("output_waste", "output waste"), ("output_coolant", "output coolant")];

/// The mode of the port at `pos` (index into [`PORT_MODES`]), or none where the block has no such property.
fn port_mode(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<usize> {
    let mode = level.block_property(host, pos, "mode")?;
    PORT_MODES.iter().position(|(v, _)| *v == mode)
}

/// A meltdown in progress (`Meltdown`): for five seconds, random explosions inside the old reactor's box.
struct Meltdown {
    dimension: String,
    lo: [i32; 3],
    hi: [i32; 3],
    magnitude: f64,
    age: u32,
    /// The reactor's master port, and the walls it had when the meltdown began: `BlockMekanism.onBlockExploded`
    /// calls `meltdownHappened` when the first of them goes.
    master: [i32; 3],
    shell: Vec<([i32; 3], String)>,
    happened: bool,
}

static MELTDOWNS: Mutex<Vec<Meltdown>> = Mutex::new(Vec::new());
/// The game time the meltdowns were last advanced at: any port may be the first to tick in a tick.
static LAST_MELTDOWN_TICK: Mutex<i64> = Mutex::new(i64::MIN);

/// `Meltdown.update` for every meltdown: a tenth of the ticks, if the heat times the chance allows, an
/// explosion of the configured radius with fire at a random spot of the box.
fn tick_meltdowns(host: &mut dyn ServerHost, level: LevelRef) {
    let now = level.game_time(&*host).unwrap_or(0);
    {
        let mut last = LAST_MELTDOWN_TICK.lock().unwrap();
        if *last == now {
            return;
        }
        *last = now;
    }
    let dimension = level.dimension_id();
    let mut list = std::mem::take(&mut *MELTDOWNS.lock().unwrap());
    list.retain_mut(|m| {
        if m.dimension != dimension {
            return true;
        }
        m.age += 1;
        let (go, at) = level
            .with_random(host, |r| {
                use mod_api::RandomSource;
                if r.next_int() % 10 == 0 && r.next_double() < m.magnitude * fission::EXPLOSION_CHANCE {
                    let mut at = [0.0; 3];
                    for k in 0..3 {
                        at[k] = f64::from(r.next_int_between_inclusive(m.lo[k], m.hi[k]).unwrap_or(m.lo[k]));
                    }
                    (true, at)
                } else {
                    (false, [0.0; 3])
                }
            })
            .unwrap_or((false, [0.0; 3]));
        if go {
            let _ = level.explode(host, None, at, fission::MELTDOWN_RADIUS, true, ExplosionInteraction::Block, ExplosionSettings::default());
            let broken = m.shell.iter().any(|(at, name)| block_name(&*host, level, *at) != *name);
            if !m.happened && broken {
                m.happened = true;
                meltdown_happened(host, level, m);
            }
        }
        m.age < 5 * 20
    });
    let mut v = MELTDOWNS.lock().unwrap();
    list.append(&mut v);
    *v = list;
}

/// `meltdownHappened`: the heated coolant is lost and the reactor starts over, damaged. (The radioactive chemicals
/// of its tanks are meant to leak into the air as radiation: the radiation system is not on the new API yet.)
fn meltdown_happened(host: &mut dyn ServerHost, level: LevelRef, m: &Meltdown) {
    let Some(entity) = host.block_entity(level, m.master) else {
        return;
    };
    let Some(mut data) = entity.data(&*host) else {
        return;
    };
    let r = reactor_of(&data);
    let mut st = state_of(&data, &r);
    st.meltdown_reset();
    save_state(&mut data, &st);
    for (key, capacity) in [("fuel", r.fuel_capacity()), ("waste", r.fuel_capacity()), ("water", r.water_capacity())] {
        let _ = (key, capacity);
    }
    let mut steam = get_tank(&data, "steam", r.steam_capacity());
    steam.extract(steam.amount);
    put_tank(&mut data, "steam", &steam);
    // `cache.sync`: the reset reactor outlives its master port.
    cache_store(&m.dimension, &data);
    let _ = entity.replace_data(host, data);
}

/// The tubes and pipes push into a port (`ChemicalFamily::receiver`): what a port takes in waits in its own `in_fuel`
/// and `in_water` tanks, which the master empties into the reactor's tanks every tick (`pull_from_carriers`).
fn port_receiver(id: &str, nbt: &Compound, chemical: &str, _from_tube: bool, _d: [i32; 3]) -> Option<(&'static str, u64)> {
    if id != PORT {
        return None;
    }
    let r = reactor_of(nbt);
    if r.assemblies == 0 {
        return None;
    }
    match chemical {
        FISSILE_FUEL => Some(("in_fuel", r.fuel_capacity())),
        "minecraft:water" | fission::SODIUM_COOLANT => Some(("in_water", r.water_capacity())),
        _ => None,
    }
}

fn register_ports() {
    super::transmit::register_chemical_family(super::transmit::ChemicalFamily {
        serves: |id| if id == PORT { 3 } else { 0 },
        receiver: port_receiver,
        source: |_, _| None,
    });
}

/// `pull_from_carriers` for the port at `p`: what its input tanks hold goes into the reactor's tank `into`.
fn pull_port(host: &mut dyn ServerHost, level: LevelRef, own: Option<&mut Compound>, p: [i32; 3], from_key: &str, into: &mut mekanism_core::chemical::ChemicalTank) {
    let mut take = |data: &mut Compound| -> bool {
        let mut theirs = get_tank(data, from_key, u64::MAX);
        let Some(chemical) = theirs.chemical.clone() else {
            return false;
        };
        let n = into.insert(&chemical, theirs.amount);
        if n == 0 {
            return false;
        }
        theirs.extract(n);
        put_tank(data, from_key, &theirs);
        true
    };
    match own {
        Some(data) => {
            take(data);
        }
        None => {
            let Some(entity) = host.block_entity(level, p) else {
                return;
            };
            let Some(mut data) = entity.data(&*host) else {
                return;
            };
            if take(&mut data) {
                let _ = entity.replace_data(host, data);
            }
        }
    }
}

fn tick_port(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut mod_api::ModBlockEntityView<'_>) {
    let Some(level) = ticking_level(&*host) else {
        return;
    };
    tick_meltdowns(host, level);
    let dimension = level.dimension_id();
    let ticker = super::nbt::get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        match find_reactor(&*host, level, pos) {
            Some((r, ports, lo, size, columns)) => {
                put_ints(&mut entity.nbt, "box", vec![lo[0], lo[1], lo[2], size[0], size[1], size[2]]);
                put_ints(&mut entity.nbt, "columns", columns.concat());
                let hi = [lo[0] + size[0] - 1, lo[1] + size[1] - 1, lo[2] + size[2] - 1];
                put_double(&mut entity.nbt, "ambient", ambient_over(&*host, level, lo, hi));
                put_long(&mut entity.nbt, "assemblies", r.assemblies);
                put_long(&mut entity.nbt, "volume", r.volume);
                put_long(&mut entity.nbt, "surface", r.surface);
                put_long(&mut entity.nbt, "locations", r.locations);
                let master = ports.first().copied().unwrap_or(pos);
                put_ints(&mut entity.nbt, "master", master.to_vec());
                put_ints(&mut entity.nbt, "ports", ports.concat());
                if master == pos {
                    // A reactor formed over the box of one that was lost takes its cached state; else the state is kept.
                    if crate::gear::data::get(&entity.nbt, "heat").is_none()
                        && let Some(kept) = cache_take(dimension, &entity.nbt)
                    {
                        for (k, v) in kept {
                            entity.nbt.insert(k, v);
                        }
                    }
                    cache_store(dimension, &entity.nbt);
                }
            }
            None => {
                put_long(&mut entity.nbt, "assemblies", 0);
                remove(&mut entity.nbt, "master");
                remove(&mut entity.nbt, "box");
            }
        }
    }
    let r = reactor_of(&entity.nbt);
    if r.assemblies == 0 || get_pos(&entity.nbt, "master") != Some(pos) {
        put_fills(entity, &[]);
        return;
    }
    let ports: Vec<[i32; 3]> = {
        let v = get_ints(&entity.nbt, "ports");
        if v.is_empty() { vec![pos] } else { v.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect() }
    };
    let mut fuel = get_tank(&entity.nbt, "fuel", r.fuel_capacity());
    let mut water = get_tank(&entity.nbt, "water", r.water_capacity());
    let mut steam = get_tank(&entity.nbt, "steam", r.steam_capacity());
    let mut waste = get_tank(&entity.nbt, "waste", r.fuel_capacity());
    // A port takes things in only in input mode, gives waste in waste mode and the heated coolant in coolant mode
    // (a block without the mode property does all three).
    let modes: Vec<Option<usize>> = ports.iter().map(|&p| port_mode(&*host, level, p)).collect();
    for (&p, mode) in ports.iter().zip(&modes) {
        if mode.is_none_or(|m| m == 0) {
            let own = (p == pos).then_some(&mut entity.nbt);
            pull_port(host, level, own, p, "in_fuel", &mut fuel);
            let own = (p == pos).then_some(&mut entity.nbt);
            pull_port(host, level, own, p, "in_water", &mut water);
        }
    }
    let mut st = state_of(&entity.nbt, &r);
    // `burnFuel`: fissile fuel into heat and nuclear waste.
    {
        let mut amount = if fuel.accepts(FISSILE_FUEL) { fuel.amount } else { 0 };
        let before = amount;
        let mut waste_amount = if waste.accepts(NUCLEAR_WASTE) { waste.amount } else { waste.capacity };
        let waste_before = waste_amount;
        st.burn(&mut amount, fuel.capacity, r.assemblies, &mut waste_amount, waste.capacity);
        fuel.extract(before.saturating_sub(amount));
        waste.insert(NUCLEAR_WASTE, waste_amount.saturating_sub(waste_before));
    }
    // `handleCoolant`: heat above boiling turns the coolant (water, or cooled sodium in the same merged tank) into its
    // heated variant (steam, superheated sodium).
    {
        let (conductivity, enthalpy, heated) = match water.chemical.as_deref() {
            Some(fission::SODIUM_COOLANT) => (fission::SODIUM_CONDUCTIVITY, fission::SODIUM_ENTHALPY, fission::SODIUM_HEATED),
            _ => (fission::WATER_CONDUCTIVITY, fission::COOLANT_ENTHALPY, "mekanism:steam"),
        };
        let mut w = water.amount;
        let w_before = w;
        let mut s = if steam.accepts(heated) { steam.amount } else { steam.capacity };
        let s_before = s;
        st.coolant_with(&mut w, &mut s, steam.capacity, fission::boil_efficiency(r.surface, r.assemblies), conductivity, enthalpy);
        water.extract(w_before - w);
        steam.insert(heated, s.saturating_sub(s_before));
    }
    for (&p, mode) in ports.iter().zip(&modes) {
        if mode.is_none_or(|m| m == 2) {
            super::transmit::push_chemical(host, p, &mut steam, None, u64::MAX, false, &|_, _| true);
        }
        if mode.is_none_or(|m| m == 1) {
            super::transmit::push_chemical(host, p, &mut waste, None, u64::MAX, false, &|_, _| true);
        }
    }
    // `simulateEnvironment`, then `handleDamage`.
    st.environment();
    let roll = level.with_random(host, |r| mod_api::RandomSource::next_double(r)).unwrap_or(1.0);
    let b = get_ints(&entity.nbt, "box");
    if st.damage_step(roll) == fission::Damage::Meltdown && b.len() == 6 {
        let lo = [b[0], b[1], b[2]];
        let hi = [b[0] + b[3] - 1, b[1] + b[4] - 1, b[2] + b[5] - 1];
        // One meltdown at a time for a reactor (`MeltdownData` keys them by the multiblock's id).
        if !MELTDOWNS.lock().unwrap().iter().any(|m| m.master == pos && m.dimension == dimension) {
            let mut shell = Vec::new();
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        if x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2] {
                            shell.push(([x, y, z], block_name(&*host, level, [x, y, z])));
                        }
                    }
                }
            }
            MELTDOWNS.lock().unwrap().push(Meltdown {
                dimension: dimension.to_owned(),
                lo,
                hi,
                magnitude: st.heat,
                age: 0,
                master: pos,
                shell,
                happened: false,
            });
        }
    }
    save_state(&mut entity.nbt, &st);
    super::multiblock::put_tank(&mut entity.nbt, "fuel", &fuel);
    put_tank(&mut entity.nbt, "water", &water);
    put_tank(&mut entity.nbt, "steam", &steam);
    put_tank(&mut entity.nbt, "waste", &waste);
    // `RenderFissionReactor`: the coolant (water) and the heated coolant (steam) fill the inside, and the
    // assemblies glow while it burns.
    let mut fills = Vec::new();
    if b.len() == 6 {
        let (at, size) = ([b[0] + 1, b[1] + 1, b[2] + 1], [b[3] - 2, b[4] - 2, b[5] - 2]);
        if water.amount > 0 {
            fills.push(Fill { at, size, scale: water.amount as f64 / water.capacity.max(1) as f64, chemical: water.chemical.clone(), shrunk: false });
        }
        if steam.amount > 0 {
            fills.push(Fill { at, size, scale: steam.amount as f64 / steam.capacity.max(1) as f64, chemical: steam.chemical.clone(), shrunk: true });
        }
    }
    put_fills(entity, &fills);
    let columns = if st.active { get_ints(&entity.nbt, "columns") } else { Vec::new() };
    let mut glow = Compound::new();
    glow.insert("glow".into(), Tag::IntArray(columns));
    put_client(entity, &["glow"], glow);
}
