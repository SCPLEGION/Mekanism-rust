// Bins (mekanism.common.tile.TileEntityBin, BinInventorySlot, BinTier) and personal storage
// (TileEntityPersonalStorage): a bin holds a great many of one item; a personal chest or barrel opens only
// for the player who placed it.

pub use crate::tier::BaseTier;

pub const PERSONAL_CHEST: &str = "mekanism:personal_chest";
pub const PERSONAL_BARREL: &str = "mekanism:personal_barrel";
/// `PersonalStorageItemInventory`: six rows of nine.
pub const PERSONAL_SLOTS: usize = 54;

/// The ticks after a right click in which another one moves every matching item of the player's
/// inventory in (`TileEntityBin.addTicks`).
pub const DOUBLE_CLICK_TICKS: u32 = 5;

/// `BinTier`: how many items a bin holds.
pub fn capacity(tier: BaseTier) -> u64 {
    let default = match tier {
        BaseTier::Basic => 4_096,
        BaseTier::Advanced => 8_192,
        BaseTier::Elite => 32_768,
        BaseTier::Ultimate => 262_144,
        BaseTier::Creative => i64::MAX as u64,
    };
    crate::config_files::active_int("tiers", "bins", &format!("{}Capacity", tier.name()), default)
}

/// The bin of a block id (`mekanism:elite_bin`).
pub fn tier_of(id: &str) -> Option<BaseTier> {
    let t = id.strip_prefix("mekanism:")?.strip_suffix("_bin")?;
    BaseTier::ALL.into_iter().find(|b| b.name() == t)
}

pub fn is_personal(id: &str) -> bool {
    id == PERSONAL_CHEST || id == PERSONAL_BARREL
}

/// A bin's one slot: the item, how many, and the item it is locked to (kept when it empties).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bin {
    pub item: Option<String>,
    pub count: u64,
    pub lock: Option<String>,
}

impl Bin {
    /// The item it holds or is locked to (`getBinItemType`).
    pub fn item_type(&self) -> Option<&str> {
        self.lock.as_deref().or(self.item.as_deref())
    }

    /// Puts up to `n` of `item` in (`BinInventorySlot.insert`) and says how many it took. A creative
    /// bin takes everything without changing; an empty one that a player fills becomes full of it, and
    /// for machines an empty creative bin is a void.
    pub fn insert(&mut self, tier: BaseTier, item: &str, n: u64, manual: bool) -> u64 {
        if n == 0 || self.item_type().is_some_and(|t| t != item) {
            return 0;
        }
        if tier == BaseTier::Creative {
            if self.count == 0 && manual {
                self.item = Some(item.to_owned());
                self.count = capacity(tier);
            }
            return n;
        }
        let taken = n.min(capacity(tier) - self.count);
        if taken > 0 {
            self.item = Some(item.to_owned());
            self.count += taken;
        }
        taken
    }

    /// Takes up to `n` out (a creative bin gives without emptying).
    pub fn extract(&mut self, tier: BaseTier, n: u64) -> Option<(String, u64)> {
        let item = self.item.clone().filter(|_| self.count > 0)?;
        let taken = n.min(self.count);
        if tier != BaseTier::Creative {
            self.count -= taken;
            if self.count == 0 {
                self.item = None;
            }
        }
        Some((item, taken))
    }

    /// Locks to what it holds, or unlocks (`setLocked`): false when nothing changes (a creative bin,
    /// or locking an empty one).
    pub fn toggle_lock(&mut self, tier: BaseTier) -> bool {
        if tier == BaseTier::Creative {
            return false;
        }
        if self.lock.is_some() {
            self.lock = None;
        } else if self.count > 0 {
            self.lock = self.item.clone();
        } else {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_to_capacity_with_one_item() {
        let mut b = Bin::default();
        assert_eq!(tier_of("mekanism:basic_bin"), Some(BaseTier::Basic));
        assert_eq!(
            b.insert(BaseTier::Basic, "minecraft:cobblestone", 4_000, false),
            4_000
        );
        assert_eq!(b.insert(BaseTier::Basic, "minecraft:dirt", 1, false), 0);
        assert_eq!(
            b.insert(BaseTier::Basic, "minecraft:cobblestone", 200, false),
            96
        );
        assert_eq!(
            b.extract(BaseTier::Basic, 64),
            Some(("minecraft:cobblestone".into(), 64))
        );
        assert_eq!(b.count, 4_032);
    }

    #[test]
    fn lock_keeps_the_type_when_empty() {
        let mut b = Bin::default();
        assert!(!b.toggle_lock(BaseTier::Elite));
        b.insert(BaseTier::Elite, "minecraft:iron_ingot", 3, true);
        assert!(b.toggle_lock(BaseTier::Elite));
        b.extract(BaseTier::Elite, 3);
        assert_eq!(b.item, None);
        assert_eq!(
            b.insert(BaseTier::Elite, "minecraft:gold_ingot", 1, true),
            0
        );
        assert_eq!(
            b.insert(BaseTier::Elite, "minecraft:iron_ingot", 1, true),
            1
        );
    }

    #[test]
    fn creative_bin_never_changes() {
        let mut b = Bin::default();
        assert_eq!(
            b.insert(BaseTier::Creative, "minecraft:diamond", 5, false),
            5
        );
        assert_eq!(b.count, 0);
        b.insert(BaseTier::Creative, "minecraft:diamond", 1, true);
        assert_eq!(b.count, capacity(BaseTier::Creative));
        b.extract(BaseTier::Creative, 64);
        assert_eq!(b.count, capacity(BaseTier::Creative));
        assert!(!b.toggle_lock(BaseTier::Creative));
    }
}
