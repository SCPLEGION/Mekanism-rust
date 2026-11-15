//! The Robit (`EntityRobit`): its numbers and how it follows its owner.

/// The mob type the mod adds, and the vanilla one it is networked as.
pub const TYPE: &str = "mekanism:robit";
pub const BASE: &str = "minecraft:silverfish";
pub const ITEM: &str = "mekanism:robit";

/// `EntityRobit.MAX_ENERGY`.
pub const MAX_ENERGY: u64 = 100_000;
/// `EntityRobit.inventorySlots`: 27 storage slots, the energy slot, the smelting input and output.
pub const INVENTORY_SLOTS: usize = 43;
/// The repair window's anvil input, material and result (the result is computed, never stored).
pub const REPAIR_INPUT: usize = 40;
pub const REPAIR_ADD: usize = 41;
pub const REPAIR_RESULT: usize = 42;
/// The crafting window's 3x3 grid (slots 30 to 38) and its result, which is computed, never stored.
pub const CRAFT_GRID: usize = 30;
pub const CRAFT_RESULT: usize = 39;
/// The slots after the 27 of storage (`EntityRobit`'s constructor).
pub const ENERGY_SLOT: usize = 27;
pub const SMELT_INPUT: usize = 28;
pub const SMELT_OUTPUT: usize = 29;
/// `ticksRequired`: five seconds, at `usage.energizedSmelter`'s 5 FE a tick.
pub const SMELT_TICKS: u32 = 100;
pub const SMELT_USAGE: u64 = 5;
/// `DISTANCE_MULTIPLIER`: energy per block walked.
const DISTANCE_MULTIPLIER: f64 = 1.5;
/// `RobitAIFollow(this, 1, 4, 2)`: starts walking from 4 blocks, stops at 2.
pub const FOLLOW_START: f64 = 4.0;
pub const FOLLOW_STOP: f64 = 2.0;
/// Further than this from its owner it is moved next to them (a stand-in for the navigator's teleport).
pub const TELEPORT: f64 = 12.0;
/// Blocks a tick while following (`MOVEMENT_SPEED` 0.3 with 1.0 speed on the ground).
pub const WALK_SPEED: f64 = 0.2;
/// `getBoundingBox().inflate(1.5, 1.5, 1.5)`.
pub const PICKUP_REACH: f64 = 1.5;

/// `getRoundedTravelEnergy`: the energy a tick's walk costs.
pub fn travel_energy(distance: f64) -> u64 {
    (DISTANCE_MULTIPLIER * distance).ceil() as u64
}

/// What the follow goal does this tick: whether it is walking after this tick, and the step
/// `(dx, dz)` to take. `moving` is whether it walked last tick.
pub fn follow(moving: bool, to_owner: [f64; 2], energy: u64) -> (bool, [f64; 2]) {
    let d2 = to_owner[0] * to_owner[0] + to_owner[1] * to_owner[1];
    let go = energy > 0 && if moving { d2 > FOLLOW_STOP * FOLLOW_STOP } else { d2 >= FOLLOW_START * FOLLOW_START };
    if !go {
        return (false, [0.0, 0.0]);
    }
    let d = d2.sqrt();
    (true, [to_owner[0] / d * WALK_SPEED, to_owner[1] / d * WALK_SPEED])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_follows_from_four_blocks_and_stops_at_two() {
        assert!(!follow(false, [3.0, 0.0], 10).0);
        assert!(follow(false, [4.0, 0.0], 10).0);
        assert!(follow(true, [3.0, 0.0], 10).0);
        assert!(!follow(true, [1.9, 0.0], 10).0);
        assert!(!follow(false, [9.0, 0.0], 0).0);
    }

    #[test]
    fn walking_costs_energy_by_distance() {
        assert_eq!(travel_energy(0.2), 1);
        assert_eq!(travel_energy(2.0), 3);
        let (_, step) = follow(false, [0.0, 10.0], 5);
        assert!((step[1] - WALK_SPEED).abs() < 1e-9);
    }
}
