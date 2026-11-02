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
    /// `round_robin`: saved and shown; one front inventory leaves nothing to take turns over.
    pub round_robin: bool,
    /// Filters switched off in the screen (`IFilter.isEnabled`), by their text.
    pub disabled: Vec<String>,
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

    /// The filters that work, in order.
    pub fn active(&self) -> impl Iterator<Item = &String> {
        self.filters.iter().filter(|f| !self.disabled.contains(f))
    }

    /// `FilterManager`'s moves (`MOVE_FILTER_UP`, `_DOWN`, `_TO_TOP`, `_TO_BOTTOM`): false if
    /// `index` is not a filter or it is already there.
    pub fn move_filter(&mut self, index: usize, op: Op) -> bool {
        let last = self.filters.len().saturating_sub(1);
        if index > last || self.filters.is_empty() {
            return false;
        }
        let to = match op {
            Op::Up if index > 0 => index - 1,
            Op::Down if index < last => index + 1,
            Op::Top if index > 0 => 0,
            Op::Bottom if index < last => last,
            _ => return false,
        };
        let f = self.filters.remove(index);
        self.filters.insert(to, f);
        true
    }

    /// `TOGGLE_FILTER_STATE`.
    pub fn toggle_enabled(&mut self, index: usize) -> bool {
        let Some(f) = self.filters.get(index) else {
            return false;
        };
        match self.disabled.iter().position(|d| d == f) {
            Some(at) => {
                self.disabled.remove(at);
            }
            None => self.disabled.push(f.clone()),
        }
        true
    }

    /// Takes filter `index` away.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.filters.len() {
            return false;
        }
        let f = self.filters.remove(index);
        self.disabled.retain(|d| *d != f);
        true
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

/// What the sorter's screen asks (`GuiInteraction`), with the filter it concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Single,
    RoundRobin,
    AutoEject,
    Up,
    Down,
    Top,
    Bottom,
    Toggle,
    Delete,
    /// A filter for the item in the player's hand, or its first tag.
    AddItem,
    AddTag,
}

const OPS: [Op; 11] = [
    Op::Single,
    Op::RoundRobin,
    Op::AutoEject,
    Op::Up,
    Op::Down,
    Op::Top,
    Op::Bottom,
    Op::Toggle,
    Op::Delete,
    Op::AddItem,
    Op::AddTag,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub op: Op,
    pub pos: [i32; 3],
    pub index: u16,
}

impl Request {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![OPS.iter().position(|o| *o == self.op).unwrap_or(0) as u8];
        for c in self.pos {
            out.extend(c.to_be_bytes());
        }
        out.extend(self.index.to_be_bytes());
        out
    }

    pub fn decode(data: &[u8]) -> Option<Request> {
        if data.len() != 15 {
            return None;
        }
        let op = *OPS.get(usize::from(data[0]))?;
        let int = |i: usize| i32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        Some(Request {
            op,
            pos: [int(1), int(5), int(9)],
            index: u16::from_be_bytes([data[13], data[14]]),
        })
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

    #[test]
    fn filters_move_toggle_and_go() {
        let mut s = Sorter {
            filters: vec!["a".into(), "b".into(), "c".into()],
            ..Sorter::default()
        };
        assert!(!s.move_filter(0, Op::Up));
        assert!(s.move_filter(2, Op::Top));
        assert_eq!(s.filters, ["c", "a", "b"]);
        assert!(s.move_filter(0, Op::Bottom));
        assert_eq!(s.filters, ["a", "b", "c"]);
        assert!(s.move_filter(0, Op::Down));
        assert_eq!(s.filters, ["b", "a", "c"]);
        assert!(s.toggle_enabled(1));
        assert_eq!(s.active().collect::<Vec<_>>(), ["b", "c"]);
        assert!(s.toggle_enabled(1));
        assert!(s.toggle_enabled(2));
        assert!(s.remove(2));
        assert!(s.disabled.is_empty());
        assert!(!s.remove(5));
    }

    #[test]
    fn requests_round_trip() {
        let r = Request {
            op: Op::Bottom,
            pos: [-3, 70, 9000],
            index: 40,
        };
        assert_eq!(Request::decode(&r.encode()), Some(r));
        assert_eq!(Request::decode(&[99; 15]), None);
        assert_eq!(Request::decode(&[0; 3]), None);
    }
}
