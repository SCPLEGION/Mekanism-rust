//! The basic electric machines (`TileEntityElectricMachine`): enrichment chamber, crusher, energized smelter,
//! precision sawmill and combiner. Port of `tick_machine` and its container of `runtime.rs`.
use super::common::{discharge_slot, key_of, max_stack, set_slot, slot, upgrades, set_active};
use super::nbt::{get_int, get_long, put_int, put_long, wide};
use crate::gear::data as item_data;
use mekanism_core::machine::{self, ElectricMachine, Operation, Slot, MACHINES};
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host, ItemStack,
    ModBlockEntityView, ModContainer, Registrar, ServerHost, Tag,
};

/// The id of the block entity type of these five machines (`mekanism:machine`).
pub(crate) const TYPE: &str = "machine";

pub(crate) fn register(r: &mut dyn Registrar) {
    let blocks: Vec<&str> = MACHINES.iter().map(|(id, _, _)| *id).collect();
    let kind = r.block_entity_type(TYPE).blocks(&blocks).build();
    r.block_entity_part(&kind, MachinePart).expect("native block entity type");
    for (id, recipe, _) in MACHINES {
        let block = r.block_key(id);
        r.block_container(&block, container(recipe));
    }
}

/// The slots of a machine: input, energy, output; the combiner's extra input and the sawmill's secondary
/// output sit in a fourth slot, which moves the menu from the furnace's to the hopper's (`extra_slots`).
fn container(recipe: &str) -> ModContainer {
    let mut inputs = vec![0, 1];
    let mut outputs = vec![2];
    let mut side = vec![1];
    match recipe {
        "combining" => {
            inputs.push(3);
            side.push(3);
        }
        "sawing" => outputs.push(3),
        _ => {
            return ModContainer {
                slots: 3,
                faces: [vec![2], vec![0], vec![1], vec![1], vec![1], vec![1]],
                inputs,
                outputs,
                menu: Some("minecraft:furnace".into()),
            };
        }
    }
    ModContainer {
        slots: 5,
        faces: [vec![2], vec![0], side.clone(), side.clone(), side.clone(), side],
        inputs,
        outputs,
        menu: Some("minecraft:hopper".into()),
    }
}

struct MachinePart;

#[mod_api::horizon_part]
impl BlockPart for MachinePart {}

#[mod_api::horizon_part]
impl BlockEntityPart for MachinePart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some((recipe, usage)) = machine::machine(event.entity.id()) else {
            return;
        };
        let active = tick(host, &mut event.entity, recipe, usage);
        // The block's `lit` shows whether the machine ran (the furnace's property); it only changes when it differs.
        set_active(host, event.pos, active);
    }
}

/// A Precision Sawmill's secondary output still waiting for the output slot.
fn secondary(nbt: &Compound) -> Option<(String, u32)> {
    let Some(Tag::Compound(c)) = item_data::get(nbt, "secondary") else {
        return None;
    };
    let Some(Tag::String(id)) = item_data::get(c, "id") else {
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
            nbt.retain(|k, _| k.to_string_lossy() != "secondary");
        }
    }
}

/// A number in [0, 1) for the sawmill's chance: a splitmix64 step kept in the block entity, so each
/// machine has its own sequence that survives saving.
pub(super) fn roll(nbt: &mut Compound) -> f64 {
    let mut x = get_long(nbt, "seed") ^ 0x9E37_79B9_7F4A_7C15;
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    put_long(nbt, "seed", x & (i64::MAX as u64));
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// What the machine does with the stack in its input slot: the operation, the stack limit of its output and
/// the sawmill's chance of a secondary output.
pub(super) fn find_operation(
    host: &dyn Host,
    recipe: &str,
    stack: Option<&ItemStack>,
    middle: &Slot,
) -> (Option<Operation>, u32, Option<(String, u32, f64)>) {
    let Some(stack) = stack else {
        return (None, 64, None);
    };
    let id = super::item_id(host, stack);
    let count = stack.count();
    let max = |id: &str| max_stack(host, id);
    let in_tag = |tag: &str, item: &str| super::in_tag(host, item, tag.trim_start_matches('#'));
    let op = |input_count, extra_count, output: &str, output_count| Operation {
        input_count,
        extra_count,
        output: output.to_owned(),
        output_count,
    };
    match recipe {
        "smelting" => {
            let found = host
                .recipes_of_type("minecraft:smelting")
                .iter()
                .find(|r| r.ingredients.first().is_some_and(|i| i.test(host, stack)))
                .and_then(|r| r.results.first().cloned());
            let Some(result) = found else {
                return (None, 64, None);
            };
            let out = super::item_id(host, &result);
            let m = max(&out);
            (Some(op(1, 0, &out, result.count())), m, None)
        }
        "sawing" => {
            let Some(r) = mekanism_core::recipes::find_sawing(&id, count, &in_tag) else {
                return (None, 64, None);
            };
            // A recipe with only a secondary output gives it every time here.
            let (main, chance) = match (r.main, r.secondary) {
                (Some(main), Some((sid, n))) => (main, Some((sid.to_owned(), n, r.secondary_chance))),
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
            let Some(r) = mekanism_core::recipes::find_combining((&id, count), (extra, middle.count), &in_tag) else {
                return (None, 64, None);
            };
            (Some(op(r.main_count, r.extra_count, r.output, r.output_count)), max(r.output), None)
        }
        _ => {
            let Some(r) =
                mekanism_core::recipes::find(mekanism_core::recipes::table(recipe), &id, count, &in_tag)
            else {
                return (None, 64, None);
            };
            (Some(op(r.input_count, 0, r.output, r.output_count)), max(r.output), None)
        }
    }
}

/// `tick_machine` of `runtime.rs`, line for line. Its result (whether the machine ran) is the block's `active`
/// state, applied by a later stage.
fn tick(host: &mut dyn ServerHost, entity: &mut ModBlockEntityView<'_>, recipe: &str, usage: u64) -> bool {
    let host: &dyn Host = &*host;
    let mut m = ElectricMachine::with_upgrades(usage, upgrades(&entity.nbt));
    m.energy.stored = get_long(&entity.nbt, "energy").min(m.energy.capacity);
    m.progress = get_int(&entity.nbt, "progress");
    // The middle slot is the machine's energy slot (the combiner's holds its extra input instead): an
    // energy tablet or cube there runs the machine.
    if recipe != "combining" {
        discharge_slot(host, entity, 1, &mut m.energy);
    }
    let mut input = slot(host, entity.items.first().and_then(Option::as_ref));
    let mut middle = slot(host, entity.items.get(1).and_then(Option::as_ref));
    let mut output = slot(host, entity.items.get(2).and_then(Option::as_ref));
    // The sawmill's secondary output slot (index 3): an output the old versions kept in the machine moves
    // into it when there is room.
    let mut second = slot(host, entity.items.get(3).and_then(Option::as_ref));
    if recipe == "sawing"
        && let Some((item, count)) = secondary(&entity.nbt)
    {
        let max = max_stack(host, &item);
        if second.item.as_ref().is_none_or(|i| *i == item) && second.count + count <= max {
            second.item = Some(item);
            second.count += count;
            set_secondary(&mut entity.nbt, None);
        }
    }
    let first = entity.items.first().and_then(Option::as_ref).cloned();
    let (mut operation, max_stack_out, chance) = find_operation(host, recipe, first.as_ref(), &middle);
    // `NOT_ENOUGH_SPACE_SECONDARY_OUTPUT_ERROR`: the sawmill stops while the secondary slot cannot take its output.
    let mut second_full = false;
    if let Some((item, count, _)) = &chance
        && operation.is_some()
    {
        let max = max_stack(host, item);
        if !(second.item.as_ref().is_none_or(|i| i == item) && second.count + count <= max) {
            operation = None;
            second_full = true;
        }
    }
    let before = output.count;
    // The issues the warning tab lists (`RecipeError`): bit 0 energy, 1 input, 2 output room, 3 input without a recipe.
    let mut warn = 0;
    match &operation {
        None if second_full => warn |= 4,
        None if input.item.is_some() => warn |= 8,
        None => {}
        Some(op) => {
            let fits = match &output.item {
                None => op.output_count <= max_stack_out,
                Some(item) => *item == op.output && output.count + op.output_count <= max_stack_out,
            };
            if !fits {
                warn |= 4;
            } else if input.count < op.input_count || (recipe == "combining" && middle.count < op.extra_count) {
                warn |= 2;
            } else if m.energy.stored < m.usage {
                warn |= 1;
            }
        }
    }
    put_int(&mut entity.nbt, "warn", warn);
    let extra = (recipe == "combining").then_some(&mut middle);
    m.tick(&mut input, &mut output, extra, operation.as_ref(), max_stack_out);
    if output.count != before
        && let Some((item, count, p)) = chance
        && roll(&mut entity.nbt) < p
    {
        second.item = Some(item);
        second.count += count;
    }
    set_slot(host, entity, 0, &input);
    if recipe == "combining" {
        set_slot(host, entity, 1, &middle);
    }
    set_slot(host, entity, 2, &output);
    put_long(&mut entity.nbt, "energy", m.energy.stored);
    put_int(&mut entity.nbt, "progress", m.progress);
    if recipe == "sawing" {
        set_slot(host, entity, 3, &second);
    }
    // The furnace menu's bars: the flame shows stored energy, the arrow the progress. Data slots are
    // 16 bits, so energy goes as thousandths of the capacity.
    entity.data = vec![
        (m.energy.stored * 1000 / m.energy.capacity.max(1)) as i32,
        1000,
        m.progress as i32,
        m.ticks_required as i32,
    ];
    // Mekanism's screen reads the rest.
    let u = upgrades(&entity.nbt);
    entity.data.extend(wide(m.energy.stored));
    entity.data.extend(wide(m.energy.capacity));
    entity.data.extend([(u.speed | u.anchor << 14 | u.chemical << 15 | u.muffling << 19 | u.stone_generator << 20) as i32, u.energy as i32]);
    entity.data.extend(wide(m.usage));
    m.active
}
