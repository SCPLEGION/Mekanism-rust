// Mekanism's glue inside Horizon's game crate (`crate::game::mekanism`). Game logic lives in the
// `mekanism_core` crate; this module only translates between the game's types and its own: block entity
// NBT, item slots, tags, recipes and neighbours.

use super::native_lifecycle::WorldPhase;
use super::Game;
use horizon_client::containers::{ModBlockEntity, ModContainer};
use horizon_client::items::ItemStack;
use horizon_world::nbt::{Compound, JavaString, Tag};
use mekanism_core::chemical::{self as chem, ChemicalTank};
use mekanism_core::energy::{Action, EnergyContainer};
use mekanism_core::factory::{self, Factory};
use mekanism_core::generator::HeatGenerator;
use mekanism_core::machine::{self, ElectricMachine, Operation, Slot};
use mekanism_core::tier::{self, BaseTier};
use mekanism_core::upgrades::{Upgrade, Upgrades};
use std::sync::Arc;

/// The mod id players turn on and off in Options > Mods.
const MOD_ID: &str = "mekanism";
const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";
/// The slots in a row of the chest menu a factory shows.
const FACTORY_ROW: usize = 9;

/// What a Mekanism block entity is, from its block (and block entity) id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Machine(&'static str, u64),
    /// processes, recipe kind, energy per tick
    Factory(usize, &'static str, u64),
    HeatGenerator,
    /// peak FE a tick, storage
    Solar(u64, u64),
    Wind,
    EnergyCube(BaseTier),
    /// A universal cable and what it carries in a tick.
    Cable(u64),
    Oxidizer,
    Separator,
    GasGenerator,
    ChemicalTank(BaseTier),
}

fn kind(id: &str) -> Option<Kind> {
    if let Some((recipe, usage)) = machine::machine(id) {
        return Some(Kind::Machine(recipe, usage));
    }
    if let Some((processes, recipe, usage)) = factory::factory(id) {
        return Some(Kind::Factory(processes, recipe, usage));
    }
    if id == HEAT_GENERATOR {
        return Some(Kind::HeatGenerator);
    }
    use mekanism_core::generator as g;
    match id {
        "mekanismgenerators:solar_generator" => return Some(Kind::Solar(g::SOLAR_GENERATION, g::SOLAR_STORAGE)),
        "mekanismgenerators:advanced_solar_generator" => {
            return Some(Kind::Solar(g::ADVANCED_SOLAR_GENERATION, g::ADVANCED_SOLAR_STORAGE));
        }
        "mekanismgenerators:wind_generator" => return Some(Kind::Wind),
        "mekanismgenerators:gas_burning_generator" => return Some(Kind::GasGenerator),
        "mekanism:chemical_oxidizer" => return Some(Kind::Oxidizer),
        "mekanism:electrolytic_separator" => return Some(Kind::Separator),
        _ => {}
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_universal_cable")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::universal_cable(tier).map(Kind::Cable);
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_chemical_tank")) {
        return BaseTier::ALL.into_iter().find(|t| t.name() == tier).map(Kind::ChemicalTank);
    }
    let tier = id.strip_prefix("mekanism:")?.strip_suffix("_energy_cube")?;
    BaseTier::ALL
        .into_iter()
        .find(|t| t.name() == tier && *t != BaseTier::Creative)
        .map(Kind::EnergyCube)
}

pub(super) fn world_phase(phase: WorldPhase) {
    if phase == WorldPhase::WorldOpenReady {
        eprintln!("{}", mekanism_core::loaded_message("(server)"));
    }
}

pub(super) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if entity.is_none() && kind(block).is_some() {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

pub(super) fn block_entity_ticks(block: &str, ticks: &mut bool) {
    if kind(block).is_some() {
        *ticks = true;
    }
}

/// Three slots shown in the furnace menu: 0 the input, 1 the energy or fuel slot, 2 the output; hoppers
/// fill the input from above and the middle slot from the sides, and take from below, as a furnace's.
pub(super) fn block_entity_container(block: &str, container: &mut Option<ModContainer>) {
    if container.is_some() {
        return;
    }
    // A factory: its inputs on the top row of a two-row chest menu, each process's output below it.
    if let Some(Kind::Factory(n, ..)) = kind(block) {
        let inputs: Vec<usize> = (0..n).collect();
        let outputs: Vec<usize> = (FACTORY_ROW..FACTORY_ROW + n).collect();
        *container = Some(ModContainer {
            slots: 2 * FACTORY_ROW,
            faces: [outputs.clone(), inputs.clone(), inputs.clone(), inputs.clone(), inputs.clone(), inputs.clone()],
            inputs,
            outputs,
            menu: Some("minecraft:generic_9x2".into()),
        });
        return;
    }
    let (inputs, outputs) = match kind(block) {
        Some(Kind::Machine(..)) => (vec![0, 1], vec![2]),
        Some(Kind::HeatGenerator) => (vec![1], vec![]),
        // The oxidizer's item goes in the first slot; the separator takes water buckets there and gives
        // the empty buckets back in the last.
        Some(Kind::Oxidizer) => (vec![0], vec![]),
        Some(Kind::Separator) => (vec![0], vec![2]),
        _ => return,
    };
    *container = Some(ModContainer {
        slots: 3,
        faces: [vec![2], vec![0], vec![1], vec![1], vec![1], vec![1]],
        inputs,
        outputs,
        menu: Some("minecraft:furnace".into()),
    });
}

pub(super) fn block_entity_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !game.native_on(MOD_ID) {
        return;
    }
    let Some(k) = kind(&entity.id) else {
        return;
    };
    let active = match k {
        Kind::Machine(recipe, usage) => tick_machine(game, entity, recipe, usage),
        Kind::Factory(n, recipe, usage) => tick_factory(game, entity, n, recipe, usage),
        Kind::HeatGenerator => tick_generator(game, pos, entity),
        Kind::Solar(peak, storage) => tick_solar(game, pos, entity, peak, storage),
        Kind::Wind => tick_wind(game, pos, entity),
        Kind::EnergyCube(t) => {
            tick_cube(game, pos, entity, t);
            None
        }
        Kind::Cable(rate) => {
            tick_cable(game, pos, entity, rate);
            None
        }
        Kind::Oxidizer => tick_oxidizer(game, pos, entity),
        Kind::Separator => tick_separator(game, pos, entity),
        Kind::GasGenerator => tick_gas_generator(game, pos, entity),
        Kind::ChemicalTank(t) => {
            tick_chemical_tank(game, pos, entity, t);
            None
        }
    };
    if let Some(active) = active {
        let lit = if active { "true" } else { "false" };
        // Blocks without the furnace's `lit` (the solar generator's iron block) show no state.
        if game.native_block_property(pos, "lit").is_some_and(|now| now != lit) {
            game.native_set_block_property(pos, "lit", lit);
        }
    }
}

// --- Machines ---------------------------------------------------------------------------------------

fn tick_machine(game: &mut Game<'_>, entity: &mut ModBlockEntity, recipe: &str, usage: u64) -> Option<bool> {
    let mut m = ElectricMachine::with_upgrades(usage, upgrades(&entity.nbt));
    m.energy.stored = get_long(&entity.nbt, "energy").min(m.energy.capacity);
    m.progress = get_int(&entity.nbt, "progress");
    let mut input = slot(entity.items.first().cloned().flatten());
    let mut middle = slot(entity.items.get(1).cloned().flatten());
    let mut output = slot(entity.items.get(2).cloned().flatten());
    // The sawmill's secondary output waits in the machine until the output slot takes it; the machine
    // stops meanwhile, as a full secondary slot stops it in Mekanism.
    let mut waiting = secondary(&entity.nbt);
    if let Some((item, count)) = waiting.clone() {
        let max = max_stack(game, &item);
        let fits = output.item.as_ref().is_none_or(|i| *i == item) && output.count + count <= max;
        if fits {
            output.item = Some(item);
            output.count += count;
            waiting = None;
        }
    }
    let (operation, max_stack, chance) = if waiting.is_some() {
        (None, 64, None)
    } else {
        find_operation(game, recipe, entity.items.first().cloned().flatten(), &middle)
    };
    let before = output.count;
    let extra = (recipe == "combining").then_some(&mut middle);
    m.tick(&mut input, &mut output, extra, operation.as_ref(), max_stack);
    if output.count != before
        && let Some((item, count, p)) = chance
        && roll(entity) < p
    {
        waiting = Some((item, count));
    }
    set_slot(entity, 0, &input);
    if recipe == "combining" {
        set_slot(entity, 1, &middle);
    }
    set_slot(entity, 2, &output);
    put_long(&mut entity.nbt, "energy", m.energy.stored);
    put_int(&mut entity.nbt, "progress", m.progress);
    set_secondary(&mut entity.nbt, waiting.as_ref());
    // The furnace menu's bars: the flame shows stored energy, the arrow the progress. Data slots are
    // 16 bits, so energy goes as thousandths of the capacity.
    entity.data = vec![
        (m.energy.stored * 1000 / m.energy.capacity.max(1)) as i32,
        1000,
        m.progress as i32,
        m.ticks_required as i32,
    ];
    Some(m.active)
}

fn tick_factory(game: &mut Game<'_>, entity: &mut ModBlockEntity, n: usize, recipe: &str, usage: u64) -> Option<bool> {
    let mut f = Factory::new(n, usage, upgrades(&entity.nbt));
    f.machine.energy.stored = get_long(&entity.nbt, "energy").min(f.machine.energy.capacity);
    if let Some(Tag::IntArray(saved)) = entity.nbt.get(&JavaString::from("progress")) {
        for (p, v) in f.progress.iter_mut().zip(saved) {
            *p = (*v).max(0) as u32;
        }
    }
    let mut inputs: Vec<Slot> = (0..n).map(|i| slot(entity.items.get(i).cloned().flatten())).collect();
    let mut outputs: Vec<Slot> = (0..n)
        .map(|i| slot(entity.items.get(FACTORY_ROW + i).cloned().flatten()))
        .collect();
    let operations: Vec<Option<(Operation, u32)>> = (0..n)
        .map(|i| {
            let (op, max, _) = find_operation(game, recipe, entity.items.get(i).cloned().flatten(), &Slot::default());
            op.map(|o| (o, max))
        })
        .collect();
    f.tick(&mut inputs, &mut outputs, &operations);
    for i in 0..n {
        set_slot(entity, i, &inputs[i]);
        set_slot(entity, FACTORY_ROW + i, &outputs[i]);
    }
    put_long(&mut entity.nbt, "energy", f.machine.energy.stored);
    entity.nbt.insert(
        "progress".into(),
        Tag::IntArray(f.progress.iter().map(|p| (*p).min(i32::MAX as u32) as i32).collect()),
    );
    Some(f.active)
}

fn max_stack(game: &Game<'_>, id: &str) -> u32 {
    world_items!(game).get(id).map_or(64, |i| i.max_stack)
}

/// A Precision Sawmill's secondary output still waiting for the output slot.
fn secondary(nbt: &Compound) -> Option<(String, u32)> {
    let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("secondary")) else {
        return None;
    };
    let Some(Tag::String(id)) = c.get(&JavaString::from("id")) else {
        return None;
    };
    Some((id.to_string().ok()?, get_int(c, "count").max(1)))
}

fn set_secondary(nbt: &mut Compound, waiting: Option<&(String, u32)>) {
    match waiting {
        Some((id, count)) => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_int(&mut c, "count", *count);
            nbt.insert("secondary".into(), Tag::Compound(c));
        }
        None => {
            nbt.remove(&JavaString::from("secondary"));
        }
    }
}

/// A number in [0, 1) for the sawmill's chance: a splitmix64 step kept in the block entity, so each
/// machine has its own sequence that survives saving.
fn roll(entity: &mut ModBlockEntity) -> f64 {
    let mut x = get_long(&entity.nbt, "seed") ^ 0x9E37_79B9_7F4A_7C15;
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    put_long(&mut entity.nbt, "seed", x & (i64::MAX as u64));
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// The upgrades installed in a machine (`upgrades` in its NBT: one count per upgrade name).
fn upgrades(nbt: &Compound) -> Upgrades {
    let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("upgrades")) else {
        return Upgrades::default();
    };
    let mut u = Upgrades::default();
    for kind in Upgrade::ALL {
        u.install(kind, get_int(c, kind.name()));
    }
    u
}

fn set_upgrades(nbt: &mut Compound, u: &Upgrades) {
    let mut c = Compound::new();
    for kind in Upgrade::ALL {
        if u.get(kind) > 0 {
            put_int(&mut c, kind.name(), u.get(kind));
        }
    }
    nbt.insert("upgrades".into(), Tag::Compound(c));
}

/// The `mekanism:upgrade_type` in a stack's custom data (the carried recipes put it there).
fn upgrade_of(stack: &ItemStack) -> Option<Upgrade> {
    if &*stack.item != "mekanism:upgrade" {
        return None;
    }
    let (_, data) = stack.extra.as_ref()?.other.iter().find(|(k, _)| k == "minecraft:custom_data")?;
    let Tag::Compound(c) = data else {
        return None;
    };
    match c.get(&JavaString::from("mekanism:upgrade_type")) {
        Some(Tag::String(id)) => Upgrade::from_id(&id.to_string().ok()?),
        _ => None,
    }
}

/// Right click on a Mekanism machine holding a speed or energy upgrade: one goes in (all that fit when
/// sneaking), as dropping it in the machine's upgrade tab does. True when the click was used.
pub(super) fn install_upgrade(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let selected = game.selected;
    let Some(Some(held)) = game.inventory.get(selected).cloned() else {
        return false;
    };
    let Some(kind) = upgrade_of(&held) else {
        return false;
    };
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let want = if sneaking { held.count } else { 1 };
    let installed = game.native_with_mod_block_entity(pos, |m| {
        if !matches!(self::kind(&m.id), Some(Kind::Machine(..) | Kind::Factory(..))) {
            return None;
        }
        let mut u = upgrades(&m.nbt);
        let n = u.install(kind, want);
        set_upgrades(&mut m.nbt, &u);
        Some((n, u.get(kind)))
    });
    let Some(Some((n, now))) = installed else {
        return false;
    };
    if n > 0 {
        let left = held.count - n;
        game.inventory[selected] = (left > 0).then(|| ItemStack { count: left, ..held });
    }
    if let Some(player) = game.native_acting_player() {
        let line = format!("{} upgrades: {now}/{}", kind.name(), kind.max());
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
    true
}

/// What the machine of kind `recipe` does with `stack` in its input (and `middle`, the Combiner's extra
/// input): the operation, the output's stack size, and the sawmill's secondary output with its chance.
#[allow(clippy::type_complexity)]
fn find_operation(
    game: &Game<'_>,
    recipe: &str,
    stack: Option<ItemStack>,
    middle: &Slot,
) -> (Option<Operation>, u32, Option<(String, u32, f64)>) {
    let Some(stack) = stack else {
        return (None, 64, None);
    };
    let items = world_items!(game);
    let max = |id: &str| items.get(id).map_or(64, |i| i.max_stack);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    let op = |input_count, extra_count, output: &str, output_count| Operation {
        input_count,
        extra_count,
        output: output.to_owned(),
        output_count,
    };
    match recipe {
        "smelting" => {
            let Some(cooking) = world_recipes!(game).cook("minecraft:smelting", &stack) else {
                return (None, 64, None);
            };
            let out = cooking.result.item.to_string();
            let m = max(&out);
            (Some(op(1, 0, &out, cooking.result.count)), m, None)
        }
        "sawing" => {
            let Some(r) = mekanism_core::recipes::find_sawing(&stack.item, stack.count, &in_tag) else {
                return (None, 64, None);
            };
            // A recipe with only a secondary output gives it every time here.
            let (main, chance) = match (r.main, r.secondary) {
                (Some(main), Some((id, n))) => (main, Some((id.to_owned(), n, r.secondary_chance))),
                (Some(main), None) => (main, None),
                (None, Some(sec)) => (sec, None),
                (None, None) => return (None, 64, None),
            };
            (Some(op(r.input_count, 0, main.0, main.1)), max(main.0), chance)
        }
        "combining" => {
            let Some(extra) = middle.item.as_deref() else {
                return (None, 64, None);
            };
            let Some(r) = mekanism_core::recipes::find_combining(
                (&stack.item, stack.count),
                (extra, middle.count),
                &in_tag,
            ) else {
                return (None, 64, None);
            };
            (Some(op(r.main_count, r.extra_count, r.output, r.output_count)), max(r.output), None)
        }
        _ => {
            let Some(r) = mekanism_core::recipes::find(
                mekanism_core::recipes::table(recipe),
                &stack.item,
                stack.count,
                &in_tag,
            ) else {
                return (None, 64, None);
            };
            (Some(op(r.input_count, 0, r.output, r.output_count)), max(r.output), None)
        }
    }
}

// --- Heat Generator ---------------------------------------------------------------------------------

fn tick_generator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut g = HeatGenerator::new();
    g.energy.stored = get_long(&entity.nbt, "energy").min(g.energy.capacity);
    g.lava = get_int(&entity.nbt, "lava");
    if let Some(heat) = get_double(&entity.nbt, "heat") {
        g.heat.heat = heat;
    }
    // FluidFuelInventorySlot.fillOrBurn: one furnace fuel item at a time becomes lava, when it fits.
    if let Some(Some(fuel)) = entity.items.get(1).cloned() {
        let burn = game.res.providers().burn_time(world_items!(game), &fuel, false).max(0) as u32;
        if g.accepts_fuel(burn) {
            g.add_fuel(burn);
            let left = fuel.count - 1;
            entity.items[1] = if left > 0 {
                Some(ItemStack { count: left, ..fuel })
            } else if &*fuel.item == "minecraft:lava_bucket" {
                Some(stack_of("minecraft:bucket", 1))
            } else {
                None
            };
        }
    }
    g.tick();
    push_energy(game, pos, &mut g.energy, None, false);
    put_long(&mut entity.nbt, "energy", g.energy.stored);
    put_int(&mut entity.nbt, "lava", g.lava);
    entity.nbt.insert("heat".into(), Tag::Double(g.heat.heat.to_bits()));
    // The flame shows the lava left, the arrow the temperature above ambient (up to 200 K).
    let warm = (g.heat.temperature() - mekanism_core::heat::AMBIENT_TEMP).clamp(0.0, 200.0) as i32;
    entity.data = vec![g.lava as i32, mekanism_core::generator::TANK_CAPACITY as i32, warm, 200];
    Some(g.active)
}

// --- Solar and wind ---------------------------------------------------------------------------------

/// `Level.canSeeSky`: nothing above `pos` that blocks the sky up to the top of the level.
fn can_see_sky(game: &Game<'_>, pos: [i32; 3]) -> bool {
    let top = game.world.top_y();
    (pos[1] + 1..top).all(|y| {
        let f = game.res.facts().get(game.world.block(pos[0], y, pos[2]));
        !f.full_collision && f.fluid == horizon_client::blocks::Fluid::None
    })
}

fn overworld(game: &Game<'_>) -> bool {
    game.dim == super::Dim::Overworld
}

/// The solar generator's sun: `LightLevelUtils.canSeeSun` and `getSunBrightness` (the sky light
/// factor, with the weather), and `SolarCheck`'s biome multiplier.
fn tick_solar(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, peak: u64, storage: u64) -> Option<bool> {
    let mut energy = EnergyContainer::new(storage, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(storage);
    let sees_sun = overworld(game) && game.sky_darken() < 4 && can_see_sky(game, pos);
    let produced = if sees_sun {
        let mut sky = game.res.environment().sample(game.world.day_time, None, None);
        let (rain, thunder) = game.world.weather.levels(1.0);
        horizon_client::environment::apply_weather(&mut sky, rain, thunder);
        let biome = game.world.chunks.get(&(pos[0] >> 4, pos[2] >> 4)).map(|c| {
            let b = &game.res.biomes().list[c.biome(pos[0], pos[1], pos[2]) as usize];
            (b.temperature, b.downfall, b.precipitation)
        });
        let (t, d, rains) = biome.unwrap_or((0.8, 0.4, true));
        let m = mekanism_core::generator::solar_biome_multiplier(t, d, rains);
        mekanism_core::generator::solar_production(peak, true, sky.sky_light_factor, m)
    } else {
        0
    };
    energy.insert(produced, Action::Execute);
    push_energy(game, pos, &mut energy, None, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    entity.data = vec![(energy.stored * 1000 / storage.max(1)) as i32, 1000, produced as i32, peak as i32];
    Some(produced > 0)
}

/// The wind generator: its multiplier is worked out once a second, from the height of its top.
fn tick_wind(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    use mekanism_core::generator as g;
    let mut energy = EnergyContainer::new(g::WIND_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(g::WIND_STORAGE);
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    let mut multiplier = get_double(&entity.nbt, "multiplier").unwrap_or(0.0);
    if ticker == 0 {
        let top = [pos[0], pos[1] + 4, pos[2]];
        let fluid = game.res.facts().get(game.world.block(top[0], top[1], top[2])).fluid;
        multiplier = if fluid == horizon_client::blocks::Fluid::None && can_see_sky(game, top) {
            g::wind_multiplier(top[1], game.world.bottom_y(), game.world.top_y() - 1)
        } else {
            0.0
        };
        entity.nbt.insert("multiplier".into(), Tag::Double(multiplier.to_bits()));
    }
    let produced = if multiplier > 0.0 { g::wind_generation(multiplier) } else { 0 };
    energy.insert(produced, Action::Execute);
    push_energy(game, pos, &mut energy, None, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    entity.data = vec![(energy.stored * 1000 / g::WIND_STORAGE) as i32, 1000, produced as i32, g::WIND_GENERATION_MAX as i32];
    Some(produced > 0)
}

// --- Energy cubes -----------------------------------------------------------------------------------

fn tick_cube(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, tier: BaseTier) {
    let (capacity, rate) = tier::energy_cube(tier);
    let mut energy = EnergyContainer::new(capacity, rate);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    // A cube's default side configuration: energy goes out of its front only.
    let front = game
        .native_block_property(pos, "facing")
        .and_then(|f| direction(&f));
    if let Some(front) = front {
        push_energy(game, pos, &mut energy, Some(front), false);
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    entity.data = vec![(energy.stored * 1000 / capacity.max(1)) as i32, 1000, 0, 0];
}

// --- Universal cables -------------------------------------------------------------------------------

/// A cable holds one tick of what it carries. It gives to the machines and cubes beside it first, then
/// lets its energy run on to emptier cables (`tier::cable_share`), so a line of cables carries energy
/// from generators to machines without it flowing back. Mekanism pools a whole cable network instead;
/// the throughput per cable is the same.
fn tick_cable(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, rate: u64) {
    let mut energy = EnergyContainer::new(rate, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(rate);
    if energy.stored > 0 {
        push_energy(game, pos, &mut energy, None, true);
    }
    for (_, d) in DIRECTIONS {
        if energy.stored == 0 {
            break;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let mine = energy.stored;
        let given = game.native_with_mod_block_entity(at, |m| {
            let Some(Kind::Cable(their_rate)) = kind(&m.id) else {
                return 0;
            };
            let theirs = get_long(&m.nbt, "energy");
            let give = tier::cable_share(mine, theirs).min(their_rate.saturating_sub(theirs));
            put_long(&mut m.nbt, "energy", theirs + give);
            give
        });
        if let Some(given) = given {
            energy.extract(given, Action::Execute);
        }
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
}

// --- Chemicals --------------------------------------------------------------------------------------

fn get_tank(nbt: &Compound, key: &str, capacity: u64) -> ChemicalTank {
    let mut t = ChemicalTank::new(capacity);
    if let Some(Tag::Compound(c)) = nbt.get(&JavaString::from(key))
        && let Some(Tag::String(id)) = c.get(&JavaString::from("id"))
        && let Ok(id) = id.to_string()
    {
        t.insert(&id, get_long(c, "amount"));
    }
    t
}

fn put_tank(nbt: &mut Compound, key: &str, t: &ChemicalTank) {
    match &t.chemical {
        Some(id) if t.amount > 0 => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_long(&mut c, "amount", t.amount);
            nbt.insert(key.into(), Tag::Compound(c));
        }
        _ => {
            nbt.remove(&JavaString::from(key));
        }
    }
}

/// Where a neighbour keeps chemicals it accepts: its NBT key and capacity, for `chemical`.
fn chemical_receiver(id: &str, chemical: &str) -> Option<(&'static str, u64)> {
    match kind(id)? {
        Kind::ChemicalTank(t) => Some(("chemical", chem::chemical_tank(t).0)),
        Kind::GasGenerator if chem::fuel(chemical).is_some() => Some(("fuel", chem::GAS_GENERATOR_TANK)),
        _ => None,
    }
}

/// Pushes up to `rate` of `from`'s chemical into the tanks and generators beside `pos` (or only the one
/// at `only`) that accept it.
fn push_chemical(game: &mut Game<'_>, pos: [i32; 3], from: &mut ChemicalTank, only: Option<[i32; 3]>, rate: u64) {
    let mut budget = rate;
    for (_, d) in DIRECTIONS {
        if only.is_some_and(|o| o != d) {
            continue;
        }
        let Some(chemical) = from.chemical.clone() else {
            return;
        };
        let offer = from.amount.min(budget);
        if offer == 0 {
            return;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let taken = game.native_with_mod_block_entity(at, |m| {
            let Some((key, capacity)) = chemical_receiver(&m.id, &chemical) else {
                return 0;
            };
            let mut to = get_tank(&m.nbt, key, capacity);
            let n = to.insert(&chemical, offer);
            put_tank(&mut m.nbt, key, &to);
            n
        });
        if let Some(n) = taken {
            from.extract(n);
            budget -= n;
        }
    }
}

/// The Chemical Oxidizer: an item becomes a chemical over 100 ticks at 15 FE a tick, pushed out to the
/// tanks beside it.
fn tick_oxidizer(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = chem::OXIDIZER_USAGE * u64::from(chem::OXIDIZER_TICKS);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "output", chem::OXIDIZER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let recipe = entity.items.first().cloned().flatten().and_then(|stack| {
        let items = world_items!(game);
        let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
        mekanism_core::recipes::find_oxidizing(&stack.item, stack.count, &in_tag).map(|r| (stack, *r))
    });
    match recipe {
        Some((stack, r)) if tank.accepts(r.output) && tank.amount + r.amount <= tank.capacity => {
            if energy.consume(chem::OXIDIZER_USAGE) {
                active = true;
                progress += 1;
                if progress >= chem::OXIDIZER_TICKS {
                    progress = 0;
                    tank.insert(r.output, r.amount);
                    let left = stack.count - r.input_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                }
            }
        }
        _ => progress = 0,
    }
    push_chemical(game, pos, &mut tank, None, u64::MAX);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "output", &tank);
    entity.data = vec![
        (energy.stored * 1000 / capacity) as i32,
        1000,
        progress as i32,
        chem::OXIDIZER_TICKS as i32,
    ];
    Some(active)
}

/// The Electrolytic Separator on water: water buckets fill its 24,000 mB tank, each tick splits 2 mB
/// into hydrogen and oxygen at 4 FE, and the gases go out to the tanks and generators beside it.
fn tick_separator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * chem::SEPARATOR_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut water = get_long(&entity.nbt, "water").min(chem::SEPARATOR_FLUID);
    // FluidInventorySlot.fill: a water bucket empties into the tank when it fits and the bucket has
    // somewhere to go.
    if let Some(Some(bucket)) = entity.items.first().cloned()
        && &*bucket.item == "minecraft:water_bucket"
        && water + 1_000 <= chem::SEPARATOR_FLUID
    {
        let out = entity.items.get(2).cloned().flatten();
        let room = match &out {
            None => true,
            Some(s) => &*s.item == "minecraft:bucket" && s.count < 16,
        };
        if room {
            water += 1_000;
            entity.items[0] = None;
            entity.items[2] = Some(match out {
                Some(s) => ItemStack { count: s.count + 1, ..s },
                None => stack_of("minecraft:bucket", 1),
            });
        }
    }
    let mut hydrogen = get_tank(&entity.nbt, "left", chem::SEPARATOR_GAS);
    let mut oxygen = get_tank(&entity.nbt, "right", chem::SEPARATOR_GAS);
    let active = chem::separate_water(&mut water, &mut hydrogen, &mut oxygen, &mut energy);
    push_chemical(game, pos, &mut hydrogen, None, u64::MAX);
    push_chemical(game, pos, &mut oxygen, None, u64::MAX);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_long(&mut entity.nbt, "water", water);
    put_tank(&mut entity.nbt, "left", &hydrogen);
    put_tank(&mut entity.nbt, "right", &oxygen);
    // The flame shows the water, the arrow the hydrogen tank.
    entity.data = vec![
        (water * 1000 / chem::SEPARATOR_FLUID) as i32,
        1000,
        (hydrogen.amount * 1000 / chem::SEPARATOR_GAS) as i32,
        1000,
    ];
    Some(active)
}

fn tick_gas_generator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut energy = EnergyContainer::new(chem::GAS_GENERATOR_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(chem::GAS_GENERATOR_STORAGE);
    let mut fuel = get_tank(&entity.nbt, "fuel", chem::GAS_GENERATOR_TANK);
    let (made, _) = chem::burn_fuel(&mut fuel, energy.capacity - energy.stored);
    energy.insert(made, Action::Execute);
    push_energy(game, pos, &mut energy, None, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fuel", &fuel);
    Some(made > 0)
}

/// A chemical tank takes from every side and gives out of its bottom at its tier's rate.
fn tick_chemical_tank(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, tier: BaseTier) {
    let (capacity, rate) = chem::chemical_tank(tier);
    let mut tank = get_tank(&entity.nbt, "chemical", capacity);
    if tank.amount > 0 {
        push_chemical(game, pos, &mut tank, Some([0, -1, 0]), rate);
        put_tank(&mut entity.nbt, "chemical", &tank);
    }
}

// --- Energy between blocks --------------------------------------------------------------------------

const DIRECTIONS: [(&str, [i32; 3]); 6] = [
    ("down", [0, -1, 0]),
    ("up", [0, 1, 0]),
    ("north", [0, 0, -1]),
    ("south", [0, 0, 1]),
    ("west", [-1, 0, 0]),
    ("east", [1, 0, 0]),
];

fn direction(name: &str) -> Option<[i32; 3]> {
    DIRECTIONS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// Pushes `from`'s energy into the Mekanism blocks beside `pos` that accept it, on every side or only
/// `only`; split evenly the way Mekanism's emitter splits to its targets (what one refuses goes on).
fn push_energy(
    game: &mut Game<'_>,
    pos: [i32; 3],
    from: &mut EnergyContainer,
    only: Option<[i32; 3]>,
    from_cable: bool,
) {
    let sides: Vec<[i32; 3]> = DIRECTIONS
        .iter()
        .map(|(_, d)| *d)
        .filter(|d| only.is_none_or(|o| o == *d))
        .collect();
    for (i, d) in sides.iter().enumerate() {
        let offer = from.extract(u64::MAX, Action::Simulate) / (sides.len() - i) as u64;
        if offer == 0 {
            continue;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let back = [-d[0], -d[1], -d[2]];
        let accepted = game
            .native_block_property(at, "facing")
            .and_then(|f| direction(&f))
            .map_or(true, |front| front != back);
        let taken = game.native_with_mod_block_entity(at, |m| {
            let Some((capacity, rate)) = receiver(&m.id, &m.nbt, accepted, from_cable) else {
                return 0;
            };
            let mut to = EnergyContainer::new(capacity, rate);
            to.stored = get_long(&m.nbt, "energy").min(capacity);
            let refused = to.insert(offer, Action::Execute);
            put_long(&mut m.nbt, "energy", to.stored);
            offer - refused
        });
        if let Some(taken) = taken {
            from.extract(taken, Action::Execute);
        }
    }
}

/// The energy a neighbour takes: (capacity, rate) when `id` is a Mekanism block that accepts energy on
/// the side it is offered; a cube refuses on its front (`facing_away` is false there).
fn receiver(id: &str, nbt: &Compound, facing_away: bool, from_cable: bool) -> Option<(u64, u64)> {
    match kind(id)? {
        Kind::Machine(_, usage) => Some((
            ElectricMachine::with_upgrades(usage, upgrades(nbt)).energy.capacity,
            u64::MAX,
        )),
        Kind::Factory(n, _, usage) => Some((Factory::new(n, usage, upgrades(nbt)).machine.energy.capacity, u64::MAX)),
        Kind::EnergyCube(t) if facing_away => Some(tier::energy_cube(t)),
        Kind::Cable(rate) if !from_cable => Some((rate, rate)),
        Kind::Oxidizer => Some((chem::OXIDIZER_USAGE * u64::from(chem::OXIDIZER_TICKS), u64::MAX)),
        Kind::Separator => Some((machine::STORAGE_MULTIPLIER * chem::SEPARATOR_USAGE, u64::MAX)),
        _ => None,
    }
}

// --- NBT and slots ----------------------------------------------------------------------------------

fn get_long(nbt: &Compound, key: &str) -> u64 {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::Long(v)) => (*v).max(0) as u64,
        Some(Tag::Int(v)) => (*v).max(0) as u64,
        _ => 0,
    }
}

fn get_int(nbt: &Compound, key: &str) -> u32 {
    get_long(nbt, key).min(u32::MAX as u64) as u32
}

fn get_double(nbt: &Compound, key: &str) -> Option<f64> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::Double(bits)) => Some(f64::from_bits(*bits)),
        _ => None,
    }
}

fn put_long(nbt: &mut Compound, key: &str, v: u64) {
    nbt.insert(key.into(), Tag::Long(v.min(i64::MAX as u64) as i64));
}

fn put_int(nbt: &mut Compound, key: &str, v: u32) {
    nbt.insert(key.into(), Tag::Int(v.min(i32::MAX as u32) as i32));
}

fn stack_of(item: &str, count: u32) -> ItemStack {
    ItemStack { item: Arc::from(item), count, damage: 0, extra: None }
}

fn slot(stack: Option<ItemStack>) -> Slot {
    match stack {
        Some(s) => Slot { item: Some(s.item.to_string()), count: s.count },
        None => Slot::default(),
    }
}

/// Writes `slot` back to item slot `index`, keeping the stack's other data when the item is the same.
fn set_slot(entity: &mut ModBlockEntity, index: usize, slot: &Slot) {
    let Some(place) = entity.items.get_mut(index) else {
        return;
    };
    *place = match (&slot.item, place.take()) {
        (None, _) => None,
        (Some(item), Some(old)) if &*old.item == item.as_str() => Some(ItemStack { count: slot.count, ..old }),
        (Some(item), _) => Some(stack_of(item, slot.count)),
    };
}

// --- Energy kept in items ---------------------------------------------------------------------------

/// The key in an item's `minecraft:custom_data` that holds its energy (FE), as Mekanism's
/// `mekanism:energy` component does.
const ENERGY_KEY: &str = "mekanism:energy";

fn custom_data(stack: &ItemStack) -> Option<&Compound> {
    let (_, data) = stack.extra.as_ref()?.other.iter().find(|(k, _)| k == "minecraft:custom_data")?;
    match data {
        Tag::Compound(c) => Some(c),
        _ => None,
    }
}

fn stored_energy(stack: &ItemStack) -> u64 {
    custom_data(stack).map_or(0, |c| get_long(c, ENERGY_KEY))
}

fn with_energy(item: &str, energy: u64) -> ItemStack {
    let mut c = Compound::new();
    put_long(&mut c, ENERGY_KEY, energy);
    let data = horizon_client::item_data::StackData {
        other: vec![("minecraft:custom_data".to_owned(), Tag::Compound(c))],
        ..Default::default()
    };
    ItemStack { extra: Some(Box::new(data)), ..stack_of(item, 1) }
}

/// Breaking an energy cube that holds energy outside creative: the block goes as `Level.destroyBlock`
/// takes it (particles, sound, no loot) and drops the cube with its energy, as Mekanism's loot copies
/// the energy component. True when it did, so the game's own break is canceled.
pub(super) fn break_keeping_energy(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || game.creative {
        return false;
    }
    let held = game.native_with_mod_block_entity(pos, |m| match kind(&m.id) {
        Some(Kind::EnergyCube(_)) => Some((m.id.clone(), get_long(&m.nbt, "energy"))),
        _ => None,
    });
    let Some(Some((id, energy))) = held else {
        return false;
    };
    if energy == 0 {
        return false;
    }
    let at = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
    let cube = with_energy(&id, energy);
    game.with_level(|b, level, ticks| {
        b.destroy_block(level, ticks, at, false);
        level.drop_lost_contents(at, vec![Some(cube)]);
    });
    true
}

/// A placed energy cube takes the energy its item held.
pub(super) fn placed(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    let energy = stored_energy(stack);
    if energy == 0 || !game.native_on(MOD_ID) {
        return;
    }
    game.native_with_mod_block_entity(pos, |m| {
        if let Some(Kind::EnergyCube(t)) = kind(&m.id) {
            put_long(&mut m.nbt, "energy", energy.min(tier::energy_cube(t).0));
        }
    });
}

/// "Stored Energy: 1.2 MFE" under a Mekanism item that holds energy.
pub(super) fn tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(horizon_client::text::Text, u32)>) {
    if !game.native_on(MOD_ID) || !stack.item.starts_with("mekanism") {
        return;
    }
    let energy = stored_energy(stack);
    if energy == 0 {
        return;
    }
    let line = format!("Stored Energy: {}", mekanism_core::energy::format_fe(energy));
    lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0x55, 0xFF, 0xFF, 0xFF)));
}
