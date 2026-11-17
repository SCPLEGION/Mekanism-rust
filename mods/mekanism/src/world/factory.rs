//! The factories (`TileEntityFactory`): an item factory runs 3, 5 or 7 or 9 processes side by side, a chemical
//! factory the same with a shared chemical tank. Port of `tick_factory`, `tick_chem_factory` and
//! `sort_factory_inputs` of `runtime.rs`.
use super::chem::{chemical_need, get_tank, put_tank, usage_const};
use super::common::{discharge_slot, key_of, max_stack, set_slot, slot, upgrades};
use super::machine::{find_operation, roll};
use super::nbt::{get_int, get_ints, get_long, put_int, put_long};
use mekanism_core::chemical as chem;
use mekanism_core::factory::{self, Factory};
use mekanism_core::machine::{Operation, Slot};
use mekanism_core::upgrades::Upgrade;
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockPart, Host, ItemStack, ModBlockEntityView,
    ModContainer, Registrar, ServerHost, Tag,
};

/// The id of the block entity type of the factories (`mekanism:factory`).
pub(crate) const TYPE: &str = "factory";
/// The slots in a row of the chest menu a factory shows.
const ROW: usize = 9;
/// A factory's energy slot (`TileEntityFactory.energySlot`), after its two rows.
const ENERGY: usize = 2 * ROW;
/// The chemical slot of the chemical factories, or the combiner factory's extra input; the sawing factory's
/// secondary outputs start here instead.
const CHEM: usize = ENERGY + 1;

fn blocks() -> Vec<String> {
    let mut all = vec![];
    for (tier, _) in factory::TIERS {
        for (kind, ..) in factory::KINDS {
            all.push(format!("mekanism:{tier}_{kind}_factory"));
        }
    }
    all
}

pub(crate) fn register(r: &mut dyn Registrar) {
    let ids = blocks();
    let ids: Vec<&str> = ids.iter().map(String::as_str).filter(|id| factory::factory(id).is_some()).collect();
    let kind = r.block_entity_type(TYPE).blocks(&ids).build();
    r.block_entity_part(&kind, FactoryPart).expect("native block entity type");
    for id in ids {
        let (n, recipe, _) = factory::factory(id).expect("filtered");
        let block = r.block_key(id);
        r.block_container(&block, container(n, recipe));
    }
}

/// How many slots a factory has: its two rows, the energy slot, and its extra slot or secondary outputs.
fn slot_count(n: usize, recipe: &str) -> usize {
    match recipe {
        "sawing" => CHEM + n,
        r if factory::has_extra_slot(r) => CHEM + 1,
        _ => CHEM,
    }
}

/// A factory: its inputs on the top row of a two-row chest menu, each process's output below it.
fn container(n: usize, recipe: &str) -> ModContainer {
    let inputs: Vec<usize> = (0..n).collect();
    let outputs: Vec<usize> = (ROW..ROW + n).collect();
    ModContainer {
        slots: slot_count(n, recipe),
        faces: [outputs.clone(), inputs.clone(), inputs.clone(), inputs.clone(), inputs.clone(), inputs.clone()],
        inputs,
        outputs,
        menu: Some("minecraft:generic_9x3".into()),
    }
}

struct FactoryPart;

#[mod_api::horizon_part]
impl BlockPart for FactoryPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for FactoryPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some((n, recipe, usage)) = factory::factory(event.entity.id()) else {
            return;
        };
        let active = if factory::takes_chemical(recipe) {
            tick_chem(host, &mut event.entity, n, recipe, usage)
        } else {
            Some(tick_items(host, &mut event.entity, n, recipe, usage))
        };
        if let Some(active) = active {
            super::common::set_active(host, event.pos, active);
        }
    }

    fn block_entity_update_tag(&self, event: &mut mod_api::BlockEntityUpdateTagEvent<'_>) -> Option<mod_api::Compound> {
        super::common::client_tag(event.entity)
    }
}

fn int_array(values: impl Iterator<Item = i32>) -> Tag {
    Tag::IntArray(values.collect())
}

fn tick_items(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>, n: usize, recipe: &str, usage: u64) -> bool {
    let host: &dyn Host = &*host;
    let mut f = Factory::new(n, usage, upgrades(&entity.nbt));
    f.machine.energy.stored = get_long(&entity.nbt, "energy").min(f.machine.energy.capacity);
    discharge_slot(host, entity, ENERGY, &mut f.machine.energy);
    for (p, v) in f.progress.iter_mut().zip(get_ints(&entity.nbt, "progress")) {
        *p = v.max(0) as u32;
    }
    let (combining, sawing) = (recipe == "combining", recipe == "sawing");
    let stack_at = |entity: &ModBlockEntityView<'_>, i: usize| entity.items.get(i).and_then(Option::as_ref).cloned();
    let mut outputs: Vec<Slot> = (0..n).map(|i| slot(host, stack_at(entity, ROW + i).as_ref())).collect();
    // The sawing factory's secondary output slot of each process.
    let mut seconds: Vec<Slot> =
        (0..n).map(|i| if sawing { slot(host, stack_at(entity, CHEM + i).as_ref()) } else { Slot::default() }).collect();
    // The combiner factory's one extra input slot, shared by every process.
    let mut extra = if combining { slot(host, stack_at(entity, CHEM).as_ref()) } else { Slot::default() };
    sort_tick(host, entity, n, recipe);
    let mut inputs: Vec<Slot> = (0..n).map(|i| slot(host, stack_at(entity, i).as_ref())).collect();
    let mut chances: Vec<Option<(String, u32, f64)>> = vec![None; n];
    let mut second_full = vec![false; n];
    let mut operations: Vec<Option<(Operation, u32)>> = (0..n)
        .map(|i| {
            let (op, max, chance) = find_operation(host, recipe, stack_at(entity, i).as_ref(), &extra);
            chances[i] = chance;
            op.map(|o| (o, max))
        })
        .collect();
    // `NOT_ENOUGH_SPACE_SECONDARY_OUTPUT_ERROR`: a process stops while its secondary slot cannot take the output.
    for i in 0..n {
        if let Some((item, count, _)) = &chances[i]
            && operations[i].is_some()
        {
            let max = max_stack(host, item);
            if !(seconds[i].item.as_ref().is_none_or(|s| s == item) && seconds[i].count + count <= max) {
                operations[i] = None;
                second_full[i] = true;
            }
        }
    }
    // The issues of each process (`RecipeError`): 1 energy, 2 input, 4 output room, 8 input without a recipe, 16 the
    // secondary output's room.
    let warn = (0..n).map(|i| match &operations[i] {
        None if second_full[i] => 4 | 16,
        None if inputs[i].item.is_some() => 8,
        None => 0,
        Some((op, max)) => {
            let fits = match &outputs[i].item {
                None => op.output_count <= *max,
                Some(item) => *item == op.output && outputs[i].count + op.output_count <= *max,
            };
            if !fits {
                4
            } else if inputs[i].count < op.input_count || (combining && extra.count < op.extra_count) {
                2
            } else {
                i32::from(f.machine.energy.stored < f.machine.usage)
            }
        }
    });
    entity.nbt.insert("warn".into(), int_array(warn));
    let before: Vec<u32> = outputs.iter().map(|o| o.count).collect();
    f.tick(&mut inputs, &mut outputs, combining.then_some(&mut extra), &operations);
    // The sawing factory's chance output, rolled for each process that finished an operation.
    for i in 0..n {
        if outputs[i].count != before[i]
            && let Some((item, count, p)) = chances[i].clone()
            && roll(&mut entity.nbt) < p
        {
            seconds[i].item = Some(item);
            seconds[i].count += count;
        }
    }
    for i in 0..n {
        set_slot(host, entity, i, &inputs[i]);
        set_slot(host, entity, ROW + i, &outputs[i]);
        if sawing {
            set_slot(host, entity, CHEM + i, &seconds[i]);
        }
    }
    if combining {
        set_slot(host, entity, CHEM, &extra);
    }
    put_long(&mut entity.nbt, "energy", f.machine.energy.stored);
    entity.nbt.insert("progress".into(), int_array(f.progress.iter().map(|p| (*p).min(i32::MAX as u32) as i32)));
    f.active
}

/// The item and chemical factories (`TileEntityItemStackChemicalToItemStackFactory`): each process turns its item
/// into another over the recipe's ticks, using the shared tank's chemical a little every tick
/// (`ChemicalUsageMultiplier.constantUse`: the whole amount spread over the ticks) and the shared energy.
fn tick_chem(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>, n: usize, kind: &str, usage: u64) -> Option<bool> {
    let host: &dyn Host = &*host;
    let m = chem_factory(kind)?;
    let mut f = Factory::new(n, usage, upgrades(&entity.nbt));
    f.machine.energy.stored = get_long(&entity.nbt, "energy").min(f.machine.energy.capacity);
    discharge_slot(host, entity, ENERGY, &mut f.machine.energy);
    let up = upgrades(&entity.nbt);
    let chem_fraction = up.fraction_of(Upgrade::Chemical);
    let ticks = f.machine.ticks_required.max(1);
    let mut tank = get_tank(&entity.nbt, "chemical", factory::chemical_per_process(kind) * n as u64);
    let mut progress: Vec<u32> = {
        let saved = get_ints(&entity.nbt, "progress");
        (0..n).map(|i| saved.get(i).copied().unwrap_or(0).max(0) as u32).collect()
    };
    let mut used: Vec<u64> = {
        let saved = get_ints(&entity.nbt, "used");
        (0..n).map(|i| saved.get(i).copied().unwrap_or(0).max(0) as u64).collect()
    };
    let in_tag = |tag: &str, item: &str| super::in_tag(host, item, tag.trim_start_matches('#'));
    // `fillOrConvert`: a chemical item in the slot fills the tank, or converts into some of its chemical.
    if let Some(Some(extra)) = entity.items.get(CHEM).cloned()
        && let Some(c) = mekanism_core::recipes::find_conversion(&super::item_id(host, &extra), &in_tag)
        && m.chemicals.contains(&c.output)
        && tank.accepts(c.output)
        && tank.amount + c.amount <= tank.capacity
    {
        tank.insert(c.output, c.amount);
        entity.items[CHEM] = extra.shrink(1);
    }
    sort_tick(host, entity, n, kind);
    let mut warn = vec![0i32; n];
    let mut any_active = false;
    for i in 0..n {
        let input = entity.items.get(i).and_then(Option::as_ref).cloned();
        let out = entity.items.get(ROW + i).and_then(Option::as_ref).cloned();
        // With a chemical in the tank, only its recipes.
        let held = tank.chemical.as_deref();
        let found = input.as_ref().and_then(|stack| {
            let id = super::item_id(host, stack);
            m.table
                .iter()
                .find(|r| held.is_none_or(|c| c == r.chemical) && stack.count() >= r.input_count && r.input.matches(&id, &in_tag))
                .map(|r| (stack.clone(), *r))
        });
        let Some((stack, r)) = found else {
            warn[i] = if input.is_some() { 8 } else { 0 };
            progress[i] = 0;
            used[i] = 0;
            continue;
        };
        let max = max_stack(host, r.output);
        let fits = out
            .as_ref()
            .is_none_or(|o| super::item_id(host, o) == r.output && o.count() + r.output_count <= max);
        // The infusing factory takes the infuse type as a whole when an operation ends (`TwoInputCachedRecipe`), the
        // others a little every tick.
        let per_tick = kind != "infusing";
        // The chemical upgrade spares chemical (`UpgradeUtils.getChemicalUsageMultiplier`): 200 base ticks, `M^-chemical` of the amount.
        let total = (r.chemical_amount as f64
            * f64::from(chem::ADVANCED_TICKS)
            * mekanism_core::upgrades::max_upgrade_multiplier().powf(-chem_fraction))
        .round() as u64;
        // The injecting and purifying factories draw it statistically (`randomizedConsumption`), the others spread the total.
        let chemical_upgrades = matches!(kind, "injecting" | "purifying");
        let statistical = chemical_upgrades && usage_const("randomizedConsumption", 1) != 0;
        let want = if !per_tick {
            0
        } else if r.chemical_amount == 1 {
            let roll = roll(&mut entity.nbt);
            chemical_need(
                &up,
                chem::ADVANCED_TICKS,
                ticks,
                progress[i],
                used[i].min(u64::from(u32::MAX)) as u32,
                statistical,
                chemical_upgrades,
                roll,
            )
        } else {
            (total * u64::from(progress[i] + 1) / u64::from(ticks)).saturating_sub(used[i])
        };
        let has_chemical = tank.chemical.as_deref() == Some(r.chemical) && tank.amount >= if per_tick { want } else { r.chemical_amount };
        let has_energy = f.machine.energy.stored >= f.machine.usage;
        warn[i] = i32::from(!has_energy) | i32::from(!has_chemical) << 1 | i32::from(!fits) << 2;
        if fits && has_chemical && has_energy {
            f.machine.energy.consume(f.machine.usage);
            tank.extract(want);
            used[i] += want;
            progress[i] += 1;
            any_active = true;
            if progress[i] >= ticks {
                progress[i] = 0;
                used[i] = 0;
                if !per_tick {
                    tank.extract(r.chemical_amount);
                }
                entity.items[i] = stack.shrink(r.input_count);
                entity.items[ROW + i] = match out {
                    Some(mut o) => {
                        let _ = o.grow(r.output_count);
                        Some(o)
                    }
                    None => key_of(host, r.output).and_then(|k| ItemStack::new(k, r.output_count)),
                };
            }
        }
    }
    entity.nbt.insert("warn".into(), int_array(warn.into_iter()));
    entity.nbt.insert("progress".into(), int_array(progress.iter().map(|p| *p as i32)));
    entity.nbt.insert("used".into(), int_array(used.iter().map(|u| (*u).min(i32::MAX as u64) as i32)));
    put_long(&mut entity.nbt, "energy", f.machine.energy.stored);
    put_tank(&mut entity.nbt, "chemical", &tank);
    Some(any_active)
}

/// The advanced machine whose recipes and chemicals a chemical factory runs.
fn chem_factory(recipe: &str) -> Option<&'static chem::AdvancedMachine> {
    if recipe == "infusing" {
        return Some(&chem::INFUSING_FACTORY);
    }
    let block = match recipe {
        "compressing" => "mekanism:osmium_compressor",
        "purifying" => "mekanism:purification_chamber",
        "injecting" => "mekanism:chemical_injection_chamber",
        _ => return None,
    };
    chem::ADVANCED_MACHINES.iter().find(|m| m.block == block)
}

/// Sorts the inputs once a second while the sorting tab is on, and counts the second.
fn sort_tick(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, n: usize, recipe: &str) {
    let tick = get_int(&entity.nbt, "sort_tick");
    if get_int(&entity.nbt, "sorting") & 1 != 0 && tick % 20 == 0 {
        sort_inputs(host, entity, n, recipe);
    }
    put_int(&mut entity.nbt, "sort_tick", (tick + 1) % 20);
}

/// `TileEntityFactory.sortInventory`: each kind of input item is spread evenly over the slots that hold it and
/// the empty ones it can fill (`factory::sort_counts`; a slot needs the recipe's input amount at least).
fn sort_inputs(host: &dyn Host, entity: &mut ModBlockEntityView<'_>, n: usize, recipe: &str) {
    let mut kinds: Vec<(Vec<usize>, u64)> = vec![];
    let mut empties = vec![];
    for i in 0..n {
        match entity.items.get(i).and_then(Option::as_ref) {
            None => empties.push(i),
            Some(s) => match kinds.iter_mut().find(|(slots, _)| {
                entity.items[slots[0]].as_ref().is_some_and(|o| o.is_stackable_with(s))
            }) {
                Some((slots, total)) => {
                    slots.push(i);
                    *total += u64::from(s.count());
                }
                None => kinds.push((vec![i], u64::from(s.count()))),
            },
        }
    }
    for (slots, total) in kinds {
        let Some(proto) = entity.items[slots[0]].clone() else {
            continue;
        };
        let id = super::item_id(host, &proto);
        let max = u64::from(max_stack(host, &id));
        if max * (slots.len() as u64) < total {
            continue;
        }
        let spare = std::mem::take(&mut empties);
        // A slot needs the recipe's input amount to run it; an item with no recipe is split as if one item were enough.
        let mut sample = proto.clone();
        let _ = sample.set_count(total.min(max) as u32);
        let min = if factory::takes_chemical(recipe) {
            chem_input_count(host, recipe, &sample)
        } else {
            find_operation(host, recipe, Some(&sample), &Slot::default()).0.map_or(1, |o| u64::from(o.input_count))
        };
        let counts = factory::sort_counts(total, slots.len(), spare.len(), min);
        let mut targets = slots.clone();
        let mut left = spare;
        while targets.len() < counts.len() {
            targets.push(left.remove(0));
        }
        empties = left;
        for (&i, &c) in targets.iter().zip(&counts) {
            entity.items[i] = if c > 0 {
                let mut s = proto.clone();
                s.set_count(c as u32).ok().map(|()| s)
            } else {
                None
            };
        }
    }
}

/// The input amount a chemical factory's recipe needs of `stack`, 1 when it has none.
fn chem_input_count(host: &dyn Host, recipe: &str, stack: &ItemStack) -> u64 {
    let Some(m) = chem_factory(recipe) else {
        return 1;
    };
    let id = super::item_id(host, stack);
    let in_tag = |tag: &str, item: &str| super::in_tag(host, item, tag.trim_start_matches('#'));
    m.table.iter().find(|r| r.input.matches(&id, &in_tag)).map_or(1, |r| u64::from(r.input_count))
}
