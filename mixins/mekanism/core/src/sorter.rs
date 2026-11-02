// The Logistical Sorter (`TileEntityLogisticalSorter`): every half second it takes items out of the
// inventory behind it and sends them into the transporters (or the inventory) in front. Its filters
// go first, in order, and the first one that moves anything ends the operation; with auto-eject on,
// items no filter names go too.

/// `TICKS_PER_HALF_SECOND`: the delay after each operation.
pub const DELAY: u32 = 10;
/// `Item.ABSOLUTE_MAX_STACK_SIZE`: what one operation moves, or 1 in single-item mode.
pub const MAX_MOVE: u32 = 64;

/// The sorter's settings (`filters`, `autoEject`, `singleItem`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sorter {
    /// Item ids, or item tags written `#namespace:path`.
    pub filters: Vec<String>,
    pub auto_eject: bool,
    pub single_item: bool,
}

impl Sorter {
    /// The filters from their saved form, one per line.
    pub fn parse_filters(saved: &str) -> Vec<String> {
        saved
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()
    }

    pub fn save_filters(&self) -> String {
        self.filters.join("\n")
    }

    /// Adds a filter for `item`, or takes it away if there is one. True when it was added.
    pub fn toggle(&mut self, item: &str) -> bool {
        if let Some(at) = self.filters.iter().position(|f| f == item) {
            self.filters.remove(at);
            false
        } else {
            self.filters.push(item.to_owned());
            true
        }
    }

    /// How many items one operation moves.
    pub fn max_move(&self) -> u32 {
        if self.single_item { 1 } else { MAX_MOVE }
    }

    /// Whether `filter` takes `item`; `in_tag` says whether the item is in a tag.
    pub fn matches(filter: &str, item: &str, in_tag: &dyn Fn(&str) -> bool) -> bool {
        match filter.strip_prefix('#') {
            Some(tag) => in_tag(tag),
            None => filter == item,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_toggle_and_round_trip() {
        let mut s = Sorter::default();
        assert!(s.toggle("minecraft:cobblestone"));
        assert!(s.toggle("#c:ingots"));
        assert!(!s.toggle("minecraft:cobblestone"));
        assert_eq!(
            Sorter::parse_filters(&s.save_filters()),
            vec!["#c:ingots".to_owned()]
        );
        assert_eq!(Sorter::parse_filters(""), Vec::<String>::new());
    }

    #[test]
    fn matching_by_id_and_tag() {
        let none = |_: &str| false;
        assert!(Sorter::matches("minecraft:dirt", "minecraft:dirt", &none));
        assert!(!Sorter::matches("minecraft:dirt", "minecraft:stone", &none));
        assert!(Sorter::matches(
            "#c:ingots",
            "mekanism:ingot_osmium",
            &|t: &str| t == "c:ingots"
        ));
        assert_eq!(
            Sorter {
                single_item: true,
                ..Default::default()
            }
            .max_move(),
            1
        );
    }
}
