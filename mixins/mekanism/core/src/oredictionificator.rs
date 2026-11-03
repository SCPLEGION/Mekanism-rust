//! The Oredictionificator (`TileEntityOredictionificator`): a filter names an item tag, and the machine
//! turns an item of that tag into the filter's selected item of the same tag.

pub const BLOCK: &str = "mekanism:oredictionificator";

/// `GeneralConfig.validOredictionificatorFilters`: the tags a filter may name, by namespace and path prefix.
const NAMESPACE: &str = "c";
const PREFIXES: [&str; 6] = [
    "ingots/",
    "ores/",
    "dusts/",
    "nuggets/",
    "storage_blocks/",
    "raw_materials/",
];

/// `TileEntityOredictionificator.isPossibleFilter`: whether a tag (`namespace:path`) is one a filter may name.
pub fn valid_tag(tag: &str) -> bool {
    tag.split_once(':').is_some_and(|(namespace, path)| {
        namespace == NAMESPACE && PREFIXES.iter().any(|p| path.starts_with(p))
    })
}

/// What the filter window does with the text typed (`GuiOredictionificatorFilter.setText`): lower case, `c` as the
/// default namespace. The error is the window's status line.
pub fn parse_tag(text: &str) -> Result<String, &'static str> {
    if text.is_empty() {
        return Err("Tag filter must have a tag");
    }
    let text = text.to_lowercase();
    let mut parts = text.split(':');
    let (namespace, path) = match (parts.next(), parts.next(), parts.next()) {
        (Some(path), None, _) => ("c", path),
        (Some(namespace), Some(path), None) => (namespace, path),
        _ => return Err("Invalid path: it must not contain a colon"),
    };
    if namespace.contains('/') {
        return Err("Invalid namespace: it must not contain a slash");
    }
    Ok(format!("{namespace}:{path}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    pub enabled: bool,
    pub tag: String,
    /// The item the filter turns things into; when the tag does not hold it, the tag's first item.
    pub selected: String,
}

impl Filter {
    pub fn new(tag: &str) -> Self {
        Self {
            enabled: true,
            tag: tag.to_owned(),
            selected: String::new(),
        }
    }

    /// `OredictionificatorFilter.hasFilter`: it names a tag a filter may name.
    pub fn has_filter(&self) -> bool {
        valid_tag(&self.tag)
    }

    pub fn save(&self) -> String {
        format!("{}|{}|{}", u8::from(self.enabled), self.tag, self.selected)
    }

    pub fn load(line: &str) -> Option<Self> {
        let mut parts = line.splitn(3, '|');
        let enabled = parts.next()? == "1";
        let tag = parts.next()?.to_owned();
        let selected = parts.next()?.to_owned();
        Some(Self {
            enabled,
            tag,
            selected,
        })
    }

    /// `OredictionificatorFilter.getResult`: the selected item when the tag holds it, else the tag's first.
    pub fn result(&self, items: &[String]) -> Option<String> {
        if items.contains(&self.selected) {
            Some(self.selected.clone())
        } else {
            items.first().cloned()
        }
    }

    /// `OredictionificatorFilter.next` and `previous`: the selected item moves one along the tag's items.
    pub fn step(&mut self, items: &[String], forward: bool) {
        if items.len() < 2 {
            return;
        }
        let at = items.iter().position(|i| *i == self.selected);
        let next = match (at, forward) {
            (Some(i), true) => (i + 1) % items.len(),
            (None, true) => 0,
            (Some(0) | None, false) => items.len() - 1,
            (Some(i), false) => i - 1,
        };
        self.selected = items[next].clone();
    }
}

/// `TileEntityOredictionificator.getResult`: for each tag of the item, in order, the first enabled filter that
/// names it and has a result. `tag_items` gives a tag's items.
pub fn result(
    filters: &[Filter],
    item_tags: &[String],
    tag_items: &dyn Fn(&str) -> Vec<String>,
) -> Option<String> {
    for tag in item_tags.iter().filter(|t| valid_tag(t)) {
        for filter in filters.iter().filter(|f| f.enabled && f.tag == *tag) {
            if let Some(result) = filter.result(&tag_items(tag)) {
                return Some(result);
            }
        }
    }
    None
}

/// `TileEntityOredictionificator.isValidTarget`: a tag the registry knows and a filter may name.
pub fn valid_target(tag: &str, exists: bool) -> bool {
    exists && valid_tag(tag)
}

/// What a filter window or a row asks of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// A row's radio button.
    Toggle,
    /// The window's Delete button.
    Delete,
    /// The window's Save button: a new filter, or the edited one at `index`.
    Save,
}

const OPS: [Op; 3] = [Op::Toggle, Op::Delete, Op::Save];

/// The index a new filter is saved at.
pub const NEW: u16 = u16::MAX;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub op: Op,
    pub pos: [i32; 3],
    pub index: u16,
    pub tag: String,
    pub selected: String,
}

impl Request {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![OPS.iter().position(|o| *o == self.op).unwrap_or(0) as u8];
        for c in self.pos {
            out.extend(c.to_be_bytes());
        }
        out.extend(self.index.to_be_bytes());
        out.extend(format!("{}\n{}", self.tag, self.selected).as_bytes());
        out
    }

    pub fn decode(data: &[u8]) -> Option<Request> {
        if data.len() < 15 {
            return None;
        }
        let op = *OPS.get(usize::from(data[0]))?;
        let int = |i: usize| i32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        let text = std::str::from_utf8(&data[15..]).ok()?;
        let (tag, selected) = text.split_once('\n')?;
        Some(Request {
            op,
            pos: [int(1), int(5), int(9)],
            index: u16::from_be_bytes([data[13], data[14]]),
            tag: tag.to_owned(),
            selected: selected.to_owned(),
        })
    }
}

/// A Save: the new filter is added (unless the machine has it), or replaces the one at `index`
/// (`FilterManager.addFilter` and `editFilter`). Whether the list changed.
pub fn save(filters: &mut Vec<Filter>, index: Option<usize>, new: Filter) -> bool {
    match index {
        None if filters.iter().any(|f| f.tag == new.tag && f.selected == new.selected) => false,
        None => {
            filters.push(new);
            true
        }
        Some(i) if i < filters.len() => {
            filters[i] = new;
            true
        }
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iron() -> Vec<String> {
        vec![
            "minecraft:iron_ingot".to_owned(),
            "mekanism:ingot_iron".to_owned(),
        ]
    }

    #[test]
    fn tags_a_filter_may_name() {
        assert!(valid_tag("c:ingots/iron"));
        assert!(valid_tag("c:storage_blocks/gold"));
        assert!(!valid_tag("c:gems/diamond"));
        assert!(!valid_tag("minecraft:logs"));
    }

    #[test]
    fn typed_text_becomes_a_tag() {
        assert_eq!(parse_tag("Ingots/Iron"), Ok("c:ingots/iron".to_owned()));
        assert_eq!(parse_tag("forge:ores/tin"), Ok("forge:ores/tin".to_owned()));
        assert!(parse_tag("").is_err());
        assert!(parse_tag("a/b:c").is_err());
        assert!(parse_tag("a:b:c").is_err());
    }

    #[test]
    fn the_result_is_the_selected_item_or_the_first() {
        let mut f = Filter::new("c:ingots/iron");
        assert_eq!(f.result(&iron()).as_deref(), Some("minecraft:iron_ingot"));
        f.step(&iron(), true);
        assert_eq!(f.result(&iron()).as_deref(), Some("minecraft:iron_ingot"));
        f.step(&iron(), true);
        assert_eq!(f.selected, "mekanism:ingot_iron");
        f.step(&iron(), true);
        assert_eq!(f.selected, "minecraft:iron_ingot");
        f.step(&iron(), false);
        assert_eq!(f.selected, "mekanism:ingot_iron");
        assert_eq!(f.result(&[]), None);
    }

    #[test]
    fn filters_are_tried_in_the_items_tag_order() {
        let mut off = Filter::new("c:ingots/iron");
        off.enabled = false;
        let on = Filter {
            selected: "mekanism:ingot_iron".to_owned(),
            ..Filter::new("c:ingots/iron")
        };
        let tags = vec!["c:ores/iron".to_owned(), "c:ingots/iron".to_owned()];
        let items = |_: &str| iron();
        assert_eq!(result(&[off.clone()], &tags, &items), None);
        assert_eq!(
            result(&[off, on], &tags, &items).as_deref(),
            Some("mekanism:ingot_iron")
        );
    }

    #[test]
    fn filters_round_trip() {
        let f = Filter {
            enabled: false,
            tag: "c:dusts/gold".to_owned(),
            selected: "mekanism:dust_gold".to_owned(),
        };
        assert_eq!(Filter::load(&f.save()), Some(f));
    }

    #[test]
    fn requests_round_trip() {
        let r = Request { op: Op::Save, pos: [1, -2, 3], index: NEW, tag: "c:ingots/iron".into(), selected: "minecraft:iron_ingot".into() };
        assert_eq!(Request::decode(&r.encode()), Some(r));
        assert_eq!(Request::decode(&[9; 20]), None);
    }

    #[test]
    fn save_adds_or_replaces() {
        let mut f = vec![Filter::new("c:ingots/iron")];
        assert!(!save(&mut f, None, Filter::new("c:ingots/iron")));
        assert!(save(&mut f, None, Filter::new("c:ingots/gold")));
        assert!(save(&mut f, Some(0), Filter::new("c:ores/iron")));
        assert!(!save(&mut f, Some(5), Filter::new("c:ores/iron")));
        assert_eq!(f[0].tag, "c:ores/iron");
        assert!(valid_target("c:ores/iron", true) && !valid_target("c:ores/iron", false));
    }
}
