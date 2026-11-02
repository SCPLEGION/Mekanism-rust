// The Logistical Sorter (`TileEntityLogisticalSorter`): every half second it takes items out of the
// inventory behind it and sends them into the transporters (or the inventory) in front. Its filters
// go first, in order, and the first one that moves anything ends the operation; with auto-eject on,
// items no filter names go too.

/// `TICKS_PER_HALF_SECOND`: the delay after each operation.
pub const DELAY: u32 = 10;
/// `Item.ABSOLUTE_MAX_STACK_SIZE`: what one operation moves, or 1 in single-item mode.
pub const MAX_MOVE: u32 = 64;

/// What a filter adds to its item, tag or mod id (`SorterFilter`): the transporter colour, whether
/// auto-eject may still send the items it names, and the item counts of one operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opts {
    pub allow_default: bool,
    /// `sizeMode`: only move between `min` and `max` items at once.
    pub size_mode: bool,
    pub min: u16,
    pub max: u16,
    /// A transporter colour (`EnumColor` ordinal), none for default.
    pub color: Option<u8>,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts {
            allow_default: false,
            size_mode: false,
            min: 0,
            max: 64,
            color: None,
        }
    }
}

impl Opts {
    /// `SorterFilter.setMinMax`'s rule: `0 <= min <= max <= 64`.
    pub fn valid_range(min: u16, max: u16) -> bool {
        min <= max && max <= u16::from(crate::sorter::MAX_MOVE as u8)
    }

    /// `TransporterUtils.increment` and `decrement` over none and the 16 dye colours.
    pub fn step_color(color: Option<u8>, next: bool) -> Option<u8> {
        match (color, next) {
            (None, true) => Some(0),
            (Some(15), true) => None,
            (Some(c), true) => Some(c + 1),
            (None, false) => Some(15),
            (Some(0), false) => None,
            (Some(c), false) => Some(c - 1),
        }
    }

    fn save(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            u8::from(self.allow_default),
            u8::from(self.size_mode),
            self.min,
            self.max,
            self.color.map_or(-1, i32::from)
        )
    }

    fn load(text: &str) -> Option<Opts> {
        let mut f = text.split('\t');
        let flag = |v: &str| v == "1";
        let o = Opts {
            allow_default: flag(f.next()?),
            size_mode: flag(f.next()?),
            min: f.next()?.parse().ok()?,
            max: f.next()?.parse().ok()?,
            color: f
                .next()?
                .parse::<i32>()
                .ok()
                .and_then(|c| u8::try_from(c).ok())
                .filter(|c| *c < 16),
        };
        Opts::valid_range(o.min, o.max).then_some(o)
    }
}

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
    /// The options of the filters that are not at their defaults, by filter text.
    pub opts: std::collections::BTreeMap<String, Opts>,
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

    /// A filter's options (the defaults when it has none).
    pub fn opts_of(&self, filter: &str) -> Opts {
        self.opts.get(filter).copied().unwrap_or_default()
    }

    /// Changes filter `index`'s options with `change`; false if there is no such filter.
    pub fn edit(&mut self, index: usize, change: impl FnOnce(&mut Opts)) -> bool {
        let Some(f) = self.filters.get(index).cloned() else {
            return false;
        };
        let mut o = self.opts_of(&f);
        change(&mut o);
        if o == Opts::default() {
            self.opts.remove(&f);
        } else {
            self.opts.insert(f, o);
        }
        true
    }

    /// The options saved in one string, a filter's text and its fields on each line.
    pub fn save_opts(&self) -> String {
        self.opts
            .iter()
            .map(|(k, o)| format!("{k}\t{}", o.save()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn parse_opts(saved: &str) -> std::collections::BTreeMap<String, Opts> {
        saved
            .lines()
            .filter_map(|l| {
                let (k, rest) = l.split_once('\t')?;
                Some((k.to_owned(), Opts::load(rest)?))
            })
            .collect()
    }

    /// `strictFinder`: whether auto-eject may send `item`: no enabled filter that names it may refuse
    /// the default.
    pub fn auto_eject_allows(&self, item: &str, in_tag: &dyn Fn(&str, &str) -> bool) -> bool {
        !self
            .active()
            .any(|f| !self.opts_of(f).allow_default && Sorter::matches_item(f, item, in_tag))
    }

    /// The filter a typed name makes: `kind` 0 an item id (`minecraft:` without a namespace), 1 a
    /// tag, 2 a mod id; none for a name that is empty, too long or has characters ids may not.
    pub fn named_filter(kind: u16, name: &str) -> Option<String> {
        let name = name.trim().trim_start_matches(['#', '@']);
        let ok = !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:".contains(c));
        if !ok {
            return None;
        }
        match kind {
            0 if name.contains(':') => Some(name.to_owned()),
            0 => Some(format!("minecraft:{name}")),
            1 => Some(format!("#{name}")),
            2 if !name.contains([':', '/']) => Some(format!("@{name}")),
            _ => None,
        }
    }

    /// Whether `filter` (an item id, `#tag` or `@mod id`) takes `item`.
    pub fn matches_item(filter: &str, item: &str, in_tag: &dyn Fn(&str, &str) -> bool) -> bool {
        match filter.as_bytes().first() {
            Some(b'#') => in_tag(&filter[1..], item),
            Some(b'@') => item
                .split_once(':')
                .is_some_and(|(ns, _)| ns == &filter[1..]),
            _ => filter == item,
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
        if !self.filters.contains(&f) {
            self.opts.remove(&f);
        }
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
    /// A filter for the mod id of the item in hand.
    AddMod,
    AllowDefault,
    SizeMode,
    ColorNext,
    ColorPrevious,
    ColorClear,
    /// The filter's range: `value` is `min << 8 | max`.
    Range,
    /// A filter made from a typed name: `index` says item (0), tag (1) or mod id (2).
    AddNamed,
    /// The colour: `value` is the dye + 1, or 0 for none.
    SetColor,
}

const OPS: [Op; 20] = [
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
    Op::AddMod,
    Op::AllowDefault,
    Op::SizeMode,
    Op::ColorNext,
    Op::ColorPrevious,
    Op::ColorClear,
    Op::Range,
    Op::AddNamed,
    Op::SetColor,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub op: Op,
    pub pos: [i32; 3],
    pub index: u16,
    /// The number a `Range` sets.
    pub value: u16,
    /// The typed name of an `AddNamed`.
    pub name: String,
}

impl Request {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![OPS.iter().position(|o| *o == self.op).unwrap_or(0) as u8];
        for c in self.pos {
            out.extend(c.to_be_bytes());
        }
        out.extend(self.index.to_be_bytes());
        out.extend(self.value.to_be_bytes());
        out.extend(self.name.as_bytes());
        out
    }

    pub fn decode(data: &[u8]) -> Option<Request> {
        if data.len() < 17 {
            return None;
        }
        let op = *OPS.get(usize::from(data[0]))?;
        let int = |i: usize| i32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        Some(Request {
            op,
            pos: [int(1), int(5), int(9)],
            index: u16::from_be_bytes([data[13], data[14]]),
            value: u16::from_be_bytes([data[15], data[16]]),
            name: std::str::from_utf8(&data[17..]).ok()?.to_owned(),
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
            value: 12,
            name: String::new(),
        };
        assert_eq!(Request::decode(&r.encode()), Some(r.clone()));
        let named = Request {
            op: Op::AddNamed,
            name: "c:ingots".into(),
            ..r
        };
        assert_eq!(Request::decode(&named.encode()), Some(named));
        assert_eq!(Request::decode(&[99; 17]), None);
        assert_eq!(Request::decode(&[0; 3]), None);
    }

    #[test]
    fn options_save_and_filters_match_mod_ids() {
        let mut s = Sorter {
            filters: vec!["@mekanism".into(), "minecraft:dirt".into()],
            ..Sorter::default()
        };
        assert!(s.edit(0, |o| {
            o.allow_default = true;
            o.min = 4;
            o.max = 32;
            o.color = Some(5);
        }));
        assert!(!s.edit(9, |_| {}));
        assert_eq!(Sorter::parse_opts(&s.save_opts()), s.opts);
        assert!(s.edit(0, |o| *o = Opts::default()));
        assert!(s.opts.is_empty());
        let tag = |_: &str, _: &str| false;
        assert!(Sorter::matches_item(
            "@mekanism",
            "mekanism:ingot_osmium",
            &tag
        ));
        assert!(!Sorter::matches_item("@mekanism", "mekanismtools:x", &tag));
        // Auto-eject skips what a filter names, unless that filter allows the default.
        assert!(!s.auto_eject_allows("minecraft:dirt", &tag));
        s.edit(1, |o| o.allow_default = true);
        assert!(s.auto_eject_allows("minecraft:dirt", &tag));
        assert_eq!(Opts::step_color(None, false), Some(15));
        assert_eq!(Opts::step_color(Some(15), true), None);
        assert!(!Opts::valid_range(5, 4) && !Opts::valid_range(0, 65));
    }

    #[test]
    fn typed_names_make_filters() {
        assert_eq!(
            Sorter::named_filter(0, "dirt").as_deref(),
            Some("minecraft:dirt")
        );
        assert_eq!(
            Sorter::named_filter(1, "c:ingots").as_deref(),
            Some("#c:ingots")
        );
        assert_eq!(
            Sorter::named_filter(2, "@mekanism").as_deref(),
            Some("@mekanism")
        );
        assert_eq!(Sorter::named_filter(2, "a:b"), None);
        assert_eq!(Sorter::named_filter(0, "Dirt"), None);
        assert_eq!(Sorter::named_filter(0, ""), None);
    }
}
