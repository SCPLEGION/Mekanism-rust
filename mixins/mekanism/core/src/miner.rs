// The Digital Miner (mekanism.common.tile.machine.TileEntityDigitalMiner), with its defaults: radius 10,
// Y 0 to 60, a block every 80 ticks (`minerTicksPerMine`) at 40 FE/t (`UsageConfig.digitalMiner`).

pub const RADIUS: i32 = 10;
pub const MIN_Y: i32 = 0;
pub const MAX_Y: i32 = 60;
pub const TICKS_PER_MINE: u32 = 80;
pub const ENERGY_USAGE: u64 = 40;
/// The energy it holds: 50 kFE, enough for a little over a minute of mining.
pub const STORAGE: u64 = 50_000;
/// How many positions one mining step looks at before it gives up until the next (Mekanism searches the
/// whole region on a thread first).
pub const SCAN_LIMIT: u32 = 4_096;
/// The block tag this port mines: Mekanism mines what its filters name; ores are what players set first.
pub const ORE_TAG: &str = "c:ores";

pub fn diameter() -> i32 {
    RADIUS * 2 + 1
}

/// `getTotalSize`: every position of the region.
pub fn total_size() -> u32 {
    (diameter() * diameter() * (MAX_Y - MIN_Y + 1)) as u32
}

/// `getOffsetForIndex` from `getStartingPos`: x fastest, then z, then y.
pub fn position(miner: [i32; 3], index: u32) -> [i32; 3] {
    let d = diameter() as u32;
    let start = [miner[0] - RADIUS, MIN_Y, miner[2] - RADIUS];
    [
        start[0] + (index % d) as i32,
        start[1] + (index / d / d) as i32,
        start[2] + ((index / d) % d) as i32,
    ]
}

/// The positions one step checks, from `cursor` (counted from the top of the region down, as Mekanism
/// mines from the highest index so falling blocks land where it will reach them), as region indices.
pub fn scan_order(cursor: u32, limit: u32) -> impl Iterator<Item = u32> {
    let total = total_size();
    (0..limit.min(total)).map(move |k| total - 1 - (cursor + k) % total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_region_is_21_by_21_by_61() {
        assert_eq!(total_size(), 21 * 21 * 61);
        assert_eq!(position([100, 64, -50], 0), [90, 0, -60]);
        assert_eq!(position([100, 64, -50], total_size() - 1), [110, 60, -40]);
        assert_eq!(position([0, 0, 0], 21), [-10, 0, -9]);
    }

    #[test]
    fn scanning_starts_at_the_top_and_wraps() {
        let first: Vec<u32> = scan_order(0, 2).collect();
        assert_eq!(first, [total_size() - 1, total_size() - 2]);
        let wrapped: Vec<u32> = scan_order(total_size() - 1, 2).collect();
        assert_eq!(wrapped, [0, total_size() - 1]);
    }
}
