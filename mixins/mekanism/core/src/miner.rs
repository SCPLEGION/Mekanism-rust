// The Digital Miner (mekanism.common.tile.machine.TileEntityDigitalMiner), with its defaults: radius 10,
// Y 0 to 60, a block every 80 ticks (`minerTicksPerMine`) at 40 FE/t (`UsageConfig.digitalMiner`).

pub const RADIUS: i32 = 10;
pub const MIN_Y: i32 = 0;
pub const MAX_Y: i32 = 60;
/// `minerMaxRadius`.
pub const MAX_RADIUS: i32 = 32;
/// The overworld's lowest and highest block Y (`Level.getMinY`, `getMaxY`).
pub const WORLD_MIN_Y: i32 = -64;
pub const WORLD_MAX_Y: i32 = 319;
/// `minerSilkMultiplier`.
pub const SILK_MULTIPLIER: u64 = 6;
pub const TICKS_PER_MINE: u32 = 80;
pub const ENERGY_USAGE: u64 = 40;
/// The energy it holds: 50 kFE, enough for a little over a minute of mining.
pub const STORAGE: u64 = 50_000;
/// How many positions one search step looks at (Mekanism searches the whole region on a thread).
pub const SCAN_LIMIT: u32 = 4_096;
/// The block tag the miner mines until it has a filter: ores are what players set first.
pub const ORE_TAG: &str = "c:ores";

/// The region a miner works: `radius` blocks around it, from `min_y` to `max_y` inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub radius: i32,
    pub min_y: i32,
    pub max_y: i32,
}

impl Default for Region {
    fn default() -> Self {
        Region {
            radius: RADIUS,
            min_y: MIN_Y,
            max_y: MAX_Y,
        }
    }
}

impl Region {
    pub fn diameter(&self) -> i32 {
        self.radius * 2 + 1
    }

    /// `getTotalSize`: every position of the region.
    pub fn total_size(&self) -> u32 {
        (self.diameter() * self.diameter() * (self.max_y - self.min_y + 1)) as u32
    }

    /// `getOffsetForIndex` from `getStartingPos`: x fastest, then z, then y.
    pub fn position(&self, miner: [i32; 3], index: u32) -> [i32; 3] {
        let d = self.diameter() as u32;
        let start = [miner[0] - self.radius, self.min_y, miner[2] - self.radius];
        [
            start[0] + (index % d) as i32,
            start[1] + (index / d / d) as i32,
            start[2] + ((index / d) % d) as i32,
        ]
    }

    /// `setRadiusFromPacket`.
    pub fn with_radius(self, radius: i32) -> Region {
        Region {
            radius: radius.clamp(0, MAX_RADIUS),
            ..self
        }
    }

    /// `setMinYFromPacket`: between the world's bottom and the current top.
    pub fn with_min_y(self, min_y: i32) -> Region {
        Region {
            min_y: min_y.clamp(WORLD_MIN_Y, self.max_y),
            ..self
        }
    }

    /// `setMaxYFromPacket`: between the current bottom and the world's top.
    pub fn with_max_y(self, max_y: i32) -> Region {
        Region {
            max_y: max_y.clamp(self.min_y, WORLD_MAX_Y),
            ..self
        }
    }

    /// `MinerEnergyContainer.updateMinerEnergyPerTick`: the usage grows with the radius and the height past
    /// the defaults, and silk touch multiplies it.
    pub fn energy_per_tick(&self, base: u64, silk: bool) -> u64 {
        let mut usage = base as f64;
        if silk {
            usage *= SILK_MULTIPLIER as f64;
        }
        let radius_range = f64::from(MAX_RADIUS - RADIUS);
        let height_range = f64::from(WORLD_MAX_Y - WORLD_MIN_Y + 1 - 1 - (MAX_Y - MIN_Y));
        let radius_cost = (f64::from(self.radius - RADIUS) / radius_range).max(0.0);
        let height_cost =
            (f64::from(self.max_y - self.min_y - (MAX_Y - MIN_Y)) / height_range).max(0.0);
        (usage * (1.0 + radius_cost) * (1.0 + height_cost)).ceil() as u64
    }

    /// The positions one search step looks at, from `cursor` counted from the top of the region down (so
    /// Mekanism's mining from the highest index lets falling blocks land where it will reach them).
    pub fn search_order(&self, cursor: u32, limit: u32) -> impl Iterator<Item = u32> {
        let total = self.total_size();
        (cursor..(cursor.saturating_add(limit)).min(total)).map(move |k| total - 1 - k)
    }
}

/// What a filter names (`MinerItemStackFilter`, `MinerTagFilter`, `MinerModIDFilter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterKind {
    #[default]
    Item,
    Tag,
    Mod,
}

impl FilterKind {
    pub fn from_index(i: u32) -> FilterKind {
        match i {
            1 => FilterKind::Tag,
            2 => FilterKind::Mod,
            _ => FilterKind::Item,
        }
    }

    pub fn index(self) -> u32 {
        match self {
            FilterKind::Item => 0,
            FilterKind::Tag => 1,
            FilterKind::Mod => 2,
        }
    }
}

/// A miner filter (`MinerFilter`): the block it names, whether it is enabled, the item that replaces the
/// mined block (`replaceTarget`, empty for air) and whether mining needs that replacement.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Filter {
    pub kind: FilterKind,
    /// An item's id, a tag's id, or a namespace.
    pub name: String,
    pub enabled: bool,
    pub replace: String,
    pub requires_replacement: bool,
}

impl Filter {
    pub fn new(kind: FilterKind, name: &str) -> Filter {
        Filter {
            kind,
            name: name.to_owned(),
            enabled: true,
            replace: String::new(),
            requires_replacement: false,
        }
    }

    /// `canFilter(state)`: the block is the item's block, in the tag, or from the mod.
    pub fn matches(&self, block: &str, in_tag: &dyn Fn(&str, &str) -> bool) -> bool {
        match self.kind {
            FilterKind::Item => self.name == block,
            FilterKind::Tag => in_tag(&self.name, block),
            FilterKind::Mod => block.split_once(':').is_some_and(|(ns, _)| ns == self.name),
        }
    }

    /// `hasBlacklistedElement`: whether the filter names something the miner can never mine
    /// (`MekanismTags.Blocks.MINER_BLACKLIST`, which holds `minecraft:bedrock` and the like).
    pub fn has_blacklisted_element(&self, blacklisted: &dyn Fn(&str) -> bool) -> bool {
        match self.kind {
            FilterKind::Item => blacklisted(&self.name),
            _ => false,
        }
    }

    /// One line per filter for the world save: `kind|enabled|require|replace|name`.
    pub fn save(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.kind.index(),
            u8::from(self.enabled),
            u8::from(self.requires_replacement),
            self.replace,
            self.name
        )
    }

    pub fn load(line: &str) -> Option<Filter> {
        let mut p = line.splitn(5, '|');
        let kind = FilterKind::from_index(p.next()?.parse().ok()?);
        let enabled = p.next()? == "1";
        let requires_replacement = p.next()? == "1";
        let replace = p.next()?.to_owned();
        let name = p.next()?.to_owned();
        Some(Filter {
            kind,
            name,
            enabled,
            replace,
            requires_replacement,
        })
    }
}

/// The first enabled filter that names `block` (the order of the list decides).
pub fn matching_filter<'a>(
    filters: &'a [Filter],
    block: &str,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'a Filter> {
    filters
        .iter()
        .filter(|f| f.enabled)
        .find(|f| f.matches(block, in_tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_region_is_21_by_21_by_61() {
        let r = Region::default();
        assert_eq!(r.total_size(), 21 * 21 * 61);
        assert_eq!(r.position([100, 64, -50], 0), [90, 0, -60]);
        assert_eq!(
            r.position([100, 64, -50], r.total_size() - 1),
            [110, 60, -40]
        );
        assert_eq!(r.position([0, 0, 0], 21), [-10, 0, -9]);
    }

    #[test]
    fn searching_starts_at_the_top() {
        let r = Region::default();
        let first: Vec<u32> = r.search_order(0, 2).collect();
        assert_eq!(first, [r.total_size() - 1, r.total_size() - 2]);
        assert_eq!(
            r.search_order(r.total_size() - 1, 5).collect::<Vec<_>>(),
            [0]
        );
    }

    #[test]
    fn settings_are_clamped_and_cost_energy() {
        let r = Region::default();
        assert_eq!(r.with_radius(99).radius, 32);
        assert_eq!(r.with_min_y(100).min_y, 60);
        assert_eq!(r.with_max_y(-100).max_y, 0);
        assert_eq!(r.energy_per_tick(40, false), 40);
        assert_eq!(r.energy_per_tick(40, true), 240);
        assert!(r.with_radius(32).energy_per_tick(40, false) == 80);
        assert!(r.with_max_y(319).energy_per_tick(40, false) > 40);
    }

    #[test]
    fn filters_match_and_round_trip() {
        let in_tag = |tag: &str, block: &str| tag == "c:ores" && block.ends_with("_ore");
        let ore = Filter::new(FilterKind::Tag, "c:ores");
        assert!(ore.matches("minecraft:coal_ore", &in_tag));
        assert!(!ore.matches("minecraft:dirt", &in_tag));
        assert!(Filter::new(FilterKind::Mod, "minecraft").matches("minecraft:dirt", &in_tag));
        let mut item = Filter::new(FilterKind::Item, "minecraft:stone");
        item.replace = "minecraft:cobblestone".into();
        item.requires_replacement = true;
        assert_eq!(Filter::load(&item.save()), Some(item.clone()));
        let off = Filter {
            enabled: false,
            ..Filter::new(FilterKind::Item, "minecraft:dirt")
        };
        assert!(matching_filter(&[off, item], "minecraft:dirt", &in_tag).is_none());
    }
}
