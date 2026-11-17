//! The machines that move chemicals and fluids between tanks: electrolytic separator, rotary condensentrator,
//! chemical infuser, pigment mixer, isotopic centrifuge, chemical washer and nutritional liquifier. Ports of
//! `tick_separator`, `tick_rotary`, `tick_chemical_infuser`, `tick_centrifuge`, `tick_washer` and
//! `tick_liquifier` of `runtime.rs`.
use super::chem::{
    bucket_fluid, drain_tank_into_bucket, drain_tank_into_item, fill_tank_from_bucket, fill_tank_from_item, get_tank,
    machine_power, move_full_chemical_item, push_chemical, put_item, put_tank, stack_of, usage_const,
};
use super::common::{MenuData, discharge_slot, put_client, set_active, upgrades};
use super::multiblock::chemical_index;
use super::nbt::{get_int, get_long, put_int, put_long};
use crate::gear::data as item_data;
use mekanism_core::chemical::{self as chem, ChemicalTank};
use mekanism_core::energy::EnergyContainer;
use mekanism_core::nuclear;
use mekanism_core::upgrades::Upgrade;
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Compound, Host,
    ModBlockEntityView, ModContainer, Registrar, ServerHost, Tag,
};

/// The id of the block entity type of these machines (`mekanism:fluid_chemical_machine`).
pub(crate) const TYPE: &str = "fluid_chemical_machine";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Separator,
    Rotary,
    ChemicalInfuser,
    PigmentMixer,
    Centrifuge,
    Washer,
    Liquifier,
}

const BLOCKS: [(&str, Kind); 7] = [
    ("mekanism:electrolytic_separator", Kind::Separator),
    ("mekanism:rotary_condensentrator", Kind::Rotary),
    ("mekanism:chemical_infuser", Kind::ChemicalInfuser),
    ("mekanism:pigment_mixer", Kind::PigmentMixer),
    ("mekanism:isotopic_centrifuge", Kind::Centrifuge),
    ("mekanism:chemical_washer", Kind::Washer),
    (chem::LIQUIFIER, Kind::Liquifier),
];

fn kind(id: &str) -> Option<Kind> {
    BLOCKS.iter().find(|(b, _)| *b == id).map(|(_, k)| *k)
}

/// The energy buffer of a machine of this family (`receiver` of `runtime.rs`).
fn energy_receiver(id: &str, nbt: &Compound, _: bool, _: bool) -> Option<super::transmit::Receiver> {
    let capacity = match kind(id)? {
        Kind::Separator => machine_power(nbt, chem::SEPARATOR_USAGE, None).3,
        Kind::Rotary => machine_power(nbt, usage_const("rotaryCondensentrator", chem::ROTARY_USAGE), None).3,
        Kind::ChemicalInfuser => machine_power(nbt, usage_const("chemicalInfuser", chem::CHEMICAL_INFUSER_USAGE), None).3,
        Kind::PigmentMixer => machine_power(nbt, usage_const("pigmentMixer", chem::PIGMENT_MIXER_USAGE), None).3,
        Kind::Centrifuge => machine_power(nbt, usage_const("isotopicCentrifuge", nuclear::CENTRIFUGE_USAGE), None).3,
        Kind::Washer => machine_power(nbt, usage_const("chemicalWasher", nuclear::WASHER_USAGE), None).3,
        Kind::Liquifier => {
            machine_power(nbt, usage_const("nutritionalLiquifier", chem::LIQUIFIER_USAGE), Some(chem::LIQUIFIER_TICKS)).3
        }
    };
    Some(super::transmit::Receiver::new(capacity, u64::MAX))
}

/// Which pipes and tubes reach these machines, and the tank that takes what they bring.
fn chemical_receiver(id: &str, nbt: &Compound, chemical: &str, _: bool, _: [i32; 3]) -> Option<(&'static str, u64)> {
    let kind = kind(id)?;
    if super::transmit::is_fluid(chemical) {
        return match kind {
            Kind::Separator if chem::separates(chemical) => Some(("fluid", chem::SEPARATOR_FLUID)),
            Kind::Rotary if chem::rotary_chemical(chemical).is_some() => Some(("fluid", chem::ROTARY_TANK)),
            Kind::Washer if chemical == "minecraft:water" => Some(("fluid", nuclear::WASHER_TANK)),
            _ => None,
        };
    }
    // The infuser and the mixer fill whichever input tank holds this chemical, else an empty one.
    let (cap, valid): (u64, fn(&str) -> bool) = match kind {
        Kind::ChemicalInfuser => (chem::CHEMICAL_INFUSER_TANK, mekanism_core::recipes::infusing_input),
        Kind::PigmentMixer => (chem::PIGMENT_MIXER_TANK, mekanism_core::recipes::pigment_mixing_input),
        Kind::Centrifuge => return nuclear::centrifuging(chemical).map(|_| ("input", nuclear::CENTRIFUGE_TANK)),
        Kind::Washer => return nuclear::find_washing(chemical).map(|_| ("input", nuclear::WASHER_TANK)),
        _ => return None,
    };
    if !valid(chemical) {
        return None;
    }
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

/// Pipes reach the liquifier, separator, rotary and washer; tubes all but the liquifier (`serves` of `runtime.rs`).
fn serves(id: &str) -> u8 {
    match kind(id) {
        Some(Kind::Liquifier) => 1,
        Some(Kind::Separator | Kind::Rotary | Kind::Washer) => 3,
        Some(_) => 2,
        None => 0,
    }
}

pub(crate) fn register(r: &mut dyn Registrar) {
    super::transmit::register_energy_receiver(energy_receiver);
    super::transmit::register_chemical_family(super::transmit::ChemicalFamily { serves, receiver: chemical_receiver, source: |_, _| None });
    let ids: Vec<&str> = BLOCKS.iter().map(|(id, _)| *id).collect();
    let entity = r.block_entity_type(TYPE).blocks(&ids).build();
    r.block_entity_part(&entity, FluidChemicalPart).expect("native block entity type");
    for (id, kind) in BLOCKS {
        let block = r.block_key(id);
        r.block_container(&block, container(kind));
    }
}

/// A hopper's five slots. The machines of the first kind have only the energy slot and chemical slots, which
/// automation fills; the others keep the furnace's roles for their item slots.
fn container(kind: Kind) -> ModContainer {
    let (inputs, outputs, side): (Vec<usize>, Vec<usize>, Vec<usize>) = match kind {
        Kind::Centrifuge => return all_slots(vec![0, 1, 2]),
        Kind::ChemicalInfuser | Kind::PigmentMixer => return all_slots(vec![0, 1, 2, 3]),
        Kind::Washer => return all_slots(vec![0, 1, 3, 4]),
        // The water or brine bucket in the first slot, the two gases' drain slots after the furnace's three.
        Kind::Separator => (vec![0, 1, 3, 4], vec![2], vec![1, 3, 4]),
        // The bucket in the first slot, the drain slot (3) and the output it moves a full item to (4).
        Kind::Rotary | Kind::Liquifier => (vec![0, 1, 3], vec![2, 4], vec![1, 3]),
    };
    ModContainer {
        slots: 5,
        faces: [vec![2], vec![0], side.clone(), side.clone(), side.clone(), side],
        inputs,
        outputs,
        menu: Some("minecraft:hopper".into()),
    }
}

fn all_slots(used: Vec<usize>) -> ModContainer {
    ModContainer {
        slots: 5,
        faces: std::array::from_fn(|_| used.clone()),
        inputs: used,
        outputs: vec![],
        menu: Some("minecraft:hopper".into()),
    }
}

struct FluidChemicalPart;

#[mod_api::horizon_part]
impl BlockPart for FluidChemicalPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for FluidChemicalPart {
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
        let entity = &mut event.entity;
        let active = match kind {
            Kind::Separator => tick_separator(host, pos, entity),
            Kind::Rotary => tick_rotary(host, pos, entity),
            Kind::ChemicalInfuser => tick_chemical_infuser(host, pos, entity, false),
            Kind::PigmentMixer => {
                let active = tick_chemical_infuser(host, pos, entity, true);
                let mut flag = Compound::new();
                put_int(&mut flag, "shaft_active", u32::from(active));
                put_client(entity, &["shaft_active"], flag);
                active
            }
            Kind::Centrifuge => tick_centrifuge(host, pos, entity),
            Kind::Washer => tick_washer(host, pos, entity),
            Kind::Liquifier => tick_liquifier(host, pos, entity),
        };
        set_active(host, pos, active);
    }
}

/// The menu data: energy, upgrades, the furnace's four slots and one gauge for each tank in the screen's order.
fn publish(
    entity: &mut ModBlockEntityView<'_>,
    energy: &EnergyContainer,
    usage: u64,
    furnace: [i32; 4],
    tanks: &[&ChemicalTank],
) {
    let u = upgrades(&entity.nbt);
    let mut data = MenuData::new(energy.stored, energy.capacity, usage).furnace(furnace).upgrades(&u);
    for t in tanks {
        data = data.gauge(t.amount, t.capacity, chemical_index(t.chemical.as_deref()));
    }
    data.write(entity);
}

fn energy_of(entity: &ModBlockEntityView<'_>, capacity: u64) -> EnergyContainer {
    let mut energy = EnergyContainer::new(capacity, u64::MAX);
    energy.stored = get_long(&entity.nbt, "energy").min(capacity);
    energy
}

/// The Electrolytic Separator: water or brine buckets fill its 24,000 mB tank, and each tick runs one `separating`
/// operation at 4 FE: 2 mB water → 2 hydrogen + 1 oxygen, or 10 mB brine → 1 sodium + 1 chlorine. The chemicals go out
/// to the blocks beside it.
fn tick_separator(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let (_, usage, _, capacity) = machine_power(&entity.nbt, chem::SEPARATOR_USAGE, None);
    let mut energy = energy_of(entity, capacity);
    let mut fluid = get_tank(&entity.nbt, "fluid", chem::SEPARATOR_FLUID);
    let mut left = get_tank(&entity.nbt, "left", chem::SEPARATOR_GAS);
    let mut right = get_tank(&entity.nbt, "right", chem::SEPARATOR_GAS);
    {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 1, &mut energy);
        // The empty bucket stays in the slot (there is no output slot for it).
        fill_tank_from_bucket(h, entity, 0, None, &mut fluid, |f| matches!(f, "minecraft:water" | "mekanism:brine"));
        drain_tank_into_item(h, entity, 3, &mut left);
        drain_tank_into_item(h, entity, 4, &mut right);
    }
    let warn = if fluid.amount > 0 && (left.amount >= left.capacity || right.amount >= right.capacity) { 4 } else { 0 };
    put_int(&mut entity.nbt, "warn", warn);
    let (mode_left, mode_right) = (get_int(&entity.nbt, "dump_left") % 3, get_int(&entity.nbt, "dump_right") % 3);
    // `canFunction`: not while both tanks dump their excess and both are at the level they keep.
    let keep = mekanism_core::config_files::active_double("general", "auto_eject", "dumpExcessKeepRatio", 0.85).clamp(0.001, 1.0);
    let at_target = |t: &ChemicalTank| t.amount >= (t.capacity as f64 * keep) as u64;
    let can_function = mode_left != 1 || mode_right != 1 || !at_target(&left) || !at_target(&right);
    let active = can_function && chem::separate(&mut fluid, &mut left, &mut right, &mut energy);
    // `handleTank`: `ChemicalUtils.dump` at 8 mB a tick per speed upgrade doubling, or the excess over the kept level.
    let dump_rate = 8u64 << upgrades(&entity.nbt).get(Upgrade::Speed).min(20);
    let eject = mekanism_core::config_files::active_int("general", "auto_eject", "chemical", 1_024).max(1);
    for (tank, mode) in [(&mut left, mode_left), (&mut right, mode_right)] {
        if mode == 0 || tank.amount == 0 {
            continue;
        }
        let to_dump = if mode == 2 {
            dump_rate
        } else {
            let target = (tank.capacity as f64 * keep) as u64;
            if target < tank.amount { (tank.amount - target).min(eject) } else { 0 }
        };
        tank.extract(to_dump);
    }
    // `setCanTankEject`: a tank that dumps does not eject.
    if mode_left != 2 {
        push_chemical(host, pos, &entity.nbt.clone(), &mut left, None, u64::MAX);
    }
    if mode_right != 2 {
        push_chemical(host, pos, &entity.nbt.clone(), &mut right, None, u64::MAX);
    }
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fluid", &fluid);
    put_tank(&mut entity.nbt, "left", &left);
    put_tank(&mut entity.nbt, "right", &right);
    // The two dump modes, then the arrow (full while it works).
    publish(entity, &energy, usage, [mode_left as i32, mode_right as i32, if active { 1000 } else { 0 }, 1000], &[&fluid, &left, &right]);
    active
}

/// The Rotary Condensentrator turning fluid into chemical (`rotary` recipes: water → water vapour), 1 mB a tick at
/// 2 FE, from buckets in its first slot (the empty bucket comes out of the last). The chemical goes out to the blocks
/// beside it.
fn tick_rotary(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let (_, usage, _, capacity) = machine_power(&entity.nbt, usage_const("rotaryCondensentrator", chem::ROTARY_USAGE), None);
    let mut energy = energy_of(entity, capacity);
    let mut fluid = get_tank(&entity.nbt, "fluid", chem::ROTARY_TANK);
    let mut gas = get_tank(&entity.nbt, "chemical", chem::ROTARY_TANK);
    {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 1, &mut energy);
        if let Some(Some(bucket)) = entity.items.first().cloned()
            && let Some(f) = bucket_fluid(&super::item_id(h, &bucket))
            && chem::rotary_chemical(&f).is_some()
            && fluid.accepts(&f)
            && fluid.amount + 1_000 <= fluid.capacity
            && put_item(h, entity, 2, "minecraft:bucket")
        {
            fluid.insert(&f, 1_000);
            entity.items[0] = None;
        }
        // `ChemicalInventorySlot.rotary`: the gas drains into a tank item, which moves to the output slot once full.
        drain_tank_into_item(h, entity, 3, &mut gas);
        move_full_chemical_item(h, entity, 3, 4);
    }
    let warn = match fluid.chemical.as_deref().filter(|_| fluid.amount > 0).map(chem::rotary_chemical) {
        Some(None) => 8,
        Some(Some(output)) if !(gas.accepts(output) && gas.amount < gas.capacity) => 4,
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    let mut active = false;
    if let Some(output) = fluid.chemical.as_deref().and_then(chem::rotary_chemical)
        && fluid.amount > 0
        && gas.accepts(output)
        && gas.amount < gas.capacity
        && energy.consume(usage)
    {
        fluid.extract(1);
        gas.insert(output, 1);
        active = true;
    }
    push_chemical(host, pos, &entity.nbt.clone(), &mut gas, None, u64::MAX);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "fluid", &fluid);
    put_tank(&mut entity.nbt, "chemical", &gas);
    publish(
        entity,
        &energy,
        usage,
        [(fluid.amount * 1000 / chem::ROTARY_TANK) as i32, 1000, (gas.amount * 1000 / chem::ROTARY_TANK) as i32, 1000],
        &[&gas, &fluid],
    );
    active
}

/// The Chemical Infuser (and the Pigment Mixer): one operation a tick of two chemicals into a third (oxygen + sulfur
/// dioxide → sulfur trioxide), 15 FE a tick, pushed out to the blocks beside it.
fn tick_chemical_infuser(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>, pigment: bool) -> bool {
    let (cap, usage, out_cap) = if pigment {
        (chem::PIGMENT_MIXER_TANK, usage_const("pigmentMixer", chem::PIGMENT_MIXER_USAGE), chem::PIGMENT_MIXER_TANK * 2)
    } else {
        (chem::CHEMICAL_INFUSER_TANK, usage_const("chemicalInfuser", chem::CHEMICAL_INFUSER_USAGE), chem::CHEMICAL_INFUSER_TANK)
    };
    let valid = if pigment { mekanism_core::recipes::pigment_mixing_input } else { mekanism_core::recipes::infusing_input };
    let find = if pigment { mekanism_core::recipes::find_pigment_mixing } else { mekanism_core::recipes::find_chemical_infusing };
    let (_, usage, _, capacity) = machine_power(&entity.nbt, usage, None);
    let mut energy = energy_of(entity, capacity);
    let mut left = get_tank(&entity.nbt, "left", cap);
    let mut right = get_tank(&entity.nbt, "right", cap);
    let mut out = get_tank(&entity.nbt, "output", out_cap);
    let mut active = false;
    {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 0, &mut energy);
        fill_tank_from_item(h, entity, 1, &mut left, valid);
        fill_tank_from_item(h, entity, 2, &mut right, valid);
        drain_tank_into_item(h, entity, 3, &mut out);
    }
    if let (Some(a), Some(b)) = (left.chemical.clone(), right.chemical.clone())
        && let Some((r, swapped)) = find(&a, &b)
    {
        let (need_a, need_b) = if swapped { (r.right_amount, r.left_amount) } else { (r.left_amount, r.right_amount) };
        if left.amount >= need_a
            && right.amount >= need_b
            && out.accepts(r.output)
            && out.amount + r.amount <= out.capacity
            && energy.consume(usage)
        {
            left.extract(need_a);
            right.extract(need_b);
            out.insert(r.output, r.amount);
            active = true;
        }
    }
    push_chemical(host, pos, &entity.nbt.clone(), &mut out, None, u64::MAX);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "left", &left);
    put_tank(&mut entity.nbt, "right", &right);
    put_tank(&mut entity.nbt, "output", &out);
    publish(entity, &energy, usage, [0, 1000, 0, 1000], &[&left, &out, &right]);
    active
}

/// The Isotopic Centrifuge: each tick it turns its input (uranium hexafluoride, nuclear waste) into its output and
/// pushes that to the tanks beside it.
fn tick_centrifuge(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let (_, usage, _, capacity) = machine_power(&entity.nbt, usage_const("isotopicCentrifuge", nuclear::CENTRIFUGE_USAGE), None);
    let mut energy = energy_of(entity, capacity);
    let mut input = get_tank(&entity.nbt, "input", nuclear::CENTRIFUGE_TANK);
    let mut output = get_tank(&entity.nbt, "output", nuclear::CENTRIFUGE_TANK);
    let mut active = false;
    {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 0, &mut energy);
        fill_tank_from_item(h, entity, 1, &mut input, |c| nuclear::centrifuging(c).is_some());
        drain_tank_into_item(h, entity, 2, &mut output);
    }
    if let Some((need, made, amount)) = input.chemical.as_deref().and_then(nuclear::centrifuging)
        && input.amount >= need
        && output.accepts(made)
        && output.amount + amount <= output.capacity
        && energy.consume(usage)
    {
        input.extract(need);
        output.insert(made, amount);
        active = true;
    }
    push_chemical(host, pos, &entity.nbt.clone(), &mut output, None, u64::MAX);
    let warn = match input.chemical.as_deref().and_then(nuclear::centrifuging) {
        None if input.chemical.is_some() => 8,
        Some((need, made, amount)) => {
            u32::from(input.amount < need) << 1
                | u32::from(!(output.accepts(made) && output.amount + amount <= output.capacity)) << 2
        }
        None => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "output", &output);
    publish(entity, &energy, usage, [0, 1000, (input.amount * 1000 / nuclear::CENTRIFUGE_TANK) as i32, 1000], &[&input, &output]);
    active
}

/// The Chemical Washer: each tick, a dirty slurry piped in and water from pipes make the clean slurry, pushed out to
/// the tanks and tubes beside it.
fn tick_washer(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let (_, usage, _, capacity) = machine_power(&entity.nbt, usage_const("chemicalWasher", nuclear::WASHER_USAGE), None);
    let mut energy = energy_of(entity, capacity);
    let mut input = get_tank(&entity.nbt, "input", nuclear::WASHER_TANK);
    let mut water = get_tank(&entity.nbt, "fluid", nuclear::WASHER_TANK);
    let mut output = get_tank(&entity.nbt, "output", nuclear::WASHER_TANK);
    let mut active = false;
    {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 0, &mut energy);
        drain_tank_into_item(h, entity, 1, &mut output);
        // `FluidInventorySlot.fill`: a water bucket empties into the tank, the empty one goes to the last slot.
        fill_tank_from_bucket(h, entity, 3, Some(4), &mut water, |f| f == "minecraft:water");
    }
    if let Some(r) = input.chemical.as_deref().and_then(nuclear::find_washing)
        && input.amount >= r.input_amount
        && water.chemical.as_deref() == Some("minecraft:water")
        && water.amount >= r.water
        && output.accepts(r.output)
        && output.amount + r.amount <= output.capacity
        && energy.consume(usage)
    {
        input.extract(r.input_amount);
        water.extract(r.water);
        output.insert(r.output, r.amount);
        active = true;
    }
    push_chemical(host, pos, &entity.nbt.clone(), &mut output, None, u64::MAX);
    let warn = match input.chemical.as_deref().map(nuclear::find_washing) {
        Some(None) => 8,
        Some(Some(r)) => {
            u32::from(input.amount < r.input_amount || water.chemical.as_deref() != Some("minecraft:water") || water.amount < r.water) << 1
                | u32::from(!(output.accepts(r.output) && output.amount + r.amount <= output.capacity)) << 2
        }
        None => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_tank(&mut entity.nbt, "input", &input);
    put_tank(&mut entity.nbt, "fluid", &water);
    put_tank(&mut entity.nbt, "output", &output);
    publish(entity, &energy, usage, [0, 1000, 0, 1000], &[&water, &input, &output]);
    active
}

/// A food's nutrition and what it leaves after eating (`minecraft:food`, `minecraft:use_remainder`).
fn food(host: &dyn Host, stack: &mod_api::ItemStack) -> Option<(u32, Option<String>)> {
    let key = stack.item();
    let Tag::Compound(food) = host.item_default_component(key, "minecraft:food")? else {
        return None;
    };
    let nutrition = match item_data::get(&food, "nutrition") {
        Some(Tag::Int(n)) => (*n).max(0) as u32,
        _ => 0,
    };
    if nutrition == 0 {
        return None;
    }
    let remainder = match host.item_default_component(key, "minecraft:use_remainder") {
        Some(Tag::Compound(c)) => item_data::get_string(&c, "id"),
        _ => None,
    };
    Some((nutrition, remainder))
}

/// The Nutritional Liquifier: food becomes nutritional paste (50 mB a point of nutrition) over 100 ticks at 8 FE a
/// tick. Foods with a use remainder (a stew's bowl) leave it in the output slot.
fn tick_liquifier(host: &mut dyn ServerHost, pos: [i32; 3], entity: &mut ModBlockEntityView<'_>) -> bool {
    let (_, usage, ticks, capacity) =
        machine_power(&entity.nbt, usage_const("nutritionalLiquifier", chem::LIQUIFIER_USAGE), Some(chem::LIQUIFIER_TICKS));
    let mut energy = energy_of(entity, capacity);
    let mut tank = get_tank(&entity.nbt, "output", chem::LIQUIFIER_TANK);
    let mut progress = get_int(&entity.nbt, "progress");
    let mut active = false;
    let (recipe, out) = {
        let h: &dyn Host = &*host;
        discharge_slot(h, entity, 1, &mut energy);
        drain_tank_into_bucket(h, entity, 3, Some(4), &mut tank);
        // `createDynamicRecipe`: food that heals gives paste, and what is left of it after eating.
        let recipe = entity.items.first().cloned().flatten().and_then(|stack| {
            let (nutrition, remainder) = food(h, &stack)?;
            Some((stack, u64::from(nutrition) * chem::paste_per_nutrition(), remainder))
        });
        (recipe, entity.items.get(2).cloned().flatten())
    };
    let h: &dyn Host = &*host;
    let fits = |paste: u64, remainder: &Option<String>| {
        tank.accepts(chem::NUTRITIONAL_PASTE)
            && tank.amount + paste <= tank.capacity
            && remainder.as_ref().is_none_or(|r| {
                out.as_ref().is_none_or(|o| super::item_id(h, o) == *r && o.count() < super::common::max_stack(h, r))
            })
    };
    let warn = match &recipe {
        None if entity.items.first().is_some_and(Option::is_some) => 8,
        Some((_, paste, remainder)) if !fits(*paste, remainder) => 4,
        _ => 0,
    };
    put_int(&mut entity.nbt, "warn", warn);
    let mut pending = None;
    match recipe {
        Some((stack, paste, remainder)) if fits(paste, &remainder) => {
            if energy.consume(usage) {
                active = true;
                progress += 1;
                if progress >= ticks {
                    progress = 0;
                    tank.insert(chem::NUTRITIONAL_PASTE, paste);
                    pending = Some((stack, remainder));
                }
            }
        }
        _ => progress = 0,
    }
    if let Some((stack, remainder)) = pending {
        entity.items[0] = stack.shrink(1);
        if let Some(r) = remainder {
            put_item(h, entity, 2, &r);
        }
    }
    // `RenderNutritionalLiquifier`: the paste's stage (of 40), whether it works, and the last food it ate.
    if active && let Some(Some(food)) = entity.items.first() {
        let item = super::item_id(h, food);
        entity.nbt.insert("last_item".into(), Tag::String(item.as_str().into()));
    }
    let mut shown = Compound::new();
    let scale = if tank.capacity == 0 { 0.0 } else { tank.amount as f64 / tank.capacity as f64 };
    put_int(&mut shown, "liq_stage", if tank.amount == 0 { 0 } else { ((scale * 39.0) as u32).min(39) });
    put_int(&mut shown, "liq_active", u32::from(active));
    shown.insert("liq_item".into(), Tag::String(item_data::get_string(&entity.nbt, "last_item").unwrap_or_default().as_str().into()));
    put_client(entity, &["liq_stage", "liq_active", "liq_item"], shown);
    put_long(&mut entity.nbt, "energy", energy.stored);
    put_int(&mut entity.nbt, "progress", progress);
    put_tank(&mut entity.nbt, "output", &tank);
    let _ = stack_of;
    publish(entity, &energy, usage, [(energy.stored * 1000 / capacity) as i32, 1000, progress as i32, ticks as i32], &[&tank]);
    active
}
