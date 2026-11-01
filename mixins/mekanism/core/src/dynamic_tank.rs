// The Dynamic Tank (mekanism.common.content.tank.TankMultiblockData, TankValidator): a box of dynamic
// tank casing, dynamic valves and structural glass, 3 to 18 on each side, air inside, holding one
// fluid or one chemical.

pub const MIN_SIZE: i32 = 3;
pub const MAX_SIZE: i32 = 18;
pub const CASING: &str = "mekanism:dynamic_tank";
pub const VALVE: &str = "mekanism:dynamic_valve";
pub const GLASS: &str = "mekanism:structural_glass";
/// `general.dynamicTank.mBPerTank`: mB for each block of the whole box (`MultiblockData.volume`).
pub const PER_BLOCK: u64 = 16_000 * 1_000;
/// What a valve takes from a pipe or tube in a tick before the tank empties it into itself.
pub const VALVE_BUFFER: u64 = 256_000;

pub fn is_frame(name: &str) -> bool {
    matches!(name, CASING | VALVE | GLASS)
}

/// `TankMultiblockData.setVolume`: the tank's capacity for a box of `volume` blocks.
pub fn capacity(volume: u64) -> u64 {
    volume.saturating_mul(PER_BLOCK)
}

/// The edges of the box need the casing itself (`CuboidStructureValidator`'s frame).
pub fn on_edge(rel: [i32; 3], size: [i32; 3]) -> bool {
    (0..3)
        .filter(|&k| rel[k] == 0 || rel[k] == size[k] - 1)
        .count()
        >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_3_cube_holds_432_buckets() {
        assert_eq!(capacity(27), 432_000_000);
        assert!(on_edge([0, 0, 1], [3, 3, 3]) && !on_edge([0, 1, 1], [3, 3, 3]));
        assert!(is_frame(GLASS) && !is_frame("minecraft:glass"));
    }
}
