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
use mekanism_core::evaporation as evap;
use mekanism_core::gear;
use mekanism_core::sides::Sides;
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
    /// A pressurized tube and the mB it carries in a tick.
    Tube(u64),
    Oxidizer,
    /// A purification chamber or osmium compressor: an item and a chemical each tick.
    Advanced(&'static chem::AdvancedMachine),
    ChemicalInfuser,
    /// A Thermal Evaporation Plant's controller; the tower's other blocks have no block entity.
    Evaporation,
    /// A Rotary Condensentrator, decondensentrating: a fluid into its chemical.
    Rotary,
    /// An Induction Matrix's port; the casing, cells and providers have no block entity.
    InductionPort,
    /// A logistical transporter and how many items it pulls at a time.
    Transporter(u32),
    Infuser,
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
        "mekanism:chemical_infuser" => return Some(Kind::ChemicalInfuser),
        "mekanism:thermal_evaporation_controller" => return Some(Kind::Evaporation),
        "mekanism:rotary_condensentrator" => return Some(Kind::Rotary),
        "mekanism:induction_port" => return Some(Kind::InductionPort),
        "mekanism:metallurgic_infuser" => return Some(Kind::Infuser),
        "mekanism:electrolytic_separator" => return Some(Kind::Separator),
        _ => {}
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_universal_cable")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::universal_cable(tier).map(Kind::Cable);
    }
    if let Some(m) = chem::ADVANCED_MACHINES.iter().find(|m| m.block == id) {
        return Some(Kind::Advanced(m));
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_logistical_transporter")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::transporter_pull(tier).map(Kind::Transporter);
    }
    if let Some(tier) = id.strip_prefix("mekanism:").and_then(|t| t.strip_suffix("_pressurized_tube")) {
        let tier = BaseTier::ALL.into_iter().find(|t| t.name() == tier)?;
        return tier::pressurized_tube(tier).map(Kind::Tube);
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
        // The heat generator burns from the middle slot; generators charge an item in the first.
        Some(Kind::HeatGenerator) => (vec![0, 1], vec![]),
        Some(Kind::Wind | Kind::GasGenerator) => (vec![0], vec![]),
        // The plain solar generator is an iron block with no menu.
        Some(Kind::Solar(..)) if block.ends_with("advanced_solar_generator") => (vec![0], vec![]),
        // The oxidizer's item goes in the first slot; the separator takes water buckets there and gives
        // the empty buckets back in the last.
        Some(Kind::Oxidizer) => (vec![0], vec![]),
        Some(Kind::Separator) => (vec![0], vec![2]),
        // An advanced machine's middle slot takes items that turn into its chemical (flint → oxygen).
        Some(Kind::Advanced(_)) => (vec![0, 1], vec![2]),
        // Water buckets in the first slot (empty buckets out of the last), and an empty bucket in the
        // middle one fills with brine.
        Some(Kind::Evaporation) => (vec![0, 1], vec![2]),
        Some(Kind::Rotary) => (vec![0], vec![2]),
        // The item to infuse in the first slot, the infuse item (coal, redstone...) in the middle one.
        Some(Kind::Infuser) => (vec![0, 1], vec![2]),
        // An energy cube's charge slot (top) and discharge slot (bottom).
        Some(Kind::EnergyCube(_)) => (vec![0, 1], vec![]),
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
    TICKING.set(Some((pos, sides_of(&entity.nbt))));
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
        Kind::Advanced(m) => tick_advanced(game, entity, m),
        Kind::ChemicalInfuser => tick_chemical_infuser(game, pos, entity),
        Kind::Evaporation => tick_evaporation(game, pos, entity),
        Kind::Rotary => tick_rotary(game, pos, entity),
        Kind::Transporter(pull) => {
            tick_transporter(game, pos, entity, pull);
            None
        }
        Kind::InductionPort => {
            tick_induction_port(game, pos, entity);
            None
        }
        Kind::Infuser => tick_infuser(game, entity),
        Kind::Separator => tick_separator(game, pos, entity),
        Kind::GasGenerator => tick_gas_generator(game, pos, entity),
        Kind::ChemicalTank(t) => {
            tick_chemical_tank(game, pos, entity, t);
            None
        }
        Kind::Tube(rate) => {
            tick_tube(game, pos, entity, rate);
            None
        }
    };
    if matches!(k, Kind::Cable(_) | Kind::Tube(_) | Kind::Transporter(_)) {
        show_connections(game, pos, entity);
    }
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
    // The middle slot is the machine's energy slot (the combiner's holds its extra input instead): an
    // energy tablet or cube there runs the machine.
    if recipe != "combining" {
        discharge_slot(entity, 1, &mut m.energy);
    }
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
    // The first slot charges an item from the generator (Mekanism's energy slot).
    charge_slot(entity, 0, &mut g.energy);
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
    charge_slot(entity, 0, &mut energy);
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
    charge_slot(entity, 0, &mut energy);
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
    // The top slot charges the item in it from the cube; the bottom one empties its item into it.
    charge_slot(entity, 0, &mut energy);
    discharge_slot(entity, 1, &mut energy);
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

/// Where a neighbour keeps chemicals it accepts: its NBT key and capacity, for `chemical`. Tubes take
/// from machines and tanks but not from other tubes, which share by gradient instead (`tick_tube`).
/// `d` is the receiver's offset from the giver: a tank takes nothing into its bottom, its output side.
fn chemical_receiver(id: &str, nbt: &Compound, chemical: &str, from_tube: bool, d: [i32; 3]) -> Option<(&'static str, u64)> {
    let kind = kind(id)?;
    // Fluids (water, brine) share the chemical code here but go only into the separator, since there
    // are no fluid pipes or fluid tanks yet.
    if FLUIDS.contains(&chemical) {
        return match kind {
            Kind::Separator if chem::separates(chemical) => Some(("fluid", chem::SEPARATOR_FLUID)),
            _ => None,
        };
    }
    match kind {
        Kind::Tube(rate) if !from_tube => Some(("chemical", rate)),
        Kind::ChemicalTank(t) if d != [0, 1, 0] => Some(("chemical", chem::chemical_tank(t).0)),
        Kind::Advanced(m) if m.chemicals.contains(&chemical) && get_tank(nbt, "chemical", chem::ADVANCED_TANK).accepts(chemical) => Some(("chemical", chem::ADVANCED_TANK)),
        // The chemical infuser fills whichever input tank holds this chemical, else an empty one.
        Kind::ChemicalInfuser if mekanism_core::recipes::infusing_input(chemical) => {
            let cap = chem::CHEMICAL_INFUSER_TANK;
            let (left, right) = (get_tank(nbt, "left", cap), get_tank(nbt, "right", cap));
            let holds = |t: &ChemicalTank| t.chemical.as_deref() == Some(chemical);
            let key = if holds(&left) || (!holds(&right) && left.amount == 0) {
                "left"
            } else if holds(&right) || right.amount == 0 {
                "right"
            } else {
                return None;
            };
            Some((key, cap))
        }
        Kind::GasGenerator if chem::fuel(chemical).is_some() => Some(("fuel", chem::GAS_GENERATOR_TANK)),
        _ => None,
    }
}

thread_local! {
    /// The block entity being ticked and its side configuration. `push_energy` and `push_chemical` are
    /// called with the ticking block's containers taken out of its NBT, so they read its sides here.
    static TICKING: std::cell::Cell<Option<([i32; 3], Sides)>> = const { std::cell::Cell::new(None) };
}

fn sides_of(nbt: &Compound) -> Sides {
    Sides(get_long(nbt, "sides") as u32)
}

fn face_index(d: [i32; 3]) -> usize {
    DIRECTIONS.iter().position(|(_, v)| *v == d).unwrap_or(0)
}

/// Whether the block at `pos` may give out through face `d`, by its side configuration.
fn gives_out(pos: [i32; 3], d: [i32; 3]) -> bool {
    match TICKING.get() {
        Some((at, sides)) if at == pos => sides.get(face_index(d)).gives_out(),
        _ => true,
    }
}

/// Whether a receiver takes in through its face `back` (the one facing the giver).
fn takes_in(nbt: &Compound, back: [i32; 3]) -> bool {
    sides_of(nbt).get(face_index(back)).takes_in()
}

/// The Configurator: a right click on a face of a Mekanism block cycles that face between Default,
/// None, Input and Output, and says so in chat.
pub(super) fn configure_side(game: &mut Game<'_>, pos: [i32; 3], face: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) {
        return false;
    }
    let holding = game.inventory.get(game.selected).cloned().flatten();
    if !holding.is_some_and(|h| &*h.item == "mekanism:configurator") {
        return false;
    }
    let mode = game.native_with_mod_block_entity(pos, |m| {
        kind(&m.id)?;
        let mut sides = sides_of(&m.nbt);
        let mode = sides.cycle(face_index(face));
        put_long(&mut m.nbt, "sides", u64::from(sides.0));
        Some(mode)
    });
    let Some(Some(mode)) = mode else {
        return false;
    };
    let name = DIRECTIONS[face_index(face)].0;
    game.system_message(horizon_client::text::Text::literal(format!("Side {name}: {}", mode.name())));
    true
}

const FLUIDS: [&str; 2] = ["minecraft:water", "mekanism:brine"];

/// Pushes up to `rate` of `from`'s chemical into the tanks and generators beside `pos` (or only the one
/// at `only`) that accept it.
fn push_chemical(
    game: &mut Game<'_>,
    pos: [i32; 3],
    from: &mut ChemicalTank,
    only: Option<[i32; 3]>,
    rate: u64,
    from_tube: bool,
) {
    let mut budget = rate;
    for (_, d) in DIRECTIONS {
        if only.is_some_and(|o| o != d) || !gives_out(pos, d) {
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
            if !takes_in(&m.nbt, [-d[0], -d[1], -d[2]]) {
                return 0;
            }
            let Some((key, capacity)) = chemical_receiver(&m.id, &m.nbt, &chemical, from_tube, d) else {
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
    push_chemical(game, pos, &mut tank, None, u64::MAX, false);
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

/// An advanced electric machine: the Purification Chamber (ore + 1 mB oxygen a tick → 3 clumps) or the
/// Osmium Compressor (refined obsidian or glowstone dust + osmium → ingot), 200 ticks. Its chemical comes
/// from the blocks beside it, or from items in the middle slot (`chemical_conversion`: flint → oxygen,
/// osmium ingot → osmium).
fn tick_advanced(game: &mut Game<'_>, entity: &mut ModBlockEntity, m: &'static chem::AdvancedMachine) -> Option<bool> {
    let ticks = chem::ADVANCED_TICKS;
    let capacity = m.usage * u64::from(ticks);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "chemical", chem::ADVANCED_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let items = world_items!(game);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    if let Some(Some(extra)) = entity.items.get(1).cloned()
        && let Some(c) = mekanism_core::recipes::find_conversion(&extra.item, &in_tag)
        && m.chemicals.contains(&c.output)
        && tank.accepts(c.output)
        && tank.amount + c.amount <= tank.capacity
    {
        tank.insert(c.output, c.amount);
        entity.items[1] = (extra.count > 1).then(|| ItemStack { count: extra.count - 1, ..extra });
    }
    let recipe = entity.items.first().cloned().flatten().and_then(|stack| {
        // With a chemical in the tank, only its recipes (the injection chamber takes two chemicals).
        let held = tank.chemical.as_deref();
        m.table
            .iter()
            .find(|r| {
                held.is_none_or(|c| c == r.chemical)
                    && stack.count >= r.input_count
                    && r.input.matches(&stack.item, &in_tag)
            })
            .map(|r| (stack, *r))
    });
    let out = entity.items.get(2).cloned().flatten();
    match recipe {
        Some((stack, r)) => {
            let max = items.get(r.output).map_or(64, |i| i.max_stack);
            let fits = match &out {
                None => true,
                Some(o) => &*o.item == r.output && o.count + r.output_count <= max,
            };
            if fits
                && tank.chemical.as_deref() == Some(r.chemical)
                && tank.amount >= r.chemical_amount
                && energy.consume(m.usage)
            {
                active = true;
                tank.extract(r.chemical_amount);
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    let left = stack.count - r.input_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                    entity.items[2] = Some(match out {
                        Some(o) => ItemStack { count: o.count + r.output_count, ..o },
                        None => stack_of(r.output, r.output_count),
                    });
                }
            }
        }
        None => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "chemical", &tank);
    entity.data = vec![
        (energy.stored * 1000 / capacity) as i32,
        1000,
        progress as i32,
        ticks as i32,
    ];
    Some(active)
}

/// The Chemical Infuser: one operation a tick of two chemicals into a third (oxygen + sulfur dioxide →
/// sulfur trioxide), 15 FE a tick, pushed out to the blocks beside it. Each input tank takes one of the
/// two chemicals (`chemical_receiver`).
fn tick_chemical_infuser(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let cap = chem::CHEMICAL_INFUSER_TANK;
    let capacity = machine::STORAGE_MULTIPLIER * chem::CHEMICAL_INFUSER_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut left = get_tank(&entity.nbt, "left", cap);
    let mut right = get_tank(&entity.nbt, "right", cap);
    let mut out = get_tank(&entity.nbt, "output", cap);
    let mut active = false;
    if let (Some(a), Some(b)) = (left.chemical.clone(), right.chemical.clone())
        && let Some((r, swapped)) = mekanism_core::recipes::find_chemical_infusing(&a, &b)
    {
        let (need_a, need_b) = if swapped { (r.right_amount, r.left_amount) } else { (r.left_amount, r.right_amount) };
        if left.amount >= need_a
            && right.amount >= need_b
            && out.accepts(r.output)
            && out.amount + r.amount <= out.capacity
            && energy.consume(chem::CHEMICAL_INFUSER_USAGE)
        {
            left.extract(need_a);
            right.extract(need_b);
            out.insert(r.output, r.amount);
            active = true;
        }
    }
    push_chemical(game, pos, &mut out, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "left", &left);
    put_tank(&mut entity.nbt, "right", &right);
    put_tank(&mut entity.nbt, "output", &out);
    Some(active)
}

/// The Metallurgic Infuser: the middle slot's item (coal, redstone, diamond dust...) turns into infuse
/// chemical in a 1,000 mB tank (`chemical_conversion`), and an item with enough of it becomes the
/// recipe's output after 200 ticks at 5 FE a tick (`metallurgic_infusing`: iron + 10 carbon → enriched
/// iron).
fn tick_infuser(game: &mut Game<'_>, entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = chem::INFUSER_USAGE * u64::from(chem::INFUSER_TICKS);
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "infuse", chem::INFUSER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let items = world_items!(game);
    let in_tag = |tag: &str, item: &str| items.item_tags.get(tag).is_some_and(|t| t.contains(item));
    // ChemicalInventorySlot: one item at a time, when all of its chemical fits.
    if let Some(Some(infuse)) = entity.items.get(1).cloned()
        && let Some(c) = mekanism_core::recipes::find_conversion(&infuse.item, &in_tag)
        && tank.accepts(c.output)
        && tank.amount + c.amount <= tank.capacity
    {
        tank.insert(c.output, c.amount);
        entity.items[1] = (infuse.count > 1).then(|| ItemStack { count: infuse.count - 1, ..infuse });
    }
    let recipe = match (entity.items.first().cloned().flatten(), tank.chemical.clone()) {
        (Some(stack), Some(chemical)) => {
            mekanism_core::recipes::find_infusing(&stack.item, stack.count, &chemical, &in_tag).map(|r| (stack, *r))
        }
        _ => None,
    };
    let out = entity.items.get(2).cloned().flatten();
    match recipe {
        Some((stack, r)) if tank.amount >= r.chemical_amount => {
            let max = items.get(r.output).map_or(64, |i| i.max_stack);
            let fits = match &out {
                None => true,
                Some(o) => &*o.item == r.output && o.count + r.output_count <= max,
            };
            if fits && energy.consume(chem::INFUSER_USAGE) {
                active = true;
                progress += 1;
                if progress >= chem::INFUSER_TICKS {
                    progress = 0;
                    tank.extract(r.chemical_amount);
                    let left = stack.count - r.input_count;
                    entity.items[0] = (left > 0).then(|| ItemStack { count: left, ..stack });
                    entity.items[2] = Some(match out {
                        Some(o) => ItemStack { count: o.count + r.output_count, ..o },
                        None => stack_of(r.output, r.output_count),
                    });
                }
            }
        }
        _ => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "infuse", &tank);
    // The flame shows the infuse tank, the arrow the progress.
    entity.data = vec![
        (tank.amount * 1000 / chem::INFUSER_TANK) as i32,
        1000,
        progress as i32,
        chem::INFUSER_TICKS as i32,
    ];
    Some(active)
}

/// The Electrolytic Separator: water or brine buckets (or brine from a Thermal Evaporation Plant) fill
/// its 24,000 mB tank, and each tick runs one `separating` operation at 4 FE: 2 mB water → 2 hydrogen +
/// 1 oxygen, or 10 mB brine → 1 sodium + 1 chlorine. The chemicals go out to the blocks beside it.
fn tick_separator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * chem::SEPARATOR_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut fluid = get_tank(&entity.nbt, "fluid", chem::SEPARATOR_FLUID);
    // FluidInventorySlot.fill: a bucket empties into the tank when it fits and the bucket has
    // somewhere to go.
    let bucket_fluid = |item: &str| match item {
        "minecraft:water_bucket" => Some("minecraft:water"),
        "mekanism:brine_bucket" => Some("mekanism:brine"),
        _ => None,
    };
    if let Some(Some(bucket)) = entity.items.first().cloned()
        && let Some(f) = bucket_fluid(&bucket.item)
        && fluid.accepts(f)
        && fluid.amount + 1_000 <= fluid.capacity
        && give_back_bucket(entity)
    {
        fluid.insert(f, 1_000);
        entity.items[0] = None;
    }
    let mut left = get_tank(&entity.nbt, "left", chem::SEPARATOR_GAS);
    let mut right = get_tank(&entity.nbt, "right", chem::SEPARATOR_GAS);
    let active = chem::separate(&mut fluid, &mut left, &mut right, &mut energy);
    push_chemical(game, pos, &mut left, None, u64::MAX, false);
    push_chemical(game, pos, &mut right, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fluid", &fluid);
    put_tank(&mut entity.nbt, "left", &left);
    put_tank(&mut entity.nbt, "right", &right);
    // The flame shows the fluid, the arrow the left gas tank.
    entity.data = vec![
        (fluid.amount * 1000 / chem::SEPARATOR_FLUID) as i32,
        1000,
        (left.amount * 1000 / chem::SEPARATOR_GAS) as i32,
        1000,
    ];
    Some(active)
}

/// The fluid in a bucket item: vanilla's water bucket, or a Mekanism `<fluid>_bucket`.
fn bucket_fluid(item: &str) -> Option<String> {
    if item == "minecraft:water_bucket" {
        return Some("minecraft:water".into());
    }
    let fluid = item.strip_suffix("_bucket")?;
    fluid.starts_with("mekanism:").then(|| fluid.to_owned())
}

/// The Rotary Condensentrator turning fluid into chemical (`rotary` recipes: water → water vapour),
/// 1 mB a tick at 2 FE, from buckets in its first slot (the empty bucket comes out of the last). The
/// chemical goes out to the blocks beside it. Condensentrating (chemical → fluid) needs a mode switch
/// and fluid outputs, which come later.
fn tick_rotary(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let capacity = machine::STORAGE_MULTIPLIER * chem::ROTARY_USAGE;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut fluid = get_tank(&entity.nbt, "fluid", chem::ROTARY_TANK);
    let mut gas = get_tank(&entity.nbt, "chemical", chem::ROTARY_TANK);
    if let Some(Some(bucket)) = entity.items.first().cloned()
        && let Some(f) = bucket_fluid(&bucket.item)
        && chem::rotary_chemical(&f).is_some()
        && fluid.accepts(&f)
        && fluid.amount + 1_000 <= fluid.capacity
        && give_back_bucket(entity)
    {
        fluid.insert(&f, 1_000);
        entity.items[0] = None;
    }
    let mut active = false;
    if let Some(output) = fluid.chemical.as_deref().and_then(chem::rotary_chemical)
        && fluid.amount > 0
        && gas.accepts(output)
        && gas.amount < gas.capacity
        && energy.consume(chem::ROTARY_USAGE)
    {
        fluid.extract(1);
        gas.insert(output, 1);
        active = true;
    }
    push_chemical(game, pos, &mut gas, None, u64::MAX, false);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fluid", &fluid);
    put_tank(&mut entity.nbt, "chemical", &gas);
    entity.data = vec![
        (fluid.amount * 1000 / chem::ROTARY_TANK) as i32,
        1000,
        (gas.amount * 1000 / chem::ROTARY_TANK) as i32,
        1000,
    ];
    Some(active)
}

/// Puts an empty bucket into the last slot, if it has room for one.
fn give_back_bucket(entity: &mut ModBlockEntity) -> bool {
    let out = entity.items.get(2).cloned().flatten();
    match out {
        None => entity.items[2] = Some(stack_of("minecraft:bucket", 1)),
        Some(s) if &*s.item == "minecraft:bucket" && s.count < 16 => {
            entity.items[2] = Some(ItemStack { count: s.count + 1, ..s })
        }
        Some(_) => return false,
    }
    true
}

// --- Transmitter shapes -----------------------------------------------------------------------------

/// The kind of thing a transmitter carries, from its block id: tiers of one kind join each other.
fn pipe_family(name: &str) -> Option<&'static str> {
    let n = name.strip_prefix("mekanism:")?;
    [
        ("_universal_cable", "cable"),
        ("_pressurized_tube", "tube"),
        ("_logistical_transporter", "transporter"),
        ("restrictive_transporter", "transporter"),
        ("diversion_transporter", "transporter"),
        ("_mechanical_pipe", "pipe"),
        ("_thermodynamic_conductor", "conductor"),
    ]
    .into_iter()
    .find_map(|(suffix, family)| n.ends_with(suffix).then_some(family))
}

/// Vanilla blocks with an inventory a transporter feeds and empties.
fn vanilla_container(name: &str) -> bool {
    let Some(n) = name.strip_prefix("minecraft:") else {
        return false;
    };
    matches!(n, "chest" | "trapped_chest" | "barrel" | "furnace" | "blast_furnace" | "smoker" | "hopper" | "dropper" | "dispenser" | "brewing_stand")
        || n.ends_with("shulker_box")
}

/// Whether a transmitter of `family` reaches into the Mekanism block `kind` (Mekanism's
/// `TransmitterType` and the capabilities its neighbour offers).
fn serves(family: &str, kind: Kind) -> bool {
    match family {
        "cable" => !matches!(
            kind,
            Kind::Cable(_) | Kind::Tube(_) | Kind::Transporter(_) | Kind::ChemicalTank(_) | Kind::Evaporation
        ),
        "tube" => matches!(
            kind,
            Kind::Oxidizer
                | Kind::Advanced(_)
                | Kind::ChemicalInfuser
                | Kind::Rotary
                | Kind::Separator
                | Kind::GasGenerator
                | Kind::ChemicalTank(_)
        ),
        // The blocks with item slots (`block_entity_container`).
        "transporter" => matches!(
            kind,
            Kind::Machine(..)
                | Kind::Factory(..)
                | Kind::HeatGenerator
                | Kind::Oxidizer
                | Kind::Separator
                | Kind::Advanced(_)
                | Kind::Evaporation
                | Kind::Rotary
                | Kind::Infuser
                | Kind::EnergyCube(_)
        ),
        _ => false,
    }
}

/// Twice a second a transmitter shows the sides it joins: its block's six direction properties (the
/// mushroom stem template's) are `false` toward a transmitter of its kind or a block it serves. Mekanism
/// renders the same from its connection cache.
fn show_connections(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "shape_ticker");
    put_int(&mut entity.nbt, "shape_ticker", (ticker + 1) % 10);
    if ticker != 0 {
        return;
    }
    let Some(family) = pipe_family(&entity.id).or_else(|| pipe_family(block_name(game, pos))) else {
        return;
    };
    for (side, d) in DIRECTIONS {
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let name = block_name(game, at).to_owned();
        let joined = pipe_family(&name) == Some(family)
            || (family == "transporter" && vanilla_container(&name))
            || game
                .native_with_mod_block_entity(at, |m| kind(&m.id))
                .flatten()
                .is_some_and(|k| serves(family, k));
        game.native_set_block_property(pos, side, if joined { "false" } else { "true" });
    }
}

// --- Logistical transporters ------------------------------------------------------------------------

fn is_transporter(name: &str) -> bool {
    name.strip_prefix("mekanism:").is_some_and(|n| n.ends_with("_logistical_transporter"))
}

/// A logistical transporter, simplified: twice a second it pulls up to its tier's count from a container
/// beside it (a chest, a furnace's output, a machine's output when its face gives out), and sends it
/// at once along the connected transporters into the nearest container that takes it: a chest, a
/// furnace, a hopper or another machine's input, through the side it faces. What fits nowhere goes back.
/// Mekanism moves items visibly along the pipe at the tier's speed and sorts by colour and filters.
fn tick_transporter(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, pull: u32) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 10);
    if ticker != 0 {
        return;
    }
    for (_, d) in DIRECTIONS {
        let source = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let back = [-d[0], -d[1], -d[2]];
        // A Mekanism block's face must give out (side configuration); any container then gives what a
        // hopper there could take (API 24 `native_extract_item`).
        let closed = game
            .native_with_mod_block_entity(source, |m| !sides_of(&m.nbt).get(face_index(back)).gives_out())
            .unwrap_or(false);
        if closed || is_transporter(block_name(game, source)) {
            continue;
        }
        let face = horizon_core::geometry::Direction::ALL[face_index(back)];
        let Some(stack) = game.native_extract_item(source, face, pull) else {
            continue;
        };
        if let Some(left) = route_items(game, pos, source, stack) {
            // What fits nowhere goes back where it came from.
            if let Some(lost) = game.native_insert_item(source, face, left) {
                game.native_insert_item(source, horizon_core::geometry::Direction::Up, lost);
            }
        }
        return;
    }
}

/// Walks the transporters joined to `start`, nearest first, offering `stack` to every container beside
/// them except `source`. Returns what no container took.
fn route_items(game: &mut Game<'_>, start: [i32; 3], source: [i32; 3], stack: ItemStack) -> Option<ItemStack> {
    let mut seen = std::collections::HashSet::from([start]);
    let mut queue = std::collections::VecDeque::from([start]);
    let mut stack = Some(stack);
    while let Some(node) = queue.pop_front() {
        for (_, d) in DIRECTIONS {
            let at = [node[0] + d[0], node[1] + d[1], node[2] + d[2]];
            if at == source || seen.contains(&at) {
                continue;
            }
            if is_transporter(block_name(game, at)) {
                if seen.len() < 512 {
                    seen.insert(at);
                    queue.push_back(at);
                }
                continue;
            }
            let face = horizon_core::geometry::Direction::ALL[face_index([-d[0], -d[1], -d[2]])];
            stack = game.native_insert_item(at, face, stack.take()?);
            if stack.is_none() {
                return None;
            }
        }
    }
    stack
}

// --- Induction Matrix --------------------------------------------------------------------------------

/// A formed matrix: its storage and rate, and its ports, the first of which keeps the energy.
struct FormedMatrix {
    matrix: mekanism_core::matrix::Matrix,
    ports: Vec<[i32; 3]>,
}

/// `MatrixValidator`, simplified: the frame blocks joined to `start` must be exactly the shell of a box
/// 3 to 18 on each side, and everything inside must be cells, providers or air.
fn find_matrix(game: &Game<'_>, start: [i32; 3]) -> Option<FormedMatrix> {
    use mekanism_core::matrix::{is_frame, MAX_SIZE, MIN_SIZE};
    let mut seen = std::collections::HashSet::from([start]);
    let mut todo = vec![start];
    let (mut lo, mut hi) = (start, start);
    while let Some(at) = todo.pop() {
        for k in 0..3 {
            lo[k] = lo[k].min(at[k]);
            hi[k] = hi[k].max(at[k]);
            if hi[k] - lo[k] >= MAX_SIZE {
                return None;
            }
        }
        for (_, d) in DIRECTIONS {
            let next = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
            if !seen.contains(&next) && is_frame(block_name(game, next)) {
                seen.insert(next);
                todo.push(next);
            }
        }
    }
    if (0..3).any(|k| hi[k] - lo[k] + 1 < MIN_SIZE) {
        return None;
    }
    let mut matrix = mekanism_core::matrix::Matrix::default();
    let mut ports = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let at = [x, y, z];
                let shell = x == lo[0] || x == hi[0] || y == lo[1] || y == hi[1] || z == lo[2] || z == hi[2];
                let name = block_name(game, at);
                if shell {
                    if !seen.contains(&at) {
                        return None;
                    }
                    if name == "mekanism:induction_port" {
                        ports.push(at);
                    }
                } else if !matrix.add_inside(name) {
                    return None;
                }
            }
        }
    }
    (matrix.capacity > 0).then_some(FormedMatrix { matrix, ports })
}

/// An Induction Port. Once a second it checks its matrix and writes the matrix's capacity and rate
/// into its NBT. The first port (`master`) holds the energy; others hand what they took in to it. A
/// face the Configurator set to Output gives energy out, up to the matrix's rate.
fn tick_induction_port(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    if ticker == 0 {
        let formed = find_matrix(game, pos);
        let (capacity, rate, master) = match &formed {
            Some(f) => (f.matrix.capacity, f.matrix.rate, f.ports.first().copied().unwrap_or(pos)),
            None => (0, 0, pos),
        };
        put_long(&mut entity.nbt, "capacity", capacity);
        put_long(&mut entity.nbt, "rate", rate);
        put_long(&mut entity.nbt, "master", u64::from(master == pos));
        for (key, v) in ["mx", "my", "mz"].into_iter().zip(master) {
            entity.nbt.insert(key.into(), Tag::Int(v));
        }
    }
    let rate = get_long(&entity.nbt, "rate");
    if rate == 0 {
        return;
    }
    let master = ["mx", "my", "mz"].map(|key| match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::Int(v)) => *v,
        _ => 0,
    });
    let is_master = master == pos;
    let capacity = get_long(&entity.nbt, "capacity");
    if !is_master {
        // Hand this port's buffer to the master.
        let mine = get_long(&entity.nbt, "energy");
        if mine > 0 {
            let taken = game
                .native_with_mod_block_entity(master, |m| {
                    let stored = get_long(&m.nbt, "energy");
                    let n = mine.min(get_long(&m.nbt, "capacity").saturating_sub(stored));
                    put_long(&mut m.nbt, "energy", stored + n);
                    n
                })
                .unwrap_or(0);
            put_long(&mut entity.nbt, "energy", mine - taken);
        }
    }
    let sides = sides_of(&entity.nbt);
    for (i, (_, d)) in DIRECTIONS.into_iter().enumerate() {
        if sides.get(i) != mekanism_core::sides::SideMode::Output {
            continue;
        }
        // Draw up to the rate from the matrix, offer it out of this face, and put back what is left.
        let draw = |stored: u64| stored.min(rate);
        let mut out = EnergyContainer::new(rate, u64::MAX);
        if is_master {
            let stored = get_long(&entity.nbt, "energy").min(capacity);
            out.stored = draw(stored);
            put_long(&mut entity.nbt, "energy", stored - out.stored);
        } else {
            out.stored = game
                .native_with_mod_block_entity(master, |m| {
                    let stored = get_long(&m.nbt, "energy");
                    put_long(&mut m.nbt, "energy", stored - draw(stored));
                    draw(stored)
                })
                .unwrap_or(0);
        }
        if out.stored == 0 {
            continue;
        }
        push_energy(game, pos, &mut out, Some(d), false);
        let left = out.stored;
        if is_master {
            let stored = get_long(&entity.nbt, "energy");
            put_long(&mut entity.nbt, "energy", stored + left);
        } else {
            game.native_with_mod_block_entity(master, |m| {
                let stored = get_long(&m.nbt, "energy");
                put_long(&mut m.nbt, "energy", stored + left);
            });
        }
    }
}

// --- Thermal Evaporation Plant -----------------------------------------------------------------------

const EVAPORATION_PARTS: [&str; 3] = [
    "mekanism:thermal_evaporation_block",
    "mekanism:thermal_evaporation_valve",
    "mekanism:thermal_evaporation_controller",
];

fn block_name<'a>(game: &'a Game<'_>, at: [i32; 3]) -> &'a str {
    let state = game.world.block(at[0], at[1], at[2]);
    game.pack.assets.registry.state(state).map_or("", |s| s.name.as_str())
}

/// A formed tower: its north-west bottom corner and height.
#[derive(Clone, Copy)]
struct Tower {
    origin: [i32; 3],
    height: u32,
}

impl Tower {
    fn perimeter(self, y: i32) -> impl Iterator<Item = [i32; 3]> {
        let [x0, _, z0] = self.origin;
        (0..4).flat_map(move |dx| (0..4).map(move |dz| (dx, dz)))
            .filter(|&(dx, dz)| dx == 0 || dx == 3 || dz == 0 || dz == 3)
            .map(move |(dx, dz)| [x0 + dx, y, z0 + dz])
    }
}

/// `EvaporationValidator`, simplified: a 4x4 floor of tower blocks, then 4x4 rings of them with air
/// inside, 3 to 18 blocks high in all, with the controller in a ring.
fn find_tower(game: &Game<'_>, c: [i32; 3]) -> Option<Tower> {
    let part = |at: [i32; 3]| EVAPORATION_PARTS.contains(&block_name(game, at));
    let mut bottom = c[1];
    while part([c[0], bottom - 1, c[2]]) && c[1] - bottom < evap::MAX_HEIGHT as i32 {
        bottom -= 1;
    }
    for x0 in c[0] - 3..=c[0] {
        for z0 in c[2] - 3..=c[2] {
            let on_ring = c[0] == x0 || c[0] == x0 + 3 || c[2] == z0 || c[2] == z0 + 3;
            if !on_ring {
                continue;
            }
            let floor = (0..4).all(|dx| (0..4).all(|dz| part([x0 + dx, bottom, z0 + dz])));
            if !floor {
                continue;
            }
            let mut tower = Tower { origin: [x0, bottom, z0], height: 1 };
            while tower.height < evap::MAX_HEIGHT {
                let y = bottom + tower.height as i32;
                let ring = tower.perimeter(y).all(part);
                let hollow = (1..3).all(|dx| (1..3).all(|dz| block_name(game, [x0 + dx, y, z0 + dz]) == "minecraft:air"));
                if !(ring && hollow) {
                    break;
                }
                tower.height += 1;
            }
            if tower.height >= evap::MIN_HEIGHT && c[1] > bottom && c[1] < bottom + tower.height as i32 {
                return Some(tower);
            }
        }
    }
    None
}

/// The Thermal Evaporation Plant's controller runs the tower: water from buckets, heat from the
/// Advanced Solar Generators on its top corners, brine into buckets or out of its valves into the
/// separators beside them. The tower is checked again once a second.
fn tick_evaporation(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let ticker = get_int(&entity.nbt, "ticker");
    put_int(&mut entity.nbt, "ticker", (ticker + 1) % 20);
    let tower = if ticker == 0 || get_int(&entity.nbt, "height") == 0 {
        let t = find_tower(game, pos);
        put_int(&mut entity.nbt, "height", t.map_or(0, |t| t.height));
        if let Some(t) = t {
            for (key, v) in ["x0", "y0", "z0"].into_iter().zip(t.origin) {
                entity.nbt.insert(key.into(), Tag::Int(v));
            }
        }
        t
    } else {
        Some(Tower {
            origin: ["x0", "y0", "z0"].map(|key| match entity.nbt.get(&JavaString::from(key)) {
                Some(Tag::Int(v)) => *v,
                _ => 0,
            }),
            height: get_int(&entity.nbt, "height"),
        })
    };
    let Some(tower) = tower else {
        entity.data = vec![0, 1000, 0, 1000];
        return Some(false);
    };
    let mut water = get_tank(&entity.nbt, "water", evap::input_capacity(tower.height));
    let mut brine = get_tank(&entity.nbt, "brine", evap::OUTPUT_TANK);
    if let Some(Some(bucket)) = entity.items.first().cloned()
        && &*bucket.item == "minecraft:water_bucket"
        && water.amount + 1_000 <= water.capacity
        && give_back_bucket(entity)
    {
        water.insert("minecraft:water", 1_000);
        entity.items[0] = None;
    }
    // The top corners: an Advanced Solar Generator that sees the sun heats the tower.
    let [x0, y0, z0] = tower.origin;
    let top = y0 + tower.height as i32;
    let sun = overworld(game) && game.sky_darken() < 4;
    let solars = [[0, 0], [3, 0], [0, 3], [3, 3]]
        .into_iter()
        .map(|[dx, dz]| [x0 + dx, top, z0 + dz])
        .filter(|&at| sun && block_name(game, at) == "mekanismgenerators:advanced_solar_generator" && can_see_sky(game, at))
        .count() as u32;
    let mut temperature = get_double(&entity.nbt, "temperature").unwrap_or(evap::AMBIENT_TEMP);
    temperature = evap::step_temperature(temperature, solars, evap::AMBIENT_TEMP);
    let mut progress = get_int(&entity.nbt, "progress");
    let ops = evap::operations(evap::temp_multiplier(temperature, tower.height), &mut progress);
    let ops = ops
        .min(water.amount / evap::WATER_PER_BRINE)
        .min((brine.capacity - brine.amount) * u64::from(brine.accepts("mekanism:brine")));
    if ops > 0 {
        water.extract(ops * evap::WATER_PER_BRINE);
        brine.insert("mekanism:brine", ops);
    }
    // FluidInventorySlot.drain: an empty bucket in the middle slot fills with a bucket of brine.
    if let Some(Some(bucket)) = entity.items.get(1).cloned()
        && &*bucket.item == "minecraft:bucket"
        && bucket.count == 1
        && brine.amount >= 1_000
    {
        brine.extract(1_000);
        entity.items[1] = Some(stack_of("mekanism:brine_bucket", 1));
    }
    // Valves give brine to the separators beside them.
    for y in y0..top {
        if brine.amount == 0 {
            break;
        }
        let valves: Vec<[i32; 3]> =
            tower.perimeter(y).filter(|&at| block_name(game, at) == "mekanism:thermal_evaporation_valve").collect();
        for valve in valves {
            push_chemical(game, valve, &mut brine, None, u64::MAX, false);
        }
    }
    put_tank(&mut entity.nbt, "water", &water);
    put_tank(&mut entity.nbt, "brine", &brine);
    put_int(&mut entity.nbt, "progress", progress);
    entity.nbt.insert("temperature".into(), Tag::Double(temperature.to_bits()));
    // The flame shows the temperature up to 3,000 K, the arrow the brine tank.
    let heat = (temperature - evap::AMBIENT_TEMP) / (evap::MAX_MULTIPLIER_TEMP - evap::AMBIENT_TEMP);
    entity.data = vec![
        (heat.clamp(0.0, 1.0) * 1000.0) as i32,
        1000,
        (brine.amount * 1000 / evap::OUTPUT_TANK) as i32,
        1000,
    ];
    Some(ops > 0)
}

fn tick_gas_generator(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> Option<bool> {
    let mut energy = EnergyContainer::new(chem::GAS_GENERATOR_STORAGE, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(chem::GAS_GENERATOR_STORAGE);
    let mut fuel = get_tank(&entity.nbt, "fuel", chem::GAS_GENERATOR_TANK);
    let (made, _) = chem::burn_fuel(&mut fuel, energy.capacity - energy.stored);
    energy.insert(made, Action::Execute);
    charge_slot(entity, 0, &mut energy);
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
        push_chemical(game, pos, &mut tank, Some([0, -1, 0]), rate, false);
        put_tank(&mut entity.nbt, "chemical", &tank);
    }
}

/// A pressurized tube feeds the tanks, machines and generators beside it, then lets the rest run on
/// to emptier tubes carrying the same chemical, as `tick_cable` does with energy.
fn tick_tube(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, rate: u64) {
    let mut tank = get_tank(&entity.nbt, "chemical", rate);
    if tank.amount == 0 {
        return;
    }
    push_chemical(game, pos, &mut tank, None, u64::MAX, true);
    for (_, d) in DIRECTIONS {
        let Some(chemical) = tank.chemical.clone() else {
            break;
        };
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let mine = tank.amount;
        let given = game.native_with_mod_block_entity(at, |m| {
            let Some(Kind::Tube(their_rate)) = kind(&m.id) else {
                return 0;
            };
            let mut theirs = get_tank(&m.nbt, "chemical", their_rate);
            if !theirs.accepts(&chemical) {
                return 0;
            }
            let give = tier::cable_share(mine, theirs.amount);
            let n = theirs.insert(&chemical, give);
            put_tank(&mut m.nbt, "chemical", &theirs);
            n
        });
        if let Some(n) = given {
            tank.extract(n);
        }
    }
    put_tank(&mut entity.nbt, "chemical", &tank);
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
        if offer == 0 || !gives_out(pos, *d) {
            continue;
        }
        let at = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let back = [-d[0], -d[1], -d[2]];
        let accepted = game
            .native_block_property(at, "facing")
            .and_then(|f| direction(&f))
            .map_or(true, |front| front != back);
        let taken = game.native_with_mod_block_entity(at, |m| {
            if !takes_in(&m.nbt, back) {
                return 0;
            }
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
        Kind::Advanced(m) => Some((m.usage * u64::from(chem::ADVANCED_TICKS), u64::MAX)),
        Kind::ChemicalInfuser => Some((machine::STORAGE_MULTIPLIER * chem::CHEMICAL_INFUSER_USAGE, u64::MAX)),
        Kind::Rotary => Some((machine::STORAGE_MULTIPLIER * chem::ROTARY_USAGE, u64::MAX)),
        // The matrix's first port holds its energy; the others take a tick's worth and pass it on.
        Kind::InductionPort if get_long(nbt, "rate") > 0 => {
            let rate = get_long(nbt, "rate");
            Some(if get_long(nbt, "master") == 1 { (get_long(nbt, "capacity"), rate) } else { (rate, rate) })
        }
        Kind::Infuser => Some((chem::INFUSER_USAGE * u64::from(chem::INFUSER_TICKS), u64::MAX)),
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

/// Sets the energy an item holds, keeping its other components and other custom data.
fn set_energy(stack: &mut ItemStack, energy: u64) {
    let extra = stack.extra.get_or_insert_with(Default::default);
    let at = extra.other.iter().position(|(k, d)| k == "minecraft:custom_data" && matches!(d, Tag::Compound(_)));
    let at = at.unwrap_or_else(|| {
        extra.other.retain(|(k, _)| k != "minecraft:custom_data");
        extra.other.push(("minecraft:custom_data".to_owned(), Tag::Compound(Compound::new())));
        extra.other.len() - 1
    });
    if let Tag::Compound(c) = &mut extra.other[at].1 {
        put_long(c, ENERGY_KEY, energy);
    }
}

/// The item in `entity`'s slot `index` and its energy container, when it holds energy.
fn energy_item(entity: &ModBlockEntity, index: usize) -> Option<(gear::ItemEnergy, u64)> {
    let stack = entity.items.get(index)?.as_ref()?;
    let item = gear::item_energy(&stack.item)?;
    Some((item, stored_energy(stack).min(item.capacity)))
}

fn put_item_energy(entity: &mut ModBlockEntity, index: usize, before: u64, after: u64) {
    if after != before
        && let Some(Some(stack)) = entity.items.get_mut(index)
    {
        set_energy(stack, after);
    }
}

/// `EnergyInventorySlot.charge`: fills the item in slot `index` from `source`.
fn charge_slot(entity: &mut ModBlockEntity, index: usize, source: &mut EnergyContainer) {
    if let Some((item, stored)) = energy_item(entity, index) {
        let after = gear::charge(item, stored, source);
        put_item_energy(entity, index, stored, after);
    }
}

/// `EnergyInventorySlot.discharge`: empties the item in slot `index` into `target`.
fn discharge_slot(entity: &mut ModBlockEntity, index: usize, target: &mut EnergyContainer) {
    if let Some((item, stored)) = energy_item(entity, index) {
        let after = gear::discharge(item, stored, target);
        put_item_energy(entity, index, stored, after);
    }
}

// --- Atomic Disassembler -----------------------------------------------------------------------------

const DISASSEMBLER: &str = "mekanism:atomic_disassembler";

/// The key in the disassembler's custom data that holds its mode (Mekanism's `disassembler_mode`).
const MODE_KEY: &str = "mekanism:disassembler_mode";

fn disassembler_mode(stack: &ItemStack) -> gear::DisassemblerMode {
    let name = custom_data(stack).and_then(|c| match c.get(&JavaString::from(MODE_KEY)) {
        Some(Tag::String(s)) => s.to_string().ok(),
        _ => None,
    });
    name.as_deref().and_then(gear::DisassemblerMode::from_name).unwrap_or_default()
}

fn set_disassembler_mode(stack: &mut ItemStack, mode: gear::DisassemblerMode) {
    let stored = stored_energy(stack);
    set_energy(stack, stored);
    if let Some(Tag::Compound(c)) = stack
        .extra
        .as_mut()
        .and_then(|e| e.other.iter_mut().find(|(k, _)| k == "minecraft:custom_data"))
        .map(|(_, d)| d)
    {
        c.insert(MODE_KEY.into(), Tag::String(mode.name().into()));
    }
}

/// `ItemAtomicDisassembler.getDestroySpeed` (hook point `native_destroy_speed`, API 25): its mode's
/// efficiency while it holds the energy for the block, slower as it runs out, not at all when empty.
pub(super) fn destroy_speed(game: &Game<'_>, stack: Option<&ItemStack>, hardness: f32, speed: &mut f32) {
    let Some(stack) = stack.filter(|s| &*s.item == DISASSEMBLER) else {
        return;
    };
    if game.native_on(MOD_ID) {
        *speed = gear::destroy_speed(disassembler_mode(stack), stored_energy(stack), hardness);
    }
}

/// Sneak + right click with the disassembler cycles its mode (Mekanism's mode key), with the new mode on
/// the action bar. Hook point `native_use_item`, API 13.
pub(super) fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if *claimed || &*stack.item != DISASSEMBLER || !game.native_on(MOD_ID) {
        return;
    }
    if !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
        return;
    }
    let Some(held) = game.held_mut().as_mut().filter(|s| &*s.item == DISASSEMBLER) else {
        return;
    };
    let mode = disassembler_mode(held).next();
    set_disassembler_mode(held, mode);
    *claimed = true;
    if let Some(player) = game.native_acting_player() {
        let line = format!("Mode changed to: {} ({})", mode.label(), mode.efficiency());
        game.native_action_bar(player.uuid, horizon_client::text::Text::literal(line));
    }
}

/// `ItemAtomicDisassembler.mineBlock`: breaking a block spends the disassembler's energy for it.
pub(super) fn spend_disassembler_energy(game: &mut Game<'_>, pos: [i32; 3]) {
    if !game.native_on(MOD_ID) || game.creative {
        return;
    }
    let state = game.world.block(pos[0], pos[1], pos[2]);
    let hardness = game.res.facts().get(state).hardness;
    if let Some(stack) = game.held_mut().as_mut().filter(|s| &*s.item == DISASSEMBLER) {
        let stored = stored_energy(stack);
        let cost = gear::destroy_energy(disassembler_mode(stack), hardness);
        set_energy(stack, stored.saturating_sub(cost));
    }
}

/// Breaking an energy cube that holds energy outside creative: the block goes as `Level.destroyBlock`
/// takes it (particles, sound, no loot) and drops the cube with its energy, as Mekanism's loot copies
/// the energy component. True when it did, so the game's own break is canceled.
pub(super) fn break_keeping_energy(game: &mut Game<'_>, pos: [i32; 3]) -> bool {
    if !game.native_on(MOD_ID) || game.creative {
        return false;
    }
    let held = game.native_with_mod_block_entity(pos, |m| match kind(&m.id) {
        Some(Kind::EnergyCube(_)) => Some((m.id.clone(), get_long(&m.nbt, "energy"), m.items.clone())),
        _ => None,
    });
    let Some(Some((id, energy, items))) = held else {
        return false;
    };
    if energy == 0 {
        return false;
    }
    let at = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
    // The items in its charge and discharge slots drop beside it.
    let mut drops = items;
    drops.push(Some(with_energy(&id, energy)));
    game.with_level(|b, level, ticks| {
        b.destroy_block(level, ticks, at, false);
        level.drop_lost_contents(at, drops);
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
    if &*stack.item == DISASSEMBLER {
        let mode = disassembler_mode(stack);
        let line = format!("Mode: {} (efficiency {})", mode.label(), mode.efficiency());
        lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0xAA, 0xAA, 0xAA, 0xFF)));
    }
    let energy = stored_energy(stack);
    if energy == 0 {
        return;
    }
    let line = format!("Stored Energy: {}", mekanism_core::energy::format_fe(energy));
    lines.push((horizon_client::text::Text::literal(line), horizon_client::hud::rgba(0x55, 0xFF, 0xFF, 0xFF)));
}
