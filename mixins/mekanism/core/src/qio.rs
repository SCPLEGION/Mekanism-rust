// Quantum Item Orchestration (mekanism.common.content.qio): drives in drive arrays hold items for every
// QIO block on the same frequency. The dashboard shows and gives them, importers put items in and
// exporters take them out.

use std::collections::BTreeMap;

pub const DRIVE_ARRAY: &str = "mekanism:qio_drive_array";
pub const DASHBOARD: &str = "mekanism:qio_dashboard";
pub const IMPORTER: &str = "mekanism:qio_importer";
pub const EXPORTER: &str = "mekanism:qio_exporter";
pub const REDSTONE_ADAPTER: &str = "mekanism:qio_redstone_adapter";
/// `TileEntityQIODriveArray`: twelve drive slots.
pub const DRIVE_SLOTS: usize = 12;
/// The dashboard's view: six rows of nine.
pub const DASHBOARD_SLOTS: usize = 54;
/// `TileEntityQIOFilterHandler.BASE_TRANSIT_COUNT`, each half second (`MAX_DELAY`).
pub const TRANSIT_COUNT: u32 = 64;
pub const TRANSIT_DELAY: u32 = 10;

/// `QIODriveTier`: how many items and how many types a drive holds.
pub fn drive_tier(item: &str) -> Option<(u64, usize)> {
    match item {
        "mekanism:qio_drive_base" => Some((16_000, 128)),
        "mekanism:qio_drive_hyper_dense" => Some((128_000, 256)),
        "mekanism:qio_drive_time_dilating" => Some((1_048_000, 1_024)),
        "mekanism:qio_drive_supermassive" => Some((16_000_000_000, 8_192)),
        _ => None,
    }
}

/// One drive (`QIODriveData`): its limits and what it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drive {
    pub count_limit: u64,
    pub type_limit: usize,
    pub items: BTreeMap<String, u64>,
}

impl Drive {
    pub fn total(&self) -> u64 {
        self.items.values().sum()
    }

    fn room(&self, item: &str) -> u64 {
        if !self.items.contains_key(item) && self.items.len() >= self.type_limit {
            return 0;
        }
        self.count_limit.saturating_sub(self.total())
    }
}

/// The drives of a frequency (`QIOFrequency`), in their arrays' order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Network {
    pub drives: Vec<Drive>,
}

impl Network {
    /// Puts up to `n` of `item` in (`QIOFrequency.massInsert`): first into drives that already hold it,
    /// then into any with room. Returns how many went in.
    pub fn insert(&mut self, item: &str, n: u64) -> u64 {
        let mut left = n;
        for pass in [true, false] {
            for d in &mut self.drives {
                if left == 0 {
                    return n;
                }
                if d.items.contains_key(item) != pass {
                    continue;
                }
                let put = left.min(d.room(item));
                if put > 0 {
                    *d.items.entry(item.to_owned()).or_default() += put;
                    left -= put;
                }
            }
        }
        n - left
    }

    /// Takes up to `n` of `item` out (`QIOFrequency.massExtract`).
    pub fn extract(&mut self, item: &str, n: u64) -> u64 {
        let mut left = n;
        for d in &mut self.drives {
            let Some(have) = d.items.get_mut(item) else {
                continue;
            };
            let take = left.min(*have);
            *have -= take;
            left -= take;
            if *have == 0 {
                d.items.remove(item);
            }
            if left == 0 {
                break;
            }
        }
        n - left
    }

    /// Every item and how many, the most first, then by id (the dashboard's default sort).
    pub fn list(&self) -> Vec<(String, u64)> {
        let mut all: BTreeMap<&str, u64> = BTreeMap::new();
        for d in &self.drives {
            for (k, v) in &d.items {
                *all.entry(k).or_default() += v;
            }
        }
        let mut out: Vec<(String, u64)> = all.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// How many of `item` the drives hold (`QIOFrequency.getStored`).
    pub fn stored(&self, item: &str) -> u64 {
        self.drives.iter().filter_map(|d| d.items.get(item)).sum()
    }

    pub fn totals(&self) -> (u64, u64, usize, usize) {
        let types: std::collections::BTreeSet<&String> =
            self.drives.iter().flat_map(|d| d.items.keys()).collect();
        (
            self.drives.iter().map(Drive::total).sum(),
            self.drives.iter().map(|d| d.count_limit).sum(),
            types.len(),
            self.drives.iter().map(|d| d.type_limit).sum(),
        )
    }
}

/// Whether a redstone adapter emits: some of its item, at least `count` of it, unless inverted.
pub fn adapter_emits(stored: u64, count: u64, inverted: bool) -> bool {
    (stored > 0 && stored >= count) != inverted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(item: &str) -> Drive {
        let (count_limit, type_limit) = drive_tier(item).unwrap();
        Drive {
            count_limit,
            type_limit,
            items: BTreeMap::new(),
        }
    }

    #[test]
    fn fills_drives_holding_the_item_first() {
        let mut n = Network {
            drives: vec![
                drive("mekanism:qio_drive_base"),
                drive("mekanism:qio_drive_base"),
            ],
        };
        n.drives[1].items.insert("minecraft:stone".into(), 10);
        assert_eq!(n.insert("minecraft:stone", 100), 100);
        assert_eq!(n.drives[1].items["minecraft:stone"], 110);
        assert!(n.drives[0].items.is_empty());
        assert_eq!(n.insert("minecraft:stone", 20_000), 20_000);
        assert_eq!(n.totals().0, 20_110);
        assert_eq!(n.insert("minecraft:stone", 20_000), 32_000 - 20_110);
    }

    #[test]
    fn type_limit_and_extract() {
        let mut n = Network {
            drives: vec![Drive {
                count_limit: 100,
                type_limit: 1,
                items: BTreeMap::new(),
            }],
        };
        assert_eq!(n.insert("a", 5), 5);
        assert_eq!(n.insert("b", 5), 0);
        assert_eq!(n.extract("a", 9), 5);
        assert!(n.drives[0].items.is_empty());
        assert_eq!(n.insert("b", 5), 5);
        assert_eq!(n.list(), vec![("b".to_owned(), 5)]);
        assert_eq!(n.stored("b"), 5);
    }

    #[test]
    fn adapter_threshold() {
        assert!(!adapter_emits(0, 0, false));
        assert!(adapter_emits(1, 0, false));
        assert!(!adapter_emits(9, 10, false));
        assert!(adapter_emits(10, 10, false));
        assert!(adapter_emits(9, 10, true));
    }
}
