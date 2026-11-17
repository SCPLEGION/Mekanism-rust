//! The machines that turn an item and a chemical into an item or a chemical (`TileEntityAdvancedElectricMachine`,
//! the metallurgic infuser, painting machine, antiprotonic nucleosynthesizer, chemical dissolution chamber,
//! crystallizer, oxidizer and pigment extractor). Ports of `tick_advanced`, `tick_infuser`, `tick_painting`,
//! `tick_nucleosynthesizer`, `tick_dissolution`, `tick_crystallizer` and `tick_oxidizer` of `runtime.rs`.
use super::chem::{
    chemical_need, convert_item_into_tank, drain_tank_into_item, fill_tank_from_item, get_tank, machine_power, push_chemical,
    put_tank, stack_of, usage_const,
};
use super::common::{MenuData, discharge_slot, max_stack, set_active};
use super::multiblock::chemical_index;
use super::machine::roll;
use super::nbt::{get_int, get_long, put_int, put_long};
use mekanism_core::chemical as chem;
use mekanism_core::energy::EnergyContainer;
use mekanism_core::nuclear;
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    ItemStack, ModBlockEntityView, ModContainer, Registrar, ServerHost,
};

/// The id of the block entity type of these machines (`mekanism:item_chemical_machine`).
pub(crate) const TYPE: &str = "item_chemical_machine";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Advanced(&'static chem::AdvancedMachine),
    Infuser,
    Painting,
    Nucleosynthesizer,
    Dissolution,
    Crystallizer,
    Oxidizer,
    PigmentExtractor,
}

fn kind(id: &str) -> Option<Kind> {
    Some(match id {
        "mekanism:metallurgic_infuser" => Kind::Infuser,
        "mekanism:painting_machine" => Kind::Painting,
        "mekanism:antiprotonic_nucleosynthesizer" => Kind::Nucleosynthesizer,
        "mekanism:chemical_dissolution_chamber" => Kind::Dissolution,
        "mekanism:chemical_crystallizer" => Kind::Crystallizer,
        "mekanism:chemical_oxidizer" => Kind::Oxidizer,
        "mekanism:pigment_extractor" => Kind::PigmentExtractor,
        _ => Kind::Advanced(chem::ADVANCED_MACHINES.iter().find(|m| m.block == id)?),
    })
}

fn all_blocks() -> Vec<&'static str> {
    let mut v = vec![
        "mekanism:metallurgic_infuser",
        "mekanism:painting_machine",
        "mekanism:antiprotonic_nucleosynthesizer",
        "mekanism:chemical_dissolution_chamber",
        "mekanism:chemical_crystallizer",
        "mekanism:chemical_oxidizer",
        "mekanism:pigment_extractor",
    ];
    v.extend(chem::ADVANCED_MACHINES.iter().map(|m| m.block));
    v
}

pub(crate) fn register(r: &mut dyn Registrar) {
    let blocks = all_blocks();
    let entity = r.block_entity_type(TYPE).blocks(&blocks).build();
    r.block_entity_part(&entity, ItemChemicalPart).expect("native block entity type");
    for id in blocks {
        let block = r.block_key(id);
        r.block_container(&block, container(kind(id).expect("listed")));
    }
}

/// A hopper's five slots: the machine's roles in the furnace's three, and its extra slots after them.
fn container(kind: Kind) -> ModContainer {
    // (inputs, outputs, side slots, energy-only extras)
    let (inputs, outputs, side): (Vec<usize>, Vec<usize>, Vec<usize>) = match kind {
        // The energy slot (3) comes after the furnace's three.
        Kind::Advanced(_) | Kind::Infuser | Kind::Painting | Kind::Nucleosynthesizer => {
            (vec![0, 1, 3], vec![2], vec![1, 3])
        }
        // The item in the first slot, the chemical item (3) and energy slot (4).
        Kind::Dissolution => (vec![0, 1, 3, 4], vec![], vec![1, 3, 4]),
        Kind::Crystallizer => (vec![1, 3], vec![2], vec![1, 3]),
        Kind::Oxidizer | Kind::PigmentExtractor => (vec![0, 1, 3], vec![], vec![1, 3]),
    };
    ModContainer {
        slots: 5,
        faces: [vec![2], vec![0], side.clone(), side.clone(), side.clone(), side],
        inputs,
        outputs,
        menu: Some("minecraft:hopper".into()),
    }
}

struct ItemChemicalPart;

#[mod_api::horizon_part]
impl BlockPart for ItemChemicalPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for ItemChemicalPart {
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
            Kind::Advanced(m) => tick_advanced(host, &mut event.entity, m),
            Kind::Infuser => tick_infuser(host, &mut event.entity),
            Kind::Painting => tick_painting(host, &mut event.entity),
            Kind::Nucleosynthesizer => tick_nucleosynthesizer(host, &mut event.entity),
            Kind::Dissolution => tick_dissolution(host, pos, &mut event.entity),
            Kind::Crystallizer => tick_crystallizer(host, &mut event.entity),
            Kind::Oxidizer => tick_oxidizer(host, pos, &mut event.entity, false),
            Kind::PigmentExtractor => tick_oxidizer(host, pos, &mut event.entity, true),
        };
        set_active(host, pos, active);
    }
}

fn advanced_usage(m: &chem::AdvancedMachine) -> u64 {
    // The block's usage is the table's; UsageConfig overrides it once the config is read.
    m.usage
}

fn in_tag<'a>(host: &'a dyn Host) -> impl Fn(&str, &str) -> bool + 'a {
    move |tag: &str, item: &str| super::in_tag(host, item, tag.trim_start_matches('#'))
}

/// The stack in item slot `index`.
fn stack_at(entity: &ModBlockEntityView<'_>, index: usize) -> Option<ItemStack> {
    entity.items.get(index).and_then(Option::as_ref).cloned()
}

/// Whether `out` takes `count` more of `output` (`max` is its stack limit).
fn fits(host: &dyn Host, out: Option<&ItemStack>, output: &str, count: u32, max: u32) -> bool {
    out.is_none_or(|o| super::item_id(host, o) == output && o.count() + count <= max)
}

/// Takes `take` of the input stack and adds `count` of `output` to the output slot.
fn finish_item(
    host: &dyn Host,
    entity: &mut ModBlockEntityView<'_>,
    input: ItemStack,
    take: u32,
    out: Option<ItemStack>,
    output: &str,
    count: u32,
) {
    entity.items[0] = input.shrink(take);
    entity.items[2] = match out {
        Some(mut o) => {
            let _ = o.grow(count);
            Some(o)
        }
        None => stack_of(host, output, count),
    };
}

/// The menu data (`MenuData`): the furnace's flame and arrow, the energy and one gauge for each of `tanks`, in the
/// order of the screen's gauges. `flame` is thousandths.
fn publish(
    entity: &mut ModBlockEntityView<'_>,
    energy: &EnergyContainer,
    usage: u64,
    flame: u64,
    progress: u32,
    ticks: u32,
    tanks: &[&mekanism_core::chemical::ChemicalTank],
) {
    let u = super::common::upgrades(&entity.nbt);
    let mut data = MenuData::new(energy.stored, energy.capacity, usage)
        .furnace([flame as i32, 1000, progress as i32, ticks as i32])
        .upgrades(&u);
    for t in tanks {
        data = data.gauge(t.amount, t.capacity, chemical_index(t.chemical.as_deref()));
    }
    data.write(entity);
}

/// An advanced electric machine: the Purification Chamber (ore + 1 mB oxygen a tick → 3 clumps), the Osmium
/// Compressor (refined obsidian or glowstone dust + osmium → ingot) or the Chemical Injection Chamber, 200 ticks.
/// Its chemical comes from items in the middle slot (`chemical_conversion`: flint → oxygen, osmium ingot → osmium).
fn tick_advanced(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>, m: &'static chem::AdvancedMachine) -> bool {
    let host: &dyn Host = &*host;
    let (u, usage, ticks, capacity) = machine_power(&entity.nbt, advanced_usage(m), Some(chem::ADVANCED_TICKS));
    let chemical_upgrades = matches!(m.block, "mekanism:purification_chamber" | "mekanism:chemical_injection_chamber");
    let statistical = usage_const("randomizedConsumption", 1) != 0 && chemical_upgrades;
    let mut used = get_int(&entity.nbt, "used");
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    discharge_slot(host, entity, 3, &mut energy);
    let mut tank = get_tank(&entity.nbt, "chemical", chem::ADVANCED_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let need = chemical_need(&u, chem::ADVANCED_TICKS, ticks, progress, used, statistical, chemical_upgrades, roll(&mut entity.nbt));
    let mut active = false;
    let tags = in_tag(host);
    convert_item_into_tank(host, entity, 1, &mut tank, |c| m.chemicals.contains(&c));
    let recipe = stack_at(entity, 0).and_then(|stack| {
        // With a chemical in the tank, only its recipes (the injection chamber takes two chemicals).
        let held = tank.chemical.as_deref();
        let id = super::item_id(host, &stack);
        m.table
            .iter()
            .find(|r| held.is_none_or(|c| c == r.chemical) && stack.count() >= r.input_count && r.input.matches(&id, &tags))
            .map(|r| (stack, *r))
    });
    let out = stack_at(entity, 2);
    let warn = match (&recipe, entity.items.first().is_some_and(Option::is_some)) {
        (None, true) => 8,
        (Some((_, r)), _) => {
            let ok = fits(host, out.as_ref(), r.output, r.output_count, max_stack(host, r.output));
            u32::from(!(tank.chemical.as_deref() == Some(r.chemical) && tank.amount >= need.max(1))) << 1
                | u32::from(!ok) << 2
        }
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r)) => {
            let ok = fits(host, out.as_ref(), r.output, r.output_count, max_stack(host, r.output));
            if ok && tank.chemical.as_deref() == Some(r.chemical) && tank.amount >= need && energy.consume(usage) {
                active = true;
                tank.extract(need);
                used += need as u32;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    used = 0;
                    finish_item(host, entity, stack, r.input_count, out, r.output, r.output_count);
                }
            }
        }
        None => {
            progress = 0;
            used = 0;
        }
    }
    put_int(&mut entity.nbt, "used", used);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "chemical", &tank);
    publish(entity, &energy, usage, energy.stored * 1000 / capacity, progress, ticks, &[&tank]);
    active
}

/// The Antiprotonic Nucleosynthesizer: antimatter pellets in the middle slot fill its tank (`chemical_conversion`),
/// and an item with antimatter turns into another over the recipe's duration, 15 FE a tick, the antimatter used when
/// the operation finishes.
fn tick_nucleosynthesizer(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>) -> bool {
    let host: &dyn Host = &*host;
    let usage = usage_const("antiprotonicNucleosynthesizer", chem::NUCLEOSYNTHESIZER_USAGE);
    let capacity = mekanism_core::machine::STORAGE_MULTIPLIER * usage;
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    discharge_slot(host, entity, 3, &mut energy);
    let mut tank = get_tank(&entity.nbt, "chemical", chem::NUCLEOSYNTHESIZER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut ticks = 0;
    let mut active = false;
    let tags = in_tag(host);
    convert_item_into_tank(host, entity, 1, &mut tank, |c| c == chem::ANTIMATTER);
    let recipe = stack_at(entity, 0).and_then(|stack| {
        let id = super::item_id(host, &stack);
        mekanism_core::recipe_table::NUCLEOSYNTHESIZING
            .iter()
            .find(|r| stack.count() >= r.recipe.input_count && r.recipe.input.matches(&id, &tags))
            .map(|r| (stack, *r))
    });
    let out = stack_at(entity, 2);
    match recipe {
        Some((stack, n)) => {
            let r = n.recipe;
            ticks = n.duration;
            let ok = fits(host, out.as_ref(), r.output, r.output_count, max_stack(host, r.output));
            if ok && tank.chemical.as_deref() == Some(r.chemical) && tank.amount >= r.chemical_amount && energy.consume(usage) {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    tank.extract(r.chemical_amount);
                    finish_item(host, entity, stack, r.input_count, out, r.output, r.output_count);
                }
            }
        }
        None => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "chemical", &tank);
    publish(entity, &energy, usage, energy.stored * 1000 / capacity, progress, ticks, &[&tank]);
    active
}

/// The Painting Machine: an item and pigment make a dyed item over 200 ticks. Pigment tank items in slot 1 fill it.
fn tick_painting(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>) -> bool {
    let host: &dyn Host = &*host;
    let (_, usage, ticks, capacity) =
        machine_power(&entity.nbt, usage_const("paintingMachine", chem::PAINTING_USAGE), Some(chem::PAINTING_TICKS));
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    discharge_slot(host, entity, 3, &mut energy);
    let mut tank = get_tank(&entity.nbt, "pigment", chem::PAINTING_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let tags = in_tag(host);
    fill_tank_from_item(host, entity, 1, &mut tank, mekanism_core::recipes::painting_pigment);
    let recipe = match (stack_at(entity, 0), tank.chemical.clone()) {
        (Some(stack), Some(pigment)) => {
            let id = super::item_id(host, &stack);
            mekanism_core::recipes::find_painting(&id, stack.count(), &pigment, &tags).map(|r| (stack, *r))
        }
        _ => None,
    };
    let out = stack_at(entity, 2);
    let has_input = entity.items.first().is_some_and(Option::is_some);
    let warn = match &recipe {
        None if has_input && tank.chemical.is_none() => 2,
        None if has_input => 8,
        Some((_, r)) => {
            let ok = fits(host, out.as_ref(), r.output, r.output_count, max_stack(host, r.output));
            u32::from(tank.amount < r.chemical_amount) << 1 | u32::from(!ok) << 2
        }
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r)) if tank.amount >= r.chemical_amount => {
            let ok = fits(host, out.as_ref(), r.output, r.output_count, max_stack(host, r.output));
            if ok && energy.consume(usage) {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    tank.extract(r.chemical_amount);
                    finish_item(host, entity, stack, r.input_count, out, r.output, r.output_count);
                }
            }
        }
        _ => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "pigment", &tank);
    publish(entity, &energy, usage, energy.stored * 1000 / capacity, progress, ticks, &[&tank]);
    active
}

/// The Metallurgic Infuser: the middle slot's item (coal, redstone, diamond dust...) turns into infuse chemical in a
/// 1,000 mB tank (`chemical_conversion`), and an item with enough of it becomes the recipe's output after 200 ticks
/// at 5 FE a tick (iron + 10 carbon → enriched iron).
fn tick_infuser(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>) -> bool {
    let host: &dyn Host = &*host;
    let (_, usage, ticks, capacity) =
        machine_power(&entity.nbt, chem::INFUSER_USAGE, Some(chem::INFUSER_TICKS));
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    discharge_slot(host, entity, 3, &mut energy);
    let mut tank = get_tank(&entity.nbt, "infuse", chem::INFUSER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let tags = in_tag(host);
    convert_item_into_tank(host, entity, 1, &mut tank, |_| true);
    let recipe = match (stack_at(entity, 0), tank.chemical.clone()) {
        (Some(stack), Some(chemical)) => {
            let id = super::item_id(host, &stack);
            mekanism_core::recipes::find_infusing(&id, stack.count(), &chemical, &tags).map(|r| (stack, *r))
        }
        _ => None,
    };
    let out = stack_at(entity, 2);
    let warn = match (&recipe, entity.items.first().is_some_and(Option::is_some)) {
        (None, true) if tank.chemical.is_none() => 2,
        (None, true) => 8,
        (Some((_, r)), _) => {
            let ok = fits(host, out.as_ref(), r.output, r.output_count, max_stack(host, r.output));
            u32::from(tank.amount < r.chemical_amount) << 1 | u32::from(!ok) << 2
        }
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r)) if tank.amount >= r.chemical_amount => {
            let ok = fits(host, out.as_ref(), r.output, r.output_count, max_stack(host, r.output));
            if ok && energy.consume(usage) {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    tank.extract(r.chemical_amount);
                    finish_item(host, entity, stack, r.input_count, out, r.output, r.output_count);
                }
            }
        }
        _ => progress = 0,
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "infuse", &tank);
    // The flame shows the infuse tank, the arrow the progress.
    publish(entity, &energy, usage, tank.amount * 1000 / chem::INFUSER_TANK, progress, ticks, &[&tank]);
    active
}

/// The Chemical Dissolution Chamber: an item in the first slot and `per_tick` of the chemical piped in, every tick
/// for 100 ticks, make the output chemical, pushed out to the tanks beside it.
fn tick_dissolution(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let (u, usage, ticks, capacity) = machine_power(
        &entity.nbt,
        usage_const("chemicalDissolutionChamber", nuclear::DISSOLUTION_USAGE),
        Some(nuclear::DISSOLUTION_TICKS),
    );
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut input = get_tank(&entity.nbt, "input", nuclear::DISSOLUTION_TANK);
    let mut output = get_tank(&entity.nbt, "output", nuclear::DISSOLUTION_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut used = get_int(&entity.nbt, "used");
    let need = chemical_need(
        &u,
        nuclear::DISSOLUTION_TICKS,
        ticks,
        progress,
        used,
        usage_const("randomizedConsumption", 1) != 0,
        true,
        roll(&mut entity.nbt),
    );
    let mut active = false;
    {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 1, &mut energy);
        fill_tank_from_item(h, entity, 3, &mut input, nuclear::dissolves_with);
        drain_tank_into_item(h, entity, 4, &mut output);
    }
    let recipe = {
        let h: &dyn Host = &*host;
        let tags = in_tag(h);
        stack_at(entity, 0).and_then(|stack| {
            let id = super::item_id(h, &stack);
            nuclear::find_dissolution(&id, stack.count(), &tags).map(|r| (stack, *r))
        })
    };
    let warn = match &recipe {
        None if entity.items.first().is_some_and(Option::is_some) => 8,
        Some((_, r)) => {
            u32::from(!(input.chemical.as_deref() == Some(r.chemical) && input.amount >= need)) << 1
                | u32::from(!(output.accepts(r.output) && output.amount + r.amount <= output.capacity)) << 2
        }
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r))
            if input.chemical.as_deref() == Some(r.chemical)
                && input.amount >= need
                && output.accepts(r.output)
                && output.amount + r.amount <= output.capacity =>
        {
            if energy.consume(usage) {
                active = true;
                input.extract(need);
                used += need as u32;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    used = 0;
                    output.insert(r.output, r.amount);
                    entity.items[0] = stack.shrink(r.count);
                }
            }
        }
        Some(_) => {}
        None => {
            progress = 0;
            used = 0;
        }
    }
    push_chemical(host, pos, &mut output, u64::MAX);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_int(&mut entity.nbt, "used", used);
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "output", &output);
    publish(entity, &energy, usage, energy.stored * 1000 / capacity, progress, ticks, &[&input, &output]);
    active
}

/// The Chemical Crystallizer: a chemical into an item.
fn tick_crystallizer(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>) -> bool {
    let host: &dyn Host = &*host;
    let (_, usage, ticks, capacity) = machine_power(
        &entity.nbt,
        usage_const("chemicalCrystallizer", nuclear::CRYSTALLIZER_USAGE),
        Some(nuclear::CRYSTALLIZER_TICKS),
    );
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    discharge_slot(host, entity, 1, &mut energy);
    let mut input = get_tank(&entity.nbt, "input", nuclear::CRYSTALLIZER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    fill_tank_from_item(host, entity, 3, &mut input, |c| nuclear::find_crystallizing(c).is_some());
    let out = stack_at(entity, 2);
    match input.chemical.as_deref().and_then(nuclear::find_crystallizing) {
        Some(r) if input.amount >= r.amount && fits(host, out.as_ref(), r.output, r.count, 64) => {
            if energy.consume(usage) {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    input.extract(r.amount);
                    let count = out.as_ref().map_or(0, ItemStack::count) + r.count;
                    entity.items[2] = stack_of(host, r.output, count);
                }
            }
        }
        _ => progress = 0,
    }
    let room = stack_at(entity, 2);
    let warn = match input.chemical.as_deref().map(nuclear::find_crystallizing) {
        Some(None) => 8,
        Some(Some(r)) => {
            u32::from(input.amount < r.amount) << 1
                | u32::from(!fits(host, room.as_ref(), r.output, r.count, 64)) << 2
        }
        None => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "input", &input);
    publish(entity, &energy, usage, energy.stored * 1000 / capacity, progress, ticks, &[&input]);
    active
}

/// The Chemical Oxidizer and the Pigment Extractor: an item becomes a chemical over 100 ticks, pushed out to the
/// tanks beside it.
fn tick_oxidizer(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>, pigment: bool) -> bool {
    let (usage, ticks, tank_size) = if pigment {
        (usage_const("pigmentExtractor", chem::PIGMENT_EXTRACTOR_USAGE), chem::PIGMENT_EXTRACTOR_TICKS, chem::PIGMENT_EXTRACTOR_TANK)
    } else {
        (usage_const("chemicalOxidizer", chem::OXIDIZER_USAGE), chem::OXIDIZER_TICKS, chem::OXIDIZER_TANK)
    };
    let (_, usage, ticks, capacity) = machine_power(&entity.nbt, usage, Some(ticks));
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    let mut tank = get_tank(&entity.nbt, "output", tank_size);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let recipe = {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 1, &mut energy);
        drain_tank_into_item(h, entity, 3, &mut tank);
        let tags = in_tag(h);
        stack_at(entity, 0).and_then(|stack| {
            let id = super::item_id(h, &stack);
            if pigment {
                mekanism_core::recipes::find_pigment_extracting(&id, stack.count(), &tags)
            } else {
                mekanism_core::recipes::find_oxidizing(&id, stack.count(), &tags)
            }
            .map(|r| (stack, *r))
        })
    };
    let warn = match &recipe {
        None if entity.items.first().is_some_and(Option::is_some) => 8,
        Some((_, r)) if !(tank.accepts(r.output) && tank.amount + r.amount <= tank.capacity) => 4,
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    match recipe {
        Some((stack, r)) if tank.accepts(r.output) && tank.amount + r.amount <= tank.capacity => {
            if energy.consume(usage) {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    tank.insert(r.output, r.amount);
                    entity.items[0] = stack.shrink(r.input_count);
                }
            }
        }
        _ => progress = 0,
    }
    push_chemical(host, pos, &mut tank, u64::MAX);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "output", &tank);
    // The pigment arrow's colour hint (data[1], `PigmentColorDetails.getColorTo`) is the screen's, sent with it.
    publish(entity, &energy, usage, energy.stored * 1000 / capacity, progress, ticks, &[&tank]);
    active
}
