//! Logistical transporters (`LogisticalTransporterBase`), the restrictive and diversion transporters, and the
//! logistical sorter. Port of the "Logistical transporters" section and `tick_sorter` of `runtime.rs`.
//!
//! Every transporter holds the stacks travelling through it (`transit`), each with its path, progress (100 a block,
//! `<tier>Speed` a tick) and colour. A stack that reaches the end of a transporter goes on into the next one (when
//! that one lets its colour and connections in) or into the container at the end of its path; one that cannot
//! recalculates its path, goes home or idles, and is dropped when it has nowhere to be. Paths are
//! `TransporterPathfinder`'s A* (`mekanism_core::transporter`). Containers are reached through the `ITEMS`
//! capability, so vanilla chests, furnaces and Mekanism's own machines all take part.
use super::nbt::{get_int, get_long, put_int, put_long};
use super::transmit::{self, conn_of, direction, face_index, neg, offset, sides_of, Own, DIRECTIONS};
use mekanism_core::sides::Transmission;
use mekanism_core::sorter::{self, Sorter};
use mekanism_core::transporter as tnet;
use mod_api::{
    BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent, BlockPart, Capability,
    Compound, Direction, Host, HostTransaction, ItemResource, ItemStack, LevelRef, PreRemoveSideEffectsEvent,
    Registrar, ResourceHandler, ServerHost, Tag, ITEMS,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

/// The block entity types of the transporters and of the sorter.
pub(crate) const TYPE: &str = "transporter";
pub(crate) const SORTER_TYPE: &str = "sorter";
const SORTER_BLOCK: &str = "mekanism:logistical_sorter";

type P3 = [i32; 3];
type Items = Capability<dyn ResourceHandler<ItemResource>>;

/// `TransporterStack.Path`.
const PATH_DEST: u8 = 0;
const PATH_HOME: u8 = 1;
const PATH_NONE: u8 = 2;

const TIERS: [(&str, u32); 4] = [("basic", 5), ("advanced", 10), ("elite", 20), ("ultimate", 50)];

fn blocks() -> Vec<String> {
    let mut blocks: Vec<String> = TIERS.iter().map(|(t, _)| format!("mekanism:{t}_logistical_transporter")).collect();
    blocks.push("mekanism:restrictive_transporter".into());
    blocks.push("mekanism:diversion_transporter".into());
    blocks
}

pub(crate) fn is_transporter(name: &str) -> bool {
    name.strip_prefix("mekanism:")
        .is_some_and(|n| n.ends_with("_logistical_transporter") || n == "restrictive_transporter" || n == "diversion_transporter")
}

/// The items a logistical transporter pulls at a time (`TransporterTier`), 16 for the restrictive and diversion ones
/// as the basic tier (`tier::transporter_pull`).
pub(crate) fn pull_of(id: &str) -> Option<u32> {
    use mekanism_core::tier::{transporter_pull, BaseTier};
    let tier = if id == "mekanism:restrictive_transporter" || id == "mekanism:diversion_transporter" {
        BaseTier::Basic
    } else {
        let t = id.strip_prefix("mekanism:")?.strip_suffix("_logistical_transporter")?;
        BaseTier::ALL.into_iter().find(|b| b.name() == t)?
    };
    transporter_pull(tier)
}

/// `TransporterTier` `<tier>Speed` of a logistical transporter: progress a tick (100 is a block).
fn transporter_speed(id: &str) -> u32 {
    let name = id.strip_prefix("mekanism:").unwrap_or(id);
    let (tier, default) = TIERS.into_iter().find(|(t, _)| name.starts_with(t)).unwrap_or(("basic", 5));
    mekanism_core::config_files::active_int("tiers", "transmitters.items", &format!("{tier}Speed"), default).max(1) as u32
}

pub(crate) fn register(r: &mut dyn Registrar) {
    let blocks = blocks();
    let blocks: Vec<&str> = blocks.iter().map(String::as_str).collect();
    let kind = r.block_entity_type(TYPE).blocks(&blocks).build();
    r.block_entity_part(&kind, TransporterPart).expect("native block entity type");
    let kind = r.block_entity_type(SORTER_TYPE).blocks(&[SORTER_BLOCK]).build();
    r.block_entity_part(&kind, SorterPart).expect("native block entity type");
}

// --- Stacks in transit ------------------------------------------------------------------------------

/// A `TransporterStack`.
#[derive(Clone)]
struct Transit {
    id: u32,
    item: ItemStack,
    progress: u32,
    color: Option<u8>,
    /// The transporters it passes in order, then (for a stack with a target) the container it goes to.
    path: Vec<P3>,
    ptype: u8,
    /// `originalLocation`: where it came from, the block before the first of its path.
    orig: P3,
    home: Option<P3>,
    idle: Option<usize>,
    init: bool,
    /// When (game time) and how far along its block the clients last heard of it.
    sync_t: u64,
    sync_pr: u32,
}

fn pos_tag(p: P3) -> Tag {
    Tag::IntArray(p.to_vec())
}

fn get_pos(nbt: &Compound, key: &str) -> Option<P3> {
    match nbt.get(&key.into()) {
        Some(Tag::IntArray(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
        _ => None,
    }
}

impl Transit {
    fn save(&self, host: &dyn Host) -> Option<Compound> {
        let mut c = Compound::new();
        put_int(&mut c, "id", self.id);
        c.insert("it".into(), Tag::Compound(self.item.to_nbt(host)?));
        put_int(&mut c, "pr", self.progress);
        put_int(&mut c, "col", self.color.map_or(0, |c| u32::from(c) + 1));
        c.insert("path".into(), Tag::IntArray(self.path.iter().flatten().copied().collect()));
        put_int(&mut c, "pt", u32::from(self.ptype));
        c.insert("orig".into(), pos_tag(self.orig));
        if let Some(h) = self.home {
            c.insert("home".into(), pos_tag(h));
        }
        put_int(&mut c, "idle", self.idle.map_or(0, |d| d as u32 + 1));
        put_int(&mut c, "init", u32::from(self.init));
        put_long(&mut c, "st", self.sync_t);
        put_int(&mut c, "spr", self.sync_pr);
        Some(c)
    }

    fn load(host: &dyn Host, c: &Compound) -> Option<Transit> {
        let Some(Tag::Compound(item)) = c.get(&"it".into()) else {
            return None;
        };
        let item = ItemStack::from_nbt(host, item)?;
        let path = match c.get(&"path".into()) {
            Some(Tag::IntArray(v)) => v.chunks(3).filter(|p| p.len() == 3).map(|p| [p[0], p[1], p[2]]).collect(),
            _ => Vec::new(),
        };
        Some(Transit {
            id: get_int(c, "id"),
            item,
            progress: get_int(c, "pr"),
            color: match get_int(c, "col") {
                0 => None,
                n => Some((n - 1) as u8),
            },
            path,
            ptype: get_int(c, "pt").min(2) as u8,
            orig: get_pos(c, "orig").unwrap_or_default(),
            home: get_pos(c, "home"),
            idle: match get_int(c, "idle") {
                0 => None,
                n => Some((n - 1).min(5) as usize),
            },
            init: get_int(c, "init") != 0,
            sync_t: get_long(c, "st"),
            sync_pr: get_int(c, "spr"),
        })
    }

    fn has_target(&self) -> bool {
        self.ptype != PATH_NONE
    }

    fn has_path(&self) -> bool {
        self.path.len() >= 2
    }

    /// `getNext`: the block after `at` on its path.
    fn next(&self, at: P3) -> Option<P3> {
        let i = self.path.iter().position(|p| *p == at)?;
        self.path.get(i + 1).copied()
    }

    /// `getPrev`: the block before `at` on its path, or where it came from.
    fn prev(&self, at: P3) -> P3 {
        match self.path.iter().position(|p| *p == at) {
            Some(i) if i > 0 => self.path[i - 1],
            _ => self.orig,
        }
    }

    /// `isFinal`: `at` is the last transporter of its path.
    fn is_final(&self, at: P3) -> bool {
        let i = if self.has_target() { self.path.len().checked_sub(2) } else { self.path.len().checked_sub(1) };
        i.and_then(|i| self.path.get(i)) == Some(&at)
    }

    /// `getSide`: the side it is nearest in the transporter at `at`, down when it has none.
    fn side(&self, at: P3) -> usize {
        let side = if self.progress < 50 {
            tnet::side_between(self.prev(at), at)
        } else {
            self.next(at).and_then(|n| tnet::side_between(at, n))
        };
        side.unwrap_or(0)
    }

    /// Tells the clients about it as it is now.
    fn touch(&mut self, now: u64) {
        self.sync_t = now;
        self.sync_pr = self.progress;
    }
}

fn load_transit(host: &dyn Host, nbt: &Compound) -> Vec<Transit> {
    match nbt.get(&"transit".into()) {
        Some(Tag::List(l)) => l
            .iter()
            .filter_map(|t| match t {
                Tag::Compound(c) => Transit::load(host, c),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn save_transit(host: &dyn Host, nbt: &mut Compound, stacks: &[Transit]) {
    let saved: Vec<Tag> = stacks.iter().filter_map(|s| s.save(host)).map(Tag::Compound).collect();
    if saved.is_empty() {
        nbt.remove(&"transit".into());
    } else {
        nbt.insert("transit".into(), Tag::List(saved));
    }
}

/// What the clients draw of the stacks of the transporter at `pos` (`PacketTransporterBatch`).
fn client_transit(host: &dyn Host, pos: P3, stacks: &[Transit]) -> Tag {
    Tag::List(
        stacks
            .iter()
            .filter(|s| s.item.count() > 0)
            .map(|s| {
                let mut c = Compound::new();
                put_int(&mut c, "id", s.id);
                c.insert("i".into(), Tag::String(s.item.item().id(host).as_str().into()));
                put_int(&mut c, "pr", s.sync_pr);
                put_long(&mut c, "t", s.sync_t);
                c.insert("pv".into(), pos_tag(s.prev(pos)));
                if let Some(n) = s.next(pos) {
                    c.insert("nx".into(), pos_tag(n));
                }
                Tag::Compound(c)
            })
            .collect(),
    )
}

/// The stacks inside each transporter this server runs, to drop when it is broken (`preRemoveSideEffects`).
static TRANSIT_DROPS: Mutex<Option<HashMap<P3, Vec<ItemStack>>>> = Mutex::new(None);

fn remember_transit(pos: P3, stacks: &[Transit]) {
    let mut guard = TRANSIT_DROPS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    if stacks.is_empty() {
        map.remove(&pos);
    } else {
        map.insert(pos, stacks.iter().map(|s| s.item.clone()).collect());
    }
}

fn color_of(nbt: &Compound) -> Option<u8> {
    match get_int(nbt, "color") {
        0 => None,
        c => Some((c - 1) as u8),
    }
}

/// Whether the item `stack` has no components: a plain stack of its item.
fn plain(stack: &ItemStack) -> bool {
    stack.components().is_none() && stack.damage() == 0
}

/// What a network files a stack under, for the stacks already on their way to a container.
fn item_key(host: &dyn Host, stack: &ItemStack) -> String {
    let mut one = stack.clone();
    let _ = one.set_count(1);
    one.to_nbt(host).map(|c| format!("{c:?}")).unwrap_or_default()
}

// --- The world a transporter sees --------------------------------------------------------------------

/// The server, the ticking block entity, and the level: what every step of a transporter's tick reaches the world by.
struct Cx<'a, 'b, 'c, 'd> {
    host: &'a mut dyn ServerHost,
    own: &'b mut Own<'c, 'd>,
    level: LevelRef,
    now: u64,
}

/// What the world around the transporters offers: the transporters joined to the start and the stacks already on
/// their way to each container (`TransporterManager`'s prediction).
struct Snap {
    net: tnet::Network,
    inflight: HashMap<(P3, String), u32>,
}

/// A way to a container (`Destination`).
struct Plan {
    /// The transporters, then the container.
    path: Vec<P3>,
    dest: P3,
    /// The side of the container's neighbour transporter it goes in through, seen from the transporter.
    side: usize,
    sending: u32,
    score: f64,
}

impl Cx<'_, '_, '_, '_> {
    fn read(&self, pos: P3) -> Option<(String, Compound)> {
        transmit::read(&*self.host, self.own, self.level, pos)
    }

    fn block_id(&self, pos: P3) -> String {
        transmit::block_id(&*self.host, self.level, pos).unwrap_or_default()
    }

    fn update<R>(&mut self, pos: P3, change: impl FnOnce(&str, &mut Compound) -> R) -> Option<R> {
        transmit::update(self.host, self.own, self.level, pos, change)
    }

    /// One transporter as the pathfinder sees it, from its block and its data.
    fn node_from(&self, at: P3, id: &str, nbt: &Compound) -> tnet::Node {
        let cost = if id == "mekanism:restrictive_transporter" {
            1000.0
        } else {
            tnet::tier_cost(transporter_speed("mekanism:ultimate_logistical_transporter"), transporter_speed(id))
        };
        let mut blocked = [false; 6];
        if id == "mekanism:diversion_transporter" {
            let modes = get_int(nbt, "div");
            if modes != 0 {
                let powered = self.level.has_neighbor_signal(&*self.host, at);
                for (d, flag) in blocked.iter_mut().enumerate() {
                    *flag = !tnet::Diversion::from_id((modes >> (2 * d)) & 3).met(powered);
                }
            }
        }
        tnet::Node { cost, color: color_of(nbt), conn: std::array::from_fn(|d| tnet::Conn::from_bits(conn_of(nbt, d))), blocked }
    }

    // The containers: the `ITEMS` capability of the block beside a transporter.

    fn handler(&mut self, pos: P3, face: usize) -> Option<Items> {
        self.level.capability(self.host, &ITEMS, pos, Some(Direction::ALL[face]))
    }

    /// `insert_any` into `cap`, simulated (the transaction is abandoned) or for real: how many went.
    fn put(&mut self, cap: &Items, stack: &ItemStack, simulate: bool) -> u32 {
        let resource = ItemResource::of(stack);
        let count = stack.count();
        let result: Result<u32, u32> = self.host.transaction(|host, tx| {
            let n = cap.insert_any(host, tx, &resource, count);
            if simulate { Err(n) } else { Ok(n) }
        });
        result.unwrap_or_else(|n| n)
    }

    /// How many of `stack` the container at `pos` takes through its side `face` (`native_can_insert_item`).
    fn can_insert(&mut self, pos: P3, face: usize, stack: &ItemStack) -> u32 {
        match self.handler(pos, face) {
            Some(cap) => self.put(&cap, stack, true),
            None => 0,
        }
    }

    /// Puts `stack` into the container at `pos` through `face`: what did not fit, None when all did.
    fn insert(&mut self, pos: P3, face: usize, stack: ItemStack) -> Option<ItemStack> {
        let Some(cap) = self.handler(pos, face) else {
            return Some(stack);
        };
        let went = self.put(&cap, &stack, false);
        let rest = stack.count().saturating_sub(went);
        stack.shrink(went).filter(|_| rest > 0)
    }

    /// Takes up to `max` of one kind of item that `wanted` accepts out of the container at `pos` through `face`.
    fn extract_where(&mut self, pos: P3, face: usize, max: u32, wanted: &dyn Fn(&ItemStack) -> bool) -> Option<ItemStack> {
        let cap = self.handler(pos, face)?;
        let size = cap.size(self.host);
        let mut kind: Option<ItemResource> = None;
        let mut taken = 0;
        for index in 0..size {
            if taken >= max {
                break;
            }
            let Some(resource) = cap.resource(self.host, index) else {
                continue;
            };
            match &kind {
                Some(k) if !k.matches(resource.as_stack()) => continue,
                None if !wanted(resource.as_stack()) => continue,
                _ => {}
            }
            let amount = cap.amount(self.host, index).min(max - taken);
            let got: u32 = self.host.transaction(|host, tx| Ok::<_, ()>(cap.extract(host, tx, index, &resource, amount))).unwrap_or(0);
            if got > 0 {
                kind = Some(resource);
                taken += got;
            }
        }
        kind.and_then(|k| k.to_stack(taken))
    }

    /// `TransporterUtils.canInsert`'s colour rule: a machine with strict input only takes what its colour for that
    /// side says.
    fn strict_input_ok(&self, dest: P3, face: usize, color: Option<u8>) -> bool {
        let facing = self.level.block_property(&*self.host, dest, "facing").and_then(|f| direction(&f)).map_or(2, face_index);
        let Some((_, nbt)) = self.read(dest) else {
            return true;
        };
        if get_int(&nbt, "ej_strict") == 0 {
            return true;
        }
        let rel = (0..6).find(|&s| mekanism_core::sides::relative_face(facing, s) == face).unwrap_or(0);
        let wanted = get_int(&nbt, &format!("ej_in{rel}"));
        wanted == 0 || color.map(|c| u32::from(c) + 1) == Some(wanted)
    }

    /// How many of `item` the container at `dest` takes through its side `face`, besides the `inflight` items already
    /// on their way to it.
    fn accepted_by(&mut self, dest: P3, face: usize, item: &ItemStack, inflight: u32, color: Option<u8>, force: bool) -> u32 {
        if !force && !self.strict_input_ok(dest, face, color) {
            return 0;
        }
        let mut probe = item.clone();
        let _ = probe.set_count(item.count() + inflight);
        self.can_insert(dest, face, &probe).saturating_sub(inflight).min(item.count())
    }

    fn add_inflight(&self, snap: &mut Snap, stacks: &[Transit], exclude: Option<u32>, here: bool) {
        for s in stacks {
            if here && Some(s.id) == exclude {
                continue;
            }
            if s.has_target()
                && s.has_path()
                && let Some(dest) = s.path.last()
            {
                *snap.inflight.entry((*dest, item_key(&*self.host, &s.item))).or_default() += s.item.count();
            }
        }
    }

    /// The transporters joined to the one at `here` (its data `here_nbt`, its stacks `here_stacks`), as
    /// `InventoryNetwork` holds them. `exclude` is a stack of `here` that is not counted as on its way.
    fn snapshot(&self, here: P3, here_nbt: &Compound, here_stacks: &[Transit], exclude: Option<u32>) -> Snap {
        let mut snap = Snap { net: tnet::Network::default(), inflight: HashMap::new() };
        let id = self.block_id(here);
        let node = self.node_from(here, &id, here_nbt);
        snap.net.nodes.insert(here, node);
        self.add_inflight(&mut snap, here_stacks, exclude, true);
        let mut order = vec![here];
        let mut member: HashSet<P3> = HashSet::from([here]);
        let mut i = 0;
        while i < order.len() && order.len() < 1024 {
            let at = order[i];
            i += 1;
            for d in 0..6 {
                let n = tnet::offset(at, d);
                if !snap.net.nodes.contains_key(&n) {
                    let nid = self.block_id(n);
                    if !is_transporter(&nid) {
                        continue;
                    }
                    let Some((_, nbt)) = self.read(n) else {
                        continue;
                    };
                    let stacks = load_transit(&*self.host, &nbt);
                    let node = self.node_from(n, &nid, &nbt);
                    snap.net.nodes.insert(n, node);
                    self.add_inflight(&mut snap, &stacks, None, false);
                }
                if !member.contains(&n) && snap.net.joined(at, d) {
                    member.insert(n);
                    order.push(n);
                }
            }
        }
        snap
    }

    /// `TransporterPathfinder.getPaths`: every container beside the network that takes the item and can be reached,
    /// best first.
    fn plan_routes(&mut self, snap: &Snap, start: P3, item: &ItemStack, color: Option<u8>, home: Option<P3>, min: u32) -> Vec<Plan> {
        let min = min.max(1);
        let mut cands: BTreeMap<P3, Vec<usize>> = BTreeMap::new();
        for t in snap.net.component(start) {
            let node = &snap.net.nodes[&t];
            for d in 0..6 {
                let n = tnet::offset(t, d);
                if node.can_emit_to(d) && !snap.net.nodes.contains_key(&n) && Some(n) != home {
                    cands.entry(n).or_default().push(d);
                }
            }
        }
        let key = item_key(&*self.host, item);
        let mut found: Vec<Plan> = Vec::new();
        for (dest, dirs) in cands {
            let inflight = snap.inflight.get(&(dest, key.clone())).copied().unwrap_or(0);
            let mut accepted = [0u32; 6];
            for &d in &dirs {
                accepted[d] = self.accepted_by(dest, tnet::opposite(d), item, inflight, color, false);
            }
            if accepted.iter().all(|a| *a < min) {
                continue;
            }
            if let Some(f) = tnet::find_path(&snap.net, start, dest, color, home, &|d| accepted[d] >= min) {
                let mut path = f.path;
                path.push(dest);
                found.push(Plan { path, dest, side: f.side, sending: accepted[f.side], score: f.score });
            }
        }
        found.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal).then(a.path.len().cmp(&b.path.len())));
        found
    }

    /// `TransporterStack.calculateIdle`: back home when it can, else wandering along the transporters (or off to a
    /// container it found after all). False when it can do neither.
    fn calc_idle(&mut self, snap: &Snap, here: P3, s: &mut Transit) -> bool {
        if let Some(home) = s.home {
            let accepts: [bool; 6] = std::array::from_fn(|d| self.can_insert(home, tnet::opposite(d), &s.item) > 0);
            if let Some(f) = tnet::find_path(&snap.net, here, home, s.color, Some(home), &|d| accepts[d]) {
                s.path = f.path;
                s.path.push(home);
                s.ptype = PATH_HOME;
                s.idle = None;
                s.orig = here;
                s.init = true;
                return true;
            }
            s.home = None;
        }
        let (item, color, home, closest) = (s.item.clone(), s.color, s.home, s.side(here));
        let mut idle = s.idle;
        let mut reroute = || {
            self.plan_routes(snap, here, &item, color, home, 0)
                .into_iter()
                .next()
                .map(|p| tnet::Found { path: p.path[..p.path.len() - 1].to_vec(), side: p.side, score: p.score })
        };
        let Some(result) = tnet::idle_path(&snap.net, here, color, &mut idle, closest, &mut reroute) else {
            return false;
        };
        s.idle = idle;
        if let Some(f) = result.dest {
            let dest = tnet::offset(*f.path.last().unwrap_or(&here), f.side);
            s.path = f.path;
            s.path.push(dest);
            s.ptype = PATH_DEST;
        } else {
            s.path = result.path;
            s.ptype = PATH_NONE;
        }
        s.orig = here;
        s.init = true;
        true
    }

    /// A stack with nowhere to go falls out of the transporter (`drop`).
    fn drop_stack(&mut self, here: P3, s: &Transit) {
        if s.item.count() > 0 {
            let _ = self.host.drop_at(here, s.item.clone());
        }
    }

    /// `recalculate`: a new way to a container, else home or idling. False when the stack was dropped instead.
    fn recalculate(&mut self, here: P3, here_nbt: &Compound, others: &[Transit], s: &mut Transit, from: Option<P3>) -> bool {
        let snap = self.snapshot(here, here_nbt, others, Some(s.id));
        let mut ok = false;
        if s.ptype != PATH_NONE {
            let plans = self.plan_routes(&snap, here, &s.item, s.color, s.home, 0);
            if let Some(p) = plans.into_iter().next() {
                s.path = p.path;
                s.ptype = PATH_DEST;
                s.idle = None;
                s.init = true;
                ok = true;
            }
        }
        if !ok && !self.calc_idle(&snap, here, s) {
            self.drop_stack(here, s);
            return false;
        }
        if let Some(f) = from {
            s.orig = f;
        }
        true
    }

    /// The next transporter takes the stack in, if its colour and connections let it (`entityEntering`).
    fn hand_over(&mut self, here: P3, here_id: &str, here_nbt: &Compound, s: &Transit, next: P3) -> bool {
        let nid = self.block_id(next);
        if !is_transporter(&nid) {
            return false;
        }
        let Some((_, nbt)) = self.read(next) else {
            return false;
        };
        let mut stacks = load_transit(&*self.host, &nbt);
        let Some(dir) = tnet::side_between(here, next) else {
            return false;
        };
        let mut net = tnet::Network::default();
        let me = self.node_from(here, here_id, here_nbt);
        let other = self.node_from(next, &nid, &nbt);
        net.nodes.insert(here, me);
        net.nodes.insert(next, other);
        if !net.can_enter(s.color, here, dir) {
            return false;
        }
        let id = get_int(&nbt, "next_id");
        let mut moved = s.clone();
        moved.id = id;
        moved.progress = s.progress % 100;
        moved.touch(self.now);
        stacks.push(moved);
        self.store_other(next, &stacks, id + 1);
        true
    }

    /// Whether the container at the end of the stack's path still takes it at the half-way mark (`canInsert`).
    fn dest_still_takes(&mut self, dest: P3, face: usize, s: &Transit) -> bool {
        let force = s.ptype == PATH_HOME;
        if !force && !self.strict_input_ok(dest, face, s.color) {
            return false;
        }
        self.can_insert(dest, face, &s.item) > 0
    }

    /// One tick of one stack in the transporter at `pos` (`tickTransit`). False when it left (or was dropped).
    fn step(&mut self, pos: P3, id: &str, nbt: &Compound, speed: u32, stacks: &mut [Transit], i: usize) -> bool {
        let now = self.now;
        let mut s = stacks[i].clone();
        if !s.init {
            if s.item.count() == 0 || !self.recalculate(pos, nbt, stacks, &mut s, None) {
                return false;
            }
            s.touch(now);
        }
        let before = s.progress;
        s.progress += speed;
        if s.progress >= 100 {
            let mut prev_set = None;
            if s.has_path() {
                if s.path.last() == Some(&pos) {
                    return false;
                }
                if let Some(next) = s.next(pos) {
                    if !s.is_final(pos) {
                        if self.hand_over(pos, id, nbt, &s, next) {
                            return false;
                        }
                        prev_set = Some(next);
                    } else if s.has_target() {
                        let face = tnet::side_between(pos, next).map_or(0, tnet::opposite);
                        let had = s.item.count();
                        match self.insert(next, face, s.item.clone()) {
                            None => return false,
                            Some(rest) if rest.count() < had => s.item = rest,
                            Some(_) => {}
                        }
                        prev_set = Some(next);
                    }
                }
            }
            if !self.recalculate(pos, nbt, stacks, &mut s, prev_set) {
                return false;
            }
            s.progress = if prev_set.is_none() { 50 } else { 0 };
            s.touch(now);
        } else if before < 50 && s.progress >= 50 {
            let again = if s.is_final(pos) {
                if s.has_target() {
                    match (s.path.last().copied(), tnet::side_between(pos, s.path.last().copied().unwrap_or(pos))) {
                        (Some(dest), Some(dir)) => {
                            let can_send = tnet::Conn::from_bits(conn_of(nbt, dir)).can_send_to();
                            !can_send || !self.dest_still_takes(dest, tnet::opposite(dir), &s)
                        }
                        _ => true,
                    }
                } else {
                    true
                }
            } else {
                match s.next(pos) {
                    None => true,
                    Some(next) => {
                        let dir = tnet::side_between(pos, next).unwrap_or(0);
                        let nid = self.block_id(next);
                        if !is_transporter(&nid) && !s.has_target() && s.path.len() == 2 {
                            // An idle stack in a lone transporter only turns when it cannot go that way.
                            !tnet::Conn::from_bits(conn_of(nbt, dir)).can_send_to()
                        } else if is_transporter(&nid) {
                            let nnbt = self.read(next).map(|(_, n)| n).unwrap_or_default();
                            let mut net = tnet::Network::default();
                            let me = self.node_from(pos, id, nbt);
                            let other = self.node_from(next, &nid, &nnbt);
                            net.nodes.insert(pos, me);
                            net.nodes.insert(next, other);
                            !net.can_enter(s.color, pos, dir)
                        } else {
                            true
                        }
                    }
                }
            };
            if again {
                if !self.recalculate(pos, nbt, stacks, &mut s, None) {
                    return false;
                }
                s.touch(now);
            }
        }
        stacks[i] = s;
        true
    }

    /// `tryPull`: from each container beside a pull face the transporter takes up to its tier's count and sends it
    /// where `plan_routes` says; what goes nowhere is put back and the next try waits longer.
    fn try_pull(&mut self, pos: P3, pull: u32, stacks: &mut Vec<Transit>) -> bool {
        let mut changed = false;
        let color = color_of(&self.own.entity.nbt);
        for d in 0..6 {
            if conn_of(&self.own.entity.nbt, d) != 2 {
                continue;
            }
            let source = tnet::offset(pos, d);
            let back = DIRECTIONS[tnet::opposite(d)].1;
            let closed = self
                .read(source)
                .is_some_and(|(_, n)| !sides_of(&n, Transmission::Item).get(face_index(back)).gives_out());
            if closed || is_transporter(&self.block_id(source)) {
                continue;
            }
            let face = tnet::opposite(d);
            let Some(stack) = self.extract_where(source, face, pull, &|_| true) else {
                continue;
            };
            let own_nbt = self.own.entity.nbt.clone();
            let snap = self.snapshot(pos, &own_nbt, stacks, None);
            let plans = self.plan_routes(&snap, pos, &stack, color, Some(source), 1);
            match plans.first() {
                Some(plan) => {
                    let sending = plan.sending.min(stack.count());
                    if sending < stack.count() {
                        let mut rest = stack.clone();
                        let _ = rest.set_count(stack.count() - sending);
                        if let Some(lost) = self.insert(source, face, rest) {
                            self.insert(source, 1, lost);
                        }
                    }
                    let mut item = stack;
                    let _ = item.set_count(sending);
                    let id = get_int(&self.own.entity.nbt, "next_id");
                    put_int(&mut self.own.entity.nbt, "next_id", id + 1);
                    let mut s = Transit {
                        id,
                        item,
                        progress: 0,
                        color,
                        path: plan.path.clone(),
                        ptype: PATH_DEST,
                        orig: source,
                        home: Some(source),
                        idle: None,
                        init: true,
                        sync_t: 0,
                        sync_pr: 0,
                    };
                    s.touch(self.now);
                    stacks.push(s);
                    put_int(&mut self.own.entity.nbt, "delay", 10);
                    put_int(&mut self.own.entity.nbt, "delay_count", 0);
                    changed = true;
                }
                None => {
                    if let Some(lost) = self.insert(source, face, stack) {
                        self.insert(source, 1, lost);
                    }
                    // Backs off: `min(40, e^tries)` ticks.
                    let tries = get_int(&self.own.entity.nbt, "delay_count") + 1;
                    put_int(&mut self.own.entity.nbt, "delay_count", tries);
                    put_int(&mut self.own.entity.nbt, "delay", (f64::from(tries).exp() as u32).min(40));
                }
            }
        }
        changed
    }

    /// Saves the stacks of the ticking transporter and tells the clients what changed.
    fn store_own(&mut self, pos: P3, stacks: &[Transit]) {
        save_transit(&*self.host, &mut self.own.entity.nbt, stacks);
        let mut client = Compound::new();
        client.insert("transit".into(), client_transit(&*self.host, pos, stacks));
        put_int(&mut client, "color", get_int(&self.own.entity.nbt, "color"));
        super::common::put_client(self.own.entity, &["transit", "color"], client);
        remember_transit(pos, stacks);
    }

    /// The same for a transporter that is not ticking now (its neighbour hands a stack over, a sorter inserts one).
    fn store_other(&mut self, pos: P3, stacks: &[Transit], next_id: u32) {
        let transit = client_transit(&*self.host, pos, stacks);
        let saved: Vec<Tag> = stacks.iter().filter_map(|s| s.save(&*self.host)).map(Tag::Compound).collect();
        self.update(pos, |_, nbt| {
            if saved.is_empty() {
                nbt.remove(&"transit".into());
            } else {
                nbt.insert("transit".into(), Tag::List(saved));
            }
            put_int(nbt, "next_id", next_id);
            let mut client = match nbt.get(&"client".into()) {
                Some(Tag::Compound(c)) => c.clone(),
                _ => Compound::new(),
            };
            client.insert("transit".into(), transit);
            put_int(&mut client, "color", get_int(nbt, "color"));
            nbt.insert("client".into(), Tag::Compound(client));
        });
        self.host.block_entity_sync(pos);
        remember_transit(pos, stacks);
    }

    /// `LogisticalTransporterBase.insert` for a block beside a transporter: a sorter hands `item` over. Returns what
    /// went nowhere.
    fn insert_into_transporter(
        &mut self,
        at: P3,
        source: P3,
        item: ItemStack,
        round_robin: bool,
        rr: &mut Option<(P3, usize)>,
        out_color: Option<u8>,
    ) -> Option<ItemStack> {
        let id = self.block_id(at);
        let Some((_, nbt)) = self.read(at) else {
            return Some(item);
        };
        let mut stacks = load_transit(&*self.host, &nbt);
        let node = self.node_from(at, &id, &nbt);
        let Some(from) = tnet::side_between(at, source) else {
            return Some(item);
        };
        // `TransitRequest.eject`: the sender's colour, else the transporter's.
        let color = out_color.or(node.color);
        if !node.can_receive_from(from) {
            return Some(item);
        }
        let snap = self.snapshot(at, &nbt, &stacks, None);
        let plans = self.plan_routes(&snap, at, &item, color, Some(source), 1);
        let i = choose_plan(&plans, round_robin, rr)?;
        let plan = &plans[i];
        let sending = plan.sending.min(item.count());
        let mut moved = item.clone();
        let _ = moved.set_count(sending);
        let left = (sending < item.count()).then(|| {
            let mut rest = item.clone();
            let _ = rest.set_count(item.count() - sending);
            rest
        });
        let id_no = get_int(&nbt, "next_id");
        let mut s = Transit {
            id: id_no,
            item: moved,
            progress: 0,
            color,
            path: plan.path.clone(),
            ptype: PATH_DEST,
            orig: source,
            home: Some(source),
            idle: None,
            init: true,
            sync_t: 0,
            sync_pr: 0,
        };
        s.touch(self.now);
        stacks.push(s);
        self.store_other(at, &stacks, id_no + 1);
        left
    }
}

/// `getNewRRPath`: with round robin the sorter takes the destinations in turns, remembering the next one.
fn choose_plan(plans: &[Plan], round_robin: bool, rr: &mut Option<(P3, usize)>) -> Option<usize> {
    if plans.is_empty() {
        return None;
    }
    if !round_robin {
        return Some(0);
    }
    let mark = |p: &Plan| (p.dest, tnet::opposite(p.side));
    let n = plans.len();
    if n > 1
        && let Some(target) = *rr
        && let Some(i) = plans.iter().position(|p| mark(p) == target)
    {
        *rr = Some(mark(&plans[(i + 1) % n]));
        return Some(i);
    }
    *rr = Some(mark(&plans[if n > 1 { 1 } else { 0 }]));
    Some(0)
}

// --- The transporter block entity -------------------------------------------------------------------

struct TransporterPart;

#[mod_api::horizon_part]
impl BlockPart for TransporterPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for TransporterPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    /// The stacks of a transporter that is broken fall out of it.
    fn pre_remove_side_effects(&self, _: &mut dyn mod_api::LevelRandomHost, event: &mut PreRemoveSideEffectsEvent<'_>) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let items = TRANSIT_DROPS.lock().unwrap().as_mut().and_then(|m| m.remove(&pos));
        event.drops.extend(items.into_iter().flatten());
    }

    /// `LogisticalTransporterBase.onUpdateServer`: pulls (every 3 ticks, or 10 after a success) and moves the stacks.
    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(level) = transmit::level(&*host) else {
            return;
        };
        let Some(pull) = pull_of(event.entity.id()) else {
            return;
        };
        let pos = event.pos;
        let now = level.game_time(&*host).unwrap_or(0).max(0) as u64;
        let mut own = Own { pos, entity: &mut event.entity };
        let mut cx = Cx { host, own: &mut own, level, now };
        let mut stacks = load_transit(&*cx.host, &cx.own.entity.nbt);
        let mut changed = false;
        let delay = get_int(&cx.own.entity.nbt, "delay");
        if delay > 0 {
            put_int(&mut cx.own.entity.nbt, "delay", delay - 1);
        } else {
            put_int(&mut cx.own.entity.nbt, "delay", 3);
            changed |= cx.try_pull(pos, pull, &mut stacks);
        }
        if !stacks.is_empty() {
            let id = cx.own.entity.id().to_owned();
            let speed = transporter_speed(&id);
            let nbt = cx.own.entity.nbt.clone();
            let mut i = 0;
            while i < stacks.len() {
                let before = (stacks[i].sync_t, stacks[i].path.len());
                if cx.step(pos, &id, &nbt, speed, &mut stacks, i) {
                    changed |= before != (stacks[i].sync_t, stacks[i].path.len());
                    i += 1;
                } else {
                    stacks.remove(i);
                    changed = true;
                }
            }
            // The progress lives in the saved data; the clients count it on their own.
            save_transit(&*cx.host, &mut cx.own.entity.nbt, &stacks);
        }
        if changed || now % 40 == 0 {
            cx.store_own(pos, &stacks);
        }
    }
}

// --- The logistical sorter --------------------------------------------------------------------------

fn strings_of(nbt: &Compound, key: &str) -> Vec<String> {
    match nbt.get(&key.into()) {
        Some(Tag::List(l)) => l
            .iter()
            .filter_map(|t| match t {
                Tag::String(s) => s.to_string().ok(),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn load_sorter(nbt: &Compound) -> Sorter {
    let filters = match nbt.get(&"filters".into()) {
        Some(Tag::String(s)) => Sorter::parse_filters(&s.to_string().unwrap_or_default()),
        _ => Vec::new(),
    };
    Sorter {
        filters,
        auto_eject: get_int(nbt, "eject") == 1,
        single_item: get_int(nbt, "single") == 1,
        round_robin: get_int(nbt, "round_robin") == 1,
        disabled: strings_of(nbt, "disabled"),
        opts: match nbt.get(&"filter_opts".into()) {
            Some(Tag::String(o)) => Sorter::parse_opts(&o.to_string().unwrap_or_default()),
            _ => Default::default(),
        },
    }
}

pub(crate) fn save_sorter(nbt: &mut Compound, s: &Sorter) {
    nbt.insert("filters".into(), Tag::String(s.save_filters().as_str().into()));
    put_int(nbt, "eject", u32::from(s.auto_eject));
    put_int(nbt, "single", u32::from(s.single_item));
    put_int(nbt, "round_robin", u32::from(s.round_robin));
    nbt.insert("disabled".into(), Tag::List(s.disabled.iter().map(|d| Tag::String(d.as_str().into())).collect()));
    nbt.insert("filter_opts".into(), Tag::String(s.save_opts().as_str().into()));
}

struct SorterPart;

#[mod_api::horizon_part]
impl BlockPart for SorterPart {}

#[mod_api::horizon_part]
impl BlockEntityPart for SorterPart {
    fn block_entity_ticks(&self, _: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        super::common::client_tag(event.entity)
    }

    /// The Logistical Sorter, every 10 ticks: each filter in turn takes its items from the inventory behind the
    /// sorter, until one moves something; then, with auto-eject, any item. They go into the transporters in front (or
    /// straight into an inventory there), and what fits nowhere goes back.
    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(level) = transmit::level(&*host) else {
            return;
        };
        let pos = event.pos;
        let now = level.game_time(&*host).unwrap_or(0).max(0) as u64;
        let mut own = Own { pos, entity: &mut event.entity };
        let mut cx = Cx { host, own: &mut own, level, now };
        tick_sorter(&mut cx, pos);
    }
}

/// A flag the clients draw (`put_flag`).
fn put_flag(cx: &mut Cx<'_, '_, '_, '_>, key: &str, on: bool) {
    let mut flag = Compound::new();
    put_int(&mut flag, key, u32::from(on));
    super::common::put_client(cx.own.entity, &[key], flag);
}

fn tick_sorter(cx: &mut Cx<'_, '_, '_, '_>, pos: P3) {
    // Active for the four ticks after it moved something (`setActive(true)`, off again at 6 ticks of the 10 left).
    let now = cx.now;
    let moved = get_long(&cx.own.entity.nbt, "moved_at");
    put_flag(cx, "sorter_active", moved > 0 && now.saturating_sub(moved) <= 4);
    let delay = get_int(&cx.own.entity.nbt, "delay").saturating_sub(1);
    put_int(&mut cx.own.entity.nbt, "delay", delay);
    if delay > 0 {
        return;
    }
    put_int(&mut cx.own.entity.nbt, "delay", sorter::DELAY);
    let Some(d) = cx.level.block_property(&*cx.host, pos, "facing").and_then(|f| direction(&f)) else {
        return;
    };
    let settings = load_sorter(&cx.own.entity.nbt);
    let back = offset(pos, neg(d));
    let front = offset(pos, d);
    // The back inventory's face toward the sorter, and the front one's.
    let back_face = face_index(d);
    let front_face = face_index(neg(d));
    let max = settings.max_move();
    let mut choices: Vec<Option<&str>> = settings.active().map(|f| Some(f.as_str())).collect();
    if settings.auto_eject {
        choices.push(None);
    }
    for filter in choices {
        // A filter in size mode moves between its min and max items (`SorterFilter.mapInventory`);
        // auto-eject takes what no filter refuses the default for (`strictFinder`).
        let opts = filter.map(|f| settings.opts_of(f));
        let ranged = opts.filter(|o| o.size_mode && !settings.single_item);
        let take = ranged.map_or(max, |o| u32::from(o.max).clamp(1, max));
        let host: &dyn Host = &*cx.host;
        let in_tag = |tag: &str, item: &str| super::in_tag(host, item, tag.trim_start_matches('#'));
        let wanted = |s: &ItemStack| {
            let id = super::item_id(host, s);
            match filter {
                // An item filter that is not fuzzy takes only a plain stack of its item (`itemType.equals(toCheck)`).
                Some(f) => {
                    Sorter::matches_item(f, &id, &in_tag)
                        && (f.starts_with(['#', '@']) || settings.opts_of(f).fuzzy || plain(s))
                }
                None => settings.auto_eject_allows(&id, &in_tag),
            }
        };
        // A filter's colour, and the sorter's own for what it ejects without one (`request.eject(.., color, ..)`).
        let out_color = match filter {
            Some(_) => opts.and_then(|o| o.color),
            None => get_int(&cx.own.entity.nbt, "dcolor").checked_sub(1).map(|c| c as u8),
        };
        let Some(stack) = cx.extract_where(back, back_face, take, &wanted) else {
            continue;
        };
        if let Some(o) = ranged
            && stack.count() < u32::from(o.min)
        {
            cx.insert(back, back_face, stack);
            continue;
        }
        let left = if is_transporter(&cx.block_id(front)) {
            let mut rr = get_pos(&cx.own.entity.nbt, "rr").map(|p| (p, get_int(&cx.own.entity.nbt, "rr_side") as usize));
            let left = cx.insert_into_transporter(front, pos, stack, settings.round_robin, &mut rr, out_color);
            match rr {
                Some((p, side)) => {
                    cx.own.entity.nbt.insert("rr".into(), pos_tag(p));
                    put_int(&mut cx.own.entity.nbt, "rr_side", side as u32);
                }
                None => {
                    cx.own.entity.nbt.remove(&"rr".into());
                }
            }
            left
        } else {
            cx.insert(front, front_face, stack)
        };
        if let Some(left) = left
            && let Some(lost) = cx.insert(back, back_face, left)
        {
            cx.insert(back, 1, lost);
        }
        put_long(&mut cx.own.entity.nbt, "moved_at", now.max(1));
        return;
    }
}
