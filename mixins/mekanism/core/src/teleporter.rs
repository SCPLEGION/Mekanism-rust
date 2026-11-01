// The Teleporter (mekanism.common.tile.TileEntityTeleporter) and the Portable Teleporter: a teleporter
// block with a frame of teleporter frames around the two blocks in front of it sends players to the
// closest other teleporter on the same frequency, for energy by the distance.

pub const TELEPORTER: &str = "mekanism:teleporter";
pub const FRAME: &str = "mekanism:teleporter_frame";
pub const PORTABLE: &str = "mekanism:portable_teleporter";

/// `UsageConfig.teleporter`: `base`, `distance` (FE a block), `dimensionPenalty`; `StorageConfig`'s
/// `teleporter`; `GearConfig`'s portable teleporter `maxEnergy` and `chargeRate`.
pub const BASE_COST: u64 = 100;
pub const DISTANCE_COST: u64 = 2;
pub const DIMENSION_PENALTY: u64 = 1_000;
pub const STORAGE: u64 = 1_000;
pub const PORTABLE_CAPACITY: u64 = 1_000_000;
pub const PORTABLE_CHARGE_RATE: u64 = 5_000;

const DIRECTIONS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// The frame blocks `hasFrame` checks for the frame toward `d` from the teleporter, rotated or not:
/// a pair across each of the four layers, and the middle of the far end.
pub fn frame_offsets(d: [i32; 3], rotated: bool) -> Vec<[i32; 3]> {
    let alt = match (rotated, d) {
        (true, [_, _, z]) if z != 0 => [1, 0, 0],
        (true, _) => [0, 0, 1],
        (false, [_, y, _]) if y != 0 => [1, 0, 0],
        (false, _) => [0, 1, 0],
    };
    let mut out = vec![[3 * d[0], 3 * d[1], 3 * d[2]]];
    for k in 0..4 {
        for s in [-1, 1] {
            out.push([0, 1, 2].map(|i| k * d[i] + s * alt[i]));
        }
    }
    out
}

/// `getFrameDirection`: the first direction (in Mekanism's order) whose frame stands, unrotated first.
pub fn frame_direction(is_frame: impl Fn([i32; 3]) -> bool) -> Option<[i32; 3]> {
    DIRECTIONS.into_iter().find(|&d| {
        [false, true]
            .into_iter()
            .any(|rotated| frame_offsets(d, rotated).into_iter().all(&is_frame))
    })
}

/// `calculateEnergyCost` for one player in the same dimension: the base and 2 FE a block, rounded.
pub fn energy_cost(from: [f64; 3], to: [i32; 3], same_dimension: bool) -> u64 {
    let d = (0..3)
        .map(|k| (from[k] - f64::from(to[k])).powi(2))
        .sum::<f64>()
        .sqrt();
    let distance = (DISTANCE_COST as f64 * d).round() as u64;
    BASE_COST + distance + if same_dimension { 0 } else { DIMENSION_PENALTY }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_upright_frame_is_found_above() {
        let frame = frame_offsets([0, 1, 0], false);
        assert_eq!(frame.len(), 9);
        assert!(
            frame.contains(&[0, 3, 0]) && frame.contains(&[-1, 0, 0]) && frame.contains(&[1, 2, 0])
        );
        assert_eq!(frame_direction(|p| frame.contains(&p)), Some([0, 1, 0]));
        let turned = frame_offsets([0, 1, 0], true);
        assert!(turned.contains(&[0, 1, 1]));
        assert_eq!(frame_direction(|p| turned.contains(&p)), Some([0, 1, 0]));
        assert_eq!(frame_direction(|_| false), None);
    }

    #[test]
    fn a_hundred_blocks_cost_three_hundred_fe() {
        assert_eq!(energy_cost([0.0, 0.0, 0.0], [100, 0, 0], true), 300);
        assert_eq!(energy_cost([0.0, 0.0, 0.0], [0, 0, 0], false), 1_100);
    }
}
