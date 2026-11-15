//! The Formulaic Assemblicator (`TileEntityFormulaicAssemblicator`, `RecipeFormula`): a crafting grid fed from
//! its own stock of ingredients, driven by an encoded crafting formula.

use std::collections::VecDeque;

pub const BLOCK: &str = "mekanism:formulaic_assemblicator";
pub const FORMULA_ITEM: &str = "mekanism:crafting_formula";
/// 5 FE a tick for `BASE_TICKS` ticks a craft.
pub const USAGE: u64 = 5;
pub const BASE_TICKS: u32 = 40;

/// The slots, in the order of its menu: the formula, 2 x 9 stock slots, the 3 x 3 grid, 3 x 2 outputs, energy.
pub const FORMULA_SLOT: usize = 0;
pub const INPUT_FIRST: usize = 1;
pub const INPUTS: usize = 18;
pub const GRID_FIRST: usize = 19;
pub const GRID: usize = 9;
pub const OUTPUT_FIRST: usize = 28;
pub const OUTPUTS: usize = 6;
pub const ENERGY_SLOT: usize = 34;
pub const SLOTS: usize = 35;

/// A formula's grid as the formula item stores it: nine item ids, empty for an empty cell.
pub fn encode(cells: &[Option<String>]) -> String {
    (0..GRID).map(|i| cells.get(i).cloned().flatten().unwrap_or_default()).collect::<Vec<_>>().join(",")
}

pub fn decode(text: &str) -> Vec<Option<String>> {
    let mut cells: Vec<Option<String>> = text.split(',').map(|c| (!c.is_empty()).then(|| c.to_owned())).collect();
    cells.resize(GRID, None);
    cells
}

/// `buildStockControlMap`: each ingredient of the formula, in order of first use, owns two stock slots for each
/// time the recipe uses it; the slots left over are uncontrolled.
pub fn stock_map(grid: &[Option<String>]) -> Vec<Option<String>> {
    let mut inputs: Vec<(&str, u32)> = vec![];
    for item in grid.iter().flatten() {
        match inputs.iter_mut().find(|(i, _)| i == item) {
            Some((_, n)) => *n += 1,
            None => inputs.push((item, 1)),
        }
    }
    let mut map: Vec<Option<String>> = vec![];
    for (item, n) in inputs {
        for _ in 0..n * 2 {
            map.push(Some(item.to_owned()));
        }
    }
    map.resize(INPUTS, None);
    map.truncate(INPUTS);
    map
}

/// `organizeStock`: the stock moves into the slots its formula assigns, the rest into the uncontrolled and then
/// the freed ones. None when it does not all fit (nothing changes then).
pub fn organize(
    slots: &[Option<(String, u32)>],
    map: &[Option<String>],
    max_stack: &dyn Fn(&str) -> u32,
) -> Option<Vec<Option<(String, u32)>>> {
    let mut stored: Vec<(String, u32)> = vec![];
    for (item, n) in slots.iter().flatten() {
        match stored.iter_mut().find(|(i, _)| i == item) {
            Some((_, total)) => *total += n,
            None => stored.push((item.clone(), *n)),
        }
    }
    let mut out: Vec<Option<(String, u32)>> = vec![None; slots.len()];
    let mut empty: VecDeque<usize> = VecDeque::new();
    for (i, slot) in out.iter_mut().enumerate() {
        match map.get(i).cloned().flatten() {
            None => empty.push_front(i),
            Some(item) => match stored.iter().position(|(s, _)| *s == item) {
                Some(at) => {
                    let count = max_stack(&item).min(stored[at].1);
                    if count == stored[at].1 {
                        stored.remove(at);
                    } else {
                        stored[at].1 -= count;
                    }
                    *slot = Some((item, count));
                }
                None => empty.push_back(i),
            },
        }
    }
    for (item, mut left) in stored {
        while left > 0 {
            let i = empty.pop_front()?;
            let count = max_stack(&item).min(left);
            out[i] = Some((item.clone(), count));
            left -= count;
        }
    }
    Some(out)
}

/// What a button asks of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Encode,
    StockControl,
    /// Fill the grid from the stock (a formula is loaded) or empty it into the stock (none is).
    FillEmpty,
    CraftSingle,
    CraftAll,
    Auto,
}

const OPS: [Op; 6] = [Op::Encode, Op::StockControl, Op::FillEmpty, Op::CraftSingle, Op::CraftAll, Op::Auto];

pub fn encode_request(op: Op, pos: [i32; 3]) -> Vec<u8> {
    let mut out = vec![OPS.iter().position(|o| *o == op).unwrap_or(0) as u8];
    for c in pos {
        out.extend(c.to_be_bytes());
    }
    out
}

pub fn decode_request(data: &[u8]) -> Option<(Op, [i32; 3])> {
    if data.len() != 13 {
        return None;
    }
    let int = |i: usize| i32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    Some((*OPS.get(usize::from(data[0]))?, [int(1), int(5), int(9)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_owned())
    }

    #[test]
    fn formula_round_trip() {
        let cells = vec![s("minecraft:stick"), None, s("minecraft:stick"), None, s("minecraft:iron_ingot"), None, None, None, None];
        assert_eq!(decode(&encode(&cells)), cells);
        assert_eq!(decode("").len(), 9);
    }

    #[test]
    fn stock_slots_per_ingredient() {
        let grid = vec![s("a"), s("b"), s("a"), None, None, None, None, None, None];
        let map = stock_map(&grid);
        assert_eq!(map.len(), 18);
        assert_eq!(&map[..6], &[s("a"), s("a"), s("a"), s("a"), s("b"), s("b")]);
        assert!(map[6..].iter().all(Option::is_none));
    }

    #[test]
    fn organize_moves_stock_to_its_slots() {
        let grid = vec![s("a"), s("b"), None, None, None, None, None, None, None];
        let map = stock_map(&grid);
        let mut slots: Vec<Option<(String, u32)>> = vec![None; 18];
        slots[5] = Some(("b".into(), 10));
        slots[17] = Some(("a".into(), 3));
        slots[9] = Some(("junk".into(), 1));
        let out = organize(&slots, &map, &|_| 64).unwrap();
        assert_eq!(out[0], Some(("a".into(), 3)));
        assert_eq!(out[2], Some(("b".into(), 10)));
        assert!(out.iter().flatten().any(|(i, _)| i == "junk"));
        // Too much for the slots: nothing fits, and nothing changes.
        let full: Vec<Option<(String, u32)>> = (0..18).map(|i| Some((format!("x{i}"), 1))).collect();
        assert!(organize(&full, &map, &|_| 64).is_some_and(|o| o.iter().flatten().count() == 18));
    }

    #[test]
    fn organize_splits_over_the_stack_size() {
        let map = stock_map(&[s("a"), None, None, None, None, None, None, None, None]);
        let mut slots: Vec<Option<(String, u32)>> = vec![None; 18];
        slots[10] = Some(("a".into(), 100));
        let out = organize(&slots, &map, &|_| 64).unwrap();
        assert_eq!(out[0], Some(("a".into(), 64)));
        assert_eq!(out[1], Some(("a".into(), 36)));
    }

    #[test]
    fn requests_round_trip() {
        assert_eq!(decode_request(&encode_request(Op::CraftAll, [1, -2, 3])), Some((Op::CraftAll, [1, -2, 3])));
        assert_eq!(decode_request(&[0; 5]), None);
    }
}
