//! The Dimensional Stabilizer (`TileEntityDimensionalStabilizer`): a 5 x 5 grid of chunks around its own, each of
//! which it can keep loaded for 15 FE a tick apiece. The centre chunk is always loaded.

pub const BLOCK: &str = "mekanism:dimensional_stabilizer";
pub const MAX_LOAD_RADIUS: i32 = 2;
pub const DIAMETER: usize = (2 * MAX_LOAD_RADIUS + 1) as usize;
pub const CELLS: usize = DIAMETER * DIAMETER;
/// The energy a chunk costs each tick (`dimensionalStabilizer`).
pub const USAGE: u64 = 15;
/// `AttributeEnergy.STORAGE_MULTIPLIER` ticks of one chunk's usage.
pub const STORAGE: u64 = 4 * USAGE;

/// Which chunks load, indexed `x * DIAMETER + z` (`loadingChunks[x][z]`; x east, z south, the centre at 2, 2).
pub type Grid = [bool; CELLS];

const CENTRE: usize = (MAX_LOAD_RADIUS as usize) * DIAMETER + MAX_LOAD_RADIUS as usize;

/// Only the stabilizer's own chunk.
pub fn new_grid() -> Grid {
    let mut grid = [false; CELLS];
    grid[CENTRE] = true;
    grid
}

pub fn encode(grid: &Grid) -> String {
    grid.iter().map(|&b| if b { '1' } else { '0' }).collect()
}

pub fn decode(text: &str) -> Grid {
    let mut grid = [false; CELLS];
    for (cell, c) in grid.iter_mut().zip(text.chars()) {
        *cell = c == '1';
    }
    grid[CENTRE] = true;
    grid
}

/// The chunks the grid loads around the stabilizer's chunk `(cx, cz)` (`getChunkSet`).
pub fn chunk_set(grid: &Grid, cx: i32, cz: i32) -> Vec<(i32, i32)> {
    let mut out = vec![];
    for x in 0..DIAMETER {
        for z in 0..DIAMETER {
            if grid[x * DIAMETER + z] {
                out.push((cx + x as i32 - MAX_LOAD_RADIUS, cz + z as i32 - MAX_LOAD_RADIUS));
            }
        }
    }
    out
}

pub fn chunks_loaded(grid: &Grid) -> u64 {
    grid.iter().filter(|&&b| b).count() as u64
}

/// `FixedUsageEnergyContainer`: the usage times the chunks loaded.
pub fn energy_per_tick(grid: &Grid) -> u64 {
    USAGE * chunks_loaded(grid)
}

fn set(grid: &mut Grid, x: usize, z: usize, load: bool) -> bool {
    let i = x * DIAMETER + z;
    if i == CENTRE || grid[i] == load {
        return false;
    }
    grid[i] = load;
    true
}

/// `toggleChunkLoadingAt` (`packetTarget = x * DIAMETER + z`): whether anything changed.
pub fn toggle(grid: &mut Grid, index: usize) -> bool {
    if index >= CELLS {
        return false;
    }
    let load = !grid[index];
    set(grid, index / DIAMETER, index % DIAMETER, load)
}

/// The cells on the ring at `radius` from the centre.
fn ring(radius: i32) -> Vec<(usize, usize)> {
    let mut cells = vec![];
    for x in -radius..=radius {
        let inner = x > -radius && x < radius;
        let step = if inner { 2 * radius } else { 1 };
        let mut z = -radius;
        while z <= radius {
            cells.push(((x + MAX_LOAD_RADIUS) as usize, (z + MAX_LOAD_RADIUS) as usize));
            z += step;
        }
    }
    cells
}

/// `GuiDimensionalStabilizer.hasAtRadius`: some chunk on the ring is in `state`.
pub fn has_at_radius(grid: &Grid, radius: i32, state: bool) -> bool {
    ring(radius).into_iter().any(|(x, z)| grid[x * DIAMETER + z] == state)
}

/// `adjustChunkLoadingRadius`: the whole ring loads or unloads. Whether anything changed.
pub fn adjust_radius(grid: &mut Grid, radius: i32, load: bool) -> bool {
    if radius <= 0 || radius > MAX_LOAD_RADIUS {
        return false;
    }
    let mut changed = false;
    for (x, z) in ring(radius) {
        changed |= set(grid, x, z, load);
    }
    changed
}

/// The centre button's left click: the smallest ring with an unloaded chunk (`ENABLE_RADIUS_CHUNKLOAD`).
pub fn next_to_enable(grid: &Grid) -> Option<i32> {
    (1..=MAX_LOAD_RADIUS).find(|&r| has_at_radius(grid, r, false))
}

/// The centre button's right click: the largest ring with a loaded chunk (`DISABLE_RADIUS_CHUNKLOAD`).
pub fn next_to_disable(grid: &Grid) -> Option<i32> {
    (1..=MAX_LOAD_RADIUS).rev().find(|&r| has_at_radius(grid, r, true))
}

/// The faces of a loaded chunk's column that show (east, west, south, north): the ones with no loaded chunk beside.
pub fn exposed(grid: &Grid, x: usize, z: usize) -> [bool; 4] {
    let loaded = |dx: i32, dz: i32| {
        let (nx, nz) = (x as i32 + dx, z as i32 + dz);
        (0..DIAMETER as i32).contains(&nx) && (0..DIAMETER as i32).contains(&nz) && grid[nx as usize * DIAMETER + nz as usize]
    };
    [!loaded(1, 0), !loaded(-1, 0), !loaded(0, 1), !loaded(0, -1)]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Toggle(u8),
    Enable(u8),
    Disable(u8),
}

pub fn encode_request(op: Op, pos: [i32; 3]) -> Vec<u8> {
    let (tag, arg) = match op {
        Op::Toggle(i) => (0, i),
        Op::Enable(r) => (1, r),
        Op::Disable(r) => (2, r),
    };
    let mut out = vec![tag, arg];
    for c in pos {
        out.extend(c.to_be_bytes());
    }
    out
}

pub fn decode_request(data: &[u8]) -> Option<(Op, [i32; 3])> {
    if data.len() != 14 {
        return None;
    }
    let arg = data[1];
    let op = match data[0] {
        0 => Op::Toggle(arg),
        1 => Op::Enable(arg),
        2 => Op::Disable(arg),
        _ => return None,
    };
    let int = |i: usize| i32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    Some((op, [int(2), int(6), int(10)]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_chunk_set_surrounds_the_stabilizer() {
        let mut g = super::new_grid();
        assert_eq!(super::chunk_set(&g, 10, -3), vec![(10, -3)]);
        g[0] = true;
        assert!(super::chunk_set(&g, 10, -3).contains(&(8, -5)));
    }

    use super::*;

    #[test]
    fn only_the_centre_loads_at_first() {
        let g = new_grid();
        assert_eq!(chunks_loaded(&g), 1);
        assert_eq!(energy_per_tick(&g), 15);
        assert_eq!(decode(&encode(&g)), g);
    }

    #[test]
    fn the_centre_cannot_be_toggled() {
        let mut g = new_grid();
        assert!(!toggle(&mut g, CENTRE));
        assert!(toggle(&mut g, 0));
        assert!(g[0] && chunks_loaded(&g) == 2);
        assert!(toggle(&mut g, 0));
        assert!(!toggle(&mut g, 99));
    }

    #[test]
    fn rings_load_and_unload() {
        let mut g = new_grid();
        assert_eq!(next_to_enable(&g), Some(1));
        assert!(adjust_radius(&mut g, 1, true));
        assert_eq!(chunks_loaded(&g), 9);
        assert_eq!(next_to_enable(&g), Some(2));
        assert!(adjust_radius(&mut g, 2, true));
        assert_eq!(chunks_loaded(&g), 25);
        assert_eq!(next_to_enable(&g), None);
        assert_eq!(next_to_disable(&g), Some(2));
        assert!(adjust_radius(&mut g, 2, false));
        assert_eq!(next_to_disable(&g), Some(1));
        assert!(!adjust_radius(&mut g, 3, true));
    }

    #[test]
    fn a_ring_of_two_has_sixteen_chunks() {
        assert_eq!(ring(2).len(), 16);
        assert_eq!(ring(1).len(), 8);
    }

    #[test]
    fn faces_between_loaded_chunks_hide() {
        let mut g = new_grid();
        assert_eq!(exposed(&g, 2, 2), [true; 4]);
        toggle(&mut g, 3 * DIAMETER + 2);
        assert_eq!(exposed(&g, 2, 2), [false, true, true, true]);
        assert_eq!(exposed(&g, 3, 2), [true, false, true, true]);
    }

    #[test]
    fn requests_round_trip() {
        for op in [Op::Toggle(7), Op::Enable(1), Op::Disable(2)] {
            assert_eq!(decode_request(&encode_request(op, [1, -2, 3])), Some((op, [1, -2, 3])));
        }
        assert_eq!(decode_request(&[9; 14]), None);
    }
}
