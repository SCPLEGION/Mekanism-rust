//! What every multiblock family shares (fission and fusion reactors, turbine, boiler, evaporation tower, SPS,
//! induction matrix, dynamic tank): finding the ticking level, reading block names, the cuboid search of
//! `StructureHelper.fetchCuboid`, chemical tanks kept in a block entity's compound and the fills clients draw.
//! Port of the helpers of `runtime.rs` those multiblocks shared.
use super::nbt::{get_int, get_long, put_int, put_long};
use mekanism_core::chemical::ChemicalTank;
use mod_api::{Compound, Host, LevelRef, ModBlockEntityView, Tag};
use std::collections::HashSet;

pub(crate) const DIRECTIONS: [(&str, [i32; 3]); 6] = [
    ("down", [0, -1, 0]),
    ("up", [0, 1, 0]),
    ("north", [0, 0, -1]),
    ("south", [0, 0, 1]),
    ("west", [-1, 0, 0]),
    ("east", [1, 0, 0]),
];

pub(crate) fn add(a: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [a[0] + d[0], a[1] + d[1], a[2] + d[2]]
}

/// The level the server ticks now, the one whose block entities a block entity tick may reach.
pub(crate) fn ticking_level(host: &dyn mod_api::ServerHost) -> Option<LevelRef> {
    host.levels().list_levels().into_iter().find(|l| l.in_place(host))
}

/// The name of the block at `at` (`mekanismgenerators:fission_reactor_port`), or an empty string when it is not loaded.
pub(crate) fn block_name(host: &dyn Host, level: LevelRef, at: [i32; 3]) -> String {
    let Some(state) = level.block_state(host, at) else {
        return String::new();
    };
    let full = host.block_state_string(state).unwrap_or_default();
    full.split('[').next().unwrap_or_default().to_owned()
}

/// `StructureHelper.fetchCuboid`: the box around the casing blocks joined to `start`, each side between its `min`
/// and `max` (x y z), with the casing blocks found.
pub(crate) fn find_box(
    host: &dyn Host,
    level: LevelRef,
    start: [i32; 3],
    is_frame: fn(&str) -> bool,
    min: [i32; 3],
    max: [i32; 3],
) -> Option<([i32; 3], [i32; 3], HashSet<[i32; 3]>)> {
    let mut seen = HashSet::from([start]);
    let mut todo = vec![start];
    let (mut lo, mut hi) = (start, start);
    while let Some(at) = todo.pop() {
        for k in 0..3 {
            lo[k] = lo[k].min(at[k]);
            hi[k] = hi[k].max(at[k]);
            if hi[k] - lo[k] + 1 > max[k] {
                return None;
            }
        }
        for (_, d) in DIRECTIONS {
            let next = add(at, d);
            if !seen.contains(&next) && is_frame(&block_name(host, level, next)) {
                seen.insert(next);
                todo.push(next);
            }
        }
    }
    if (0..3).any(|k| hi[k] - lo[k] + 1 < min[k]) {
        return None;
    }
    Some((lo, hi, seen))
}

/// `VoxelCuboid.getWallRelative(..).isOnEdge()`: the position lies on an edge (or corner) of the box.
pub(crate) fn on_box_edge(lo: [i32; 3], hi: [i32; 3], at: [i32; 3]) -> bool {
    (0..3).filter(|&k| at[k] == lo[k] || at[k] == hi[k]).count() >= 2
}

pub(crate) fn get_pos(nbt: &Compound, key: &str) -> Option<[i32; 3]> {
    let v = super::nbt::get_ints(nbt, key);
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

pub(crate) fn put_ints(nbt: &mut Compound, key: &str, v: Vec<i32>) {
    nbt.insert(key.into(), Tag::IntArray(v));
}

pub(crate) fn put_double(nbt: &mut Compound, key: &str, v: f64) {
    nbt.insert(key.into(), Tag::Double(v.to_bits()));
}

pub(crate) fn remove(nbt: &mut Compound, key: &str) {
    nbt.retain(|k, _| k.to_string_lossy() != key);
}

/// A chemical tank saved as `{id, amount}` under `key` (the layout of the fork, so saves carry over).
pub(crate) fn get_tank(nbt: &Compound, key: &str, capacity: u64) -> ChemicalTank {
    let mut t = ChemicalTank::new(capacity);
    if let Some(Tag::Compound(c)) = crate::gear::data::get(nbt, key)
        && let Some(Tag::String(id)) = crate::gear::data::get(c, "id")
        && let Ok(id) = id.to_string()
    {
        t.insert(&id, get_long(c, "amount"));
    }
    t
}

pub(crate) fn put_tank(nbt: &mut Compound, key: &str, t: &ChemicalTank) {
    match &t.chemical {
        Some(id) if t.amount > 0 => {
            let mut c = Compound::new();
            c.insert("id".into(), Tag::String(id.as_str().into()));
            put_long(&mut c, "amount", t.amount);
            nbt.insert(key.into(), Tag::Compound(c));
        }
        _ => remove(nbt, key),
    }
}

/// A fluid or chemical filling part of a multiblock's inside (`MultiblockTileEntityRenderer`): where its
/// interior starts, the interior's size, how full it is and with what.
pub(crate) struct Fill {
    pub at: [i32; 3],
    pub size: [i32; 3],
    pub scale: f64,
    pub chemical: Option<String>,
    /// A slightly smaller box, so that it does not fight the one it sits in (a reactor's heated coolant).
    pub shrunk: bool,
}

const FILL_KEYS: [&str; 10] = ["x", "y", "z", "l", "h", "w", "scale", "chem", "on", "shrunk"];

/// Tells clients what a multiblock's master holds, as up to two fills of its client view.
pub(crate) fn put_fills(entity: &mut ModBlockEntityView<'_>, fills: &[Fill]) {
    let mut shown = Compound::new();
    let mut owned = Vec::new();
    for i in 0..2 {
        for k in FILL_KEYS {
            owned.push(format!("f{i}_{k}"));
        }
        let Some(f) = fills.get(i) else {
            put_int(&mut shown, &format!("f{i}_on"), 0);
            remove(&mut entity.nbt, &format!("pscale{i}"));
            continue;
        };
        // `MekanismUtils.getScale`: the drawn level eases towards the real one, a tenth of the way a tick.
        let prev = get_int(&entity.nbt, &format!("pscale{i}")) as f32 / 1_000_000.0;
        let target = f.scale as f32;
        let (empty, full) = (target <= 0.0, target >= 1.0);
        let difference = (prev - target).abs();
        let eased = if difference > 0.01 {
            (9.0 * prev + target) / 10.0
        } else if (!empty && full && difference > 0.0) || (!empty && prev == 0.0) {
            target
        } else if empty && prev < 0.01 {
            0.0
        } else {
            prev
        };
        put_int(&mut entity.nbt, &format!("pscale{i}"), (eased * 1_000_000.0) as u32);
        for (k, v) in FILL_KEYS.iter().zip([f.at[0], f.at[1], f.at[2], f.size[0], f.size[1], f.size[2]]) {
            put_int(&mut shown, &format!("f{i}_{k}"), v as u32);
        }
        put_int(&mut shown, &format!("f{i}_scale"), (eased * 1_000_000.0) as u32);
        put_int(&mut shown, &format!("f{i}_chem"), chemical_index(f.chemical.as_deref()) as u32);
        put_int(&mut shown, &format!("f{i}_on"), 1);
        put_int(&mut shown, &format!("f{i}_shrunk"), u32::from(f.shrunk));
    }
    let owned: Vec<&str> = owned.iter().map(String::as_str).collect();
    super::common::put_client(entity, &owned, shown);
}

/// The chemicals clients know by index (the order of `CHEMICALS` of `runtime.rs`, which the fills and gauges use).
const CHEMICALS: &[&str] = &[
    "minecraft:water",
    "minecraft:lava",
    "mekanism:brine",
    "mekanism:heavy_water",
    "mekanism:hydrogen",
    "mekanism:oxygen",
    "mekanism:steam",
    "mekanism:nutritional_paste",
    "mekanism:chlorine",
    "mekanism:sodium",
    "mekanism:superheated_sodium",
    "mekanism:hydrogen_chloride",
    "mekanism:sulfur_dioxide",
    "mekanism:sulfur_trioxide",
    "mekanism:sulfuric_acid",
    "mekanism:ethene",
    "mekanism:lithium",
    "mekanism:uranium_oxide",
    "mekanism:uranium_hexafluoride",
    "mekanism:fissile_fuel",
    "mekanism:nuclear_waste",
    "mekanism:plutonium",
    "mekanism:polonium",
    "mekanism:antimatter",
    "mekanismgenerators:deuterium",
    "mekanismgenerators:tritium",
    "mekanismgenerators:fusion_fuel",
    "mekanism:aqua",
    "mekanism:black",
    "mekanism:blue",
    "mekanism:brown",
    "mekanism:cyan",
    "mekanism:dark_red",
    "mekanism:gray",
    "mekanism:green",
    "mekanism:light_blue",
    "mekanism:light_gray",
    "mekanism:lime",
    "mekanism:magenta",
    "mekanism:orange",
    "mekanism:pink",
    "mekanism:purple",
    "mekanism:red",
    "mekanism:white",
    "mekanism:yellow",
];

/// Index of a chemical in `CHEMICALS`, -1 for none or an unknown one.
pub(crate) fn chemical_index(id: Option<&str>) -> i32 {
    id.and_then(|id| CHEMICALS.iter().position(|c| *c == id)).map_or(-1, |i| i as i32)
}
