// The Fluidic Plenisher (`TileEntityFluidicPlenisher`): the Electric Pump's opposite. It fills the
// space below it with a bucket of fluid every 20 ticks, spreading out and down from the block under it
// (never up), until it has filled `maxPlenisherNodes` spots or runs out of places; then it only keeps
// the block right under it filled.

use std::collections::HashSet;

/// `BASE_TICKS_REQUIRED`: a bucket every second, at 2 FE a tick (`usage.fluidicPlenisher`).
pub const PLENISHER_TICKS: u32 = 20;
pub const PLENISHER_USAGE: u64 = 2;
/// `MAX_FLUID`: 10 buckets.
pub const PLENISHER_TANK: u64 = 10_000;
/// `general.maxPlenisherNodes`.
pub const MAX_NODES: usize = 4_000;
/// A placed block of fluid takes a bucket.
pub const BUCKET: u64 = 1_000;

/// The sides the fill spreads to (`EnumSet.complementOf(UP)`, in `Direction` order).
pub const SPREAD: [[i32; 3]; 5] = [[0, -1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/// The block a fluid in the tank becomes (`canBePlacedInLevel`): only the fluids with a block.
pub fn placeable(fluid: &str) -> Option<&'static str> {
    match fluid {
        "minecraft:water" => Some("minecraft:water"),
        "minecraft:lava" => Some("minecraft:lava"),
        _ => None,
    }
}

/// What is at a spot, as `canReplace` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// Air, flowing fluid, or a block the fluid washes away.
    Free,
    /// A fluid source: the fill passes through it but does not place there.
    Source,
    /// Anything else, or an unloaded chunk.
    Blocked,
}

/// The fill's search state (`activeNodes`, `usedNodes`, `finishedCalc`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plenisher {
    /// The spots still to visit, in the order found (a linked set).
    pub active: Vec<[i32; 3]>,
    pub used: HashSet<[i32; 3]>,
    pub finished: bool,
}

impl Plenisher {
    fn can_replace(&self, at: [i32; 3], spot: Spot, check_nodes: bool, pathfinding: bool) -> bool {
        if check_nodes && self.used.contains(&at) {
            return false;
        }
        match spot {
            Spot::Free => true,
            Spot::Source => pathfinding,
            Spot::Blocked => false,
        }
    }

    /// One operation of a plenisher at `pos` (`doPlenish`, or the finished branch): the spot it
    /// places a bucket of fluid in, if any. `spot` reads the world.
    pub fn operate(&mut self, pos: [i32; 3], spot: impl Fn([i32; 3]) -> Spot) -> Option<[i32; 3]> {
        let below = [pos[0], pos[1] - 1, pos[2]];
        if self.finished {
            return self
                .can_replace(below, spot(below), false, false)
                .then_some(below);
        }
        if self.used.len() >= MAX_NODES {
            self.finished = true;
            return None;
        }
        if self.active.is_empty() {
            if !self.used.is_empty() || !self.can_replace(below, spot(below), true, true) {
                self.finished = true;
                return None;
            }
            self.active.push(below);
        }
        let node = self.active.remove(0);
        let place = self
            .can_replace(node, spot(node), true, false)
            .then_some(node);
        for d in SPREAD {
            let next = [node[0] + d[0], node[1] + d[1], node[2] + d[2]];
            if !self.active.contains(&next) && self.can_replace(next, spot(next), true, true) {
                self.active.push(next);
            }
        }
        self.used.insert(node);
        place
    }

    /// Sneak-right-click with an empty hand (`reset`): the next fill starts over from below.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The spots as one flat list of coordinates, for saving.
    pub fn flatten(spots: impl IntoIterator<Item = [i32; 3]>) -> Vec<i32> {
        spots.into_iter().flatten().collect()
    }

    /// The spots back from a flat list.
    pub fn unflatten(flat: &[i32]) -> Vec<[i32; 3]> {
        flat.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3×3 pit, two deep, below a plenisher at the origin; air above the pit's rim is blocked off.
    fn pit(at: [i32; 3]) -> Spot {
        if (-1..=1).contains(&at[0]) && (-1..=1).contains(&at[2]) && (-2..=-1).contains(&at[1]) {
            Spot::Free
        } else {
            Spot::Blocked
        }
    }

    #[test]
    fn fills_the_pit_once_then_keeps_the_block_below() {
        let mut p = Plenisher::default();
        let mut placed = Vec::new();
        for _ in 0..40 {
            if let Some(at) = p.operate([0, 0, 0], pit) {
                placed.push(at);
            }
            if p.finished {
                break;
            }
        }
        assert_eq!(placed.len(), 18);
        assert_eq!(placed[0], [0, -1, 0]);
        // Straight down first, as the spread order puts DOWN before the sides.
        assert_eq!(placed[1], [0, -2, 0]);
        assert!(p.finished);
        assert_eq!(p.operate([0, 0, 0], pit), Some([0, -1, 0]));
    }

    #[test]
    fn passes_through_sources_without_placing() {
        let mut p = Plenisher::default();
        let world = |at: [i32; 3]| match at {
            [0, -1, 0] => Spot::Source,
            [0, -2, 0] => Spot::Free,
            _ => Spot::Blocked,
        };
        assert_eq!(p.operate([0, 0, 0], world), None);
        assert_eq!(p.operate([0, 0, 0], world), Some([0, -2, 0]));
        assert_eq!(p.operate([0, 0, 0], world), None);
        assert!(p.finished);
    }

    #[test]
    fn blocked_below_finishes_at_once() {
        let mut p = Plenisher::default();
        assert_eq!(p.operate([0, 0, 0], |_| Spot::Blocked), None);
        assert!(p.finished);
        p.reset();
        assert!(!p.finished);
    }

    #[test]
    fn spots_round_trip() {
        let spots = vec![[1, -2, 3], [-4, 5, -6]];
        assert_eq!(
            Plenisher::unflatten(&Plenisher::flatten(spots.clone())),
            spots
        );
        assert_eq!(placeable("minecraft:lava"), Some("minecraft:lava"));
        assert_eq!(placeable("mekanism:heavy_water"), None);
    }
}
