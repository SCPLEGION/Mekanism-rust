//! The logistical transporter network: `TransporterPathfinder` (the A* search to a destination and the
//! idle paths), `TransporterStack`'s path rules and `ConnectionType`, as plain functions over a snapshot of
//! the transporters, so the search runs without a game. Directions are indexed down, up, north, south, west,
//! east (`Direction.ordinal()`); `d ^ 1` is the opposite.

use std::collections::HashMap;

pub type Pos = [i32; 3];

pub const DIRS: [Pos; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

pub fn offset(p: Pos, d: usize) -> Pos {
    [p[0] + DIRS[d][0], p[1] + DIRS[d][1], p[2] + DIRS[d][2]]
}

pub fn opposite(d: usize) -> usize {
    d ^ 1
}

/// The direction from `a` to the block `b` next to it (`WorldUtils.sideDifference(b, a)`).
pub fn side_between(a: Pos, b: Pos) -> Option<usize> {
    DIRS.iter().position(|d| [a[0] + d[0], a[1] + d[1], a[2] + d[2]] == b)
}

/// `WorldUtils.distanceBetween`: the straight distance between two block positions.
pub fn distance(a: Pos, b: Pos) -> f64 {
    let d = |i: usize| f64::from(a[i] - b[i]);
    (d(0) * d(0) + d(1) * d(1) + d(2) * d(2)).sqrt()
}

/// `ConnectionType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conn {
    Normal,
    Push,
    Pull,
    None,
}

impl Conn {
    /// From the two bits a face takes in a transmitter's `conn` (0 normal, 1 push, 2 pull, 3 none).
    pub fn from_bits(bits: u32) -> Conn {
        match bits & 3 {
            0 => Conn::Normal,
            1 => Conn::Push,
            2 => Conn::Pull,
            _ => Conn::None,
        }
    }

    pub fn can_accept(self) -> bool {
        matches!(self, Conn::Normal | Conn::Pull)
    }

    pub fn can_send_to(self) -> bool {
        matches!(self, Conn::Normal | Conn::Push)
    }
}

/// `DiversionTransporter.DiversionControl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Diversion {
    #[default]
    Disabled,
    High,
    Low,
}

impl Diversion {
    pub fn from_id(id: u32) -> Diversion {
        match id {
            1 => Diversion::High,
            2 => Diversion::Low,
            _ => Diversion::Disabled,
        }
    }

    pub fn next(self) -> Diversion {
        match self {
            Diversion::Disabled => Diversion::High,
            Diversion::High => Diversion::Low,
            Diversion::Low => Diversion::Disabled,
        }
    }

    /// `modeReqsMet`: a high face works with a redstone signal, a low one without.
    pub fn met(self, powered: bool) -> bool {
        match self {
            Diversion::Disabled => true,
            Diversion::High => powered,
            Diversion::Low => !powered,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Diversion::Disabled => "Always active",
            Diversion::High => "Active with signal",
            Diversion::Low => "Active without signal",
        }
    }
}

/// `EnumColor` in `ordinal()` order: the colour name and its rgb.
pub const COLORS: [(&str, u32); 18] = [
    ("Black", 0x404040),
    ("Blue", 0x366BD0),
    ("Green", 0x59C15F),
    ("Cyan", 0x00F3D0),
    ("Dark Red", 0xC9071F),
    ("Purple", 0xA460D9),
    ("Orange", 0xFFA160),
    ("Light Gray", 0xCFCFCF),
    ("Gray", 0x7A7A7A),
    ("Light Blue", 0x559EFF),
    ("Lime", 0x75FF89),
    ("Aqua", 0x30FFF9),
    ("Red", 0xFF383C),
    ("Magenta", 0xD55ECB),
    ("Yellow", 0xFFDD4F),
    ("White", 0xFFFFFF),
    ("Brown", 0xA17649),
    ("Pink", 0xFFBCC4),
];

/// `TransporterUtils.increment`: the next colour, none after the last, the first after none.
pub fn next_color(color: Option<u8>) -> Option<u8> {
    match color {
        None => Some(0),
        Some(c) if usize::from(c) + 1 >= COLORS.len() => None,
        Some(c) => Some(c + 1),
    }
}

/// `TransporterUtils.decrement`: the previous colour, the last after none, none before the first.
pub fn previous_color(color: Option<u8>) -> Option<u8> {
    match color {
        None => Some(COLORS.len() as u8 - 1),
        Some(0) => None,
        Some(c) => Some(c - 1),
    }
}

/// What the pathfinder needs of one transporter (`LogisticalTransporterBase`).
#[derive(Clone, Debug)]
pub struct Node {
    /// `getCost()`: 1 for the tiers' ultimate speed, more for slower ones, 1000 for a restrictive transporter.
    pub cost: f64,
    pub color: Option<u8>,
    pub conn: [Conn; 6],
    /// A diversion transporter's faces whose redstone condition is not met: they do not connect.
    pub blocked: [bool; 6],
}

impl Node {
    pub fn can_connect(&self, d: usize) -> bool {
        self.conn[d] != Conn::None && !self.blocked[d]
    }

    pub fn can_emit_to(&self, d: usize) -> bool {
        self.can_connect(d) && self.conn[d].can_send_to()
    }

    pub fn can_receive_from(&self, d: usize) -> bool {
        self.can_connect(d) && self.conn[d].can_accept()
    }
}

/// `getCost` of a tier: the ultimate tier's speed over its own.
pub fn tier_cost(ultimate_speed: u32, speed: u32) -> f64 {
    f64::from(ultimate_speed) / f64::from(speed.max(1))
}

#[derive(Clone, Debug, Default)]
pub struct Network {
    pub nodes: HashMap<Pos, Node>,
}

impl Network {
    /// `isValidTransmitterBasic` between two transporters: both connect to each other and their colours match
    /// (a transporter without a colour joins any).
    pub fn joined(&self, a: Pos, d: usize) -> bool {
        let (Some(x), Some(y)) = (self.nodes.get(&a), self.nodes.get(&offset(a, d))) else {
            return false;
        };
        (x.color.is_none() || y.color.is_none() || x.color == y.color) && x.can_connect(d) && y.can_connect(opposite(d))
    }

    /// `TransporterStack.canInsertToTransporter`: a stack of `color` at `from` may go on through its side `d`.
    pub fn can_enter(&self, color: Option<u8>, from: Pos, d: usize) -> bool {
        let Some(t) = self.nodes.get(&offset(from, d)) else {
            return false;
        };
        (t.color.is_none() || t.color == color) && t.can_connect(opposite(d)) && self.nodes.get(&from).is_none_or(|f| f.can_connect(d))
    }

    /// The transporters joined to `start` (`InventoryNetwork`), `start` included, nearest first.
    pub fn component(&self, start: Pos) -> Vec<Pos> {
        let mut out = vec![start];
        let mut seen = std::collections::HashSet::from([start]);
        let mut i = 0;
        while i < out.len() {
            let at = out[i];
            i += 1;
            for d in 0..6 {
                let next = offset(at, d);
                if !seen.contains(&next) && self.joined(at, d) {
                    seen.insert(next);
                    out.push(next);
                }
            }
        }
        out
    }

    /// `checkPath`: every transporter on the path is still there and none has a colour other than the stack's.
    pub fn check_path(&self, path: &[Pos], color: Option<u8>) -> bool {
        path.iter().all(|p| self.nodes.get(p).is_some_and(|n| n.color.is_none() || n.color == color))
    }
}

/// A path to a container (`Destination`): the transporters from the start to the last one, the side of the
/// container the stack goes in through (as seen from the last transporter) and the score to sort them by.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub path: Vec<Pos>,
    pub side: usize,
    pub score: f64,
}

/// `TransporterPathfinder.Pathfinder`: the A* search from the transporter `start` to the container `dest`.
/// `accepts(d)` says whether the container takes the item through the side facing the transporter at `d`;
/// `home` is the stack's home container, which it may return to even through a pull face.
pub fn find_path(net: &Network, start: Pos, dest: Pos, color: Option<u8>, home: Option<Pos>, accepts: &dyn Fn(usize) -> bool) -> Option<Found> {
    let valid_dest = |at: Pos, d: usize| -> Option<usize> {
        let node = net.nodes.get(&at)?;
        (offset(at, d) == dest && accepts(d) && (node.can_emit_to(d) || (home == Some(dest) && node.can_connect(d)))).then_some(d)
    };
    let total = distance(start, dest);
    let mut has_valid_direction = false;
    for d in 0..6 {
        if net.can_enter(color, start, d) {
            has_valid_direction = true;
            break;
        } else if let Some(side) = valid_dest(start, d) {
            return Some(Found { path: vec![start], side, score: distance(start, dest) });
        }
    }
    if !has_valid_direction {
        return None;
    }
    let max_search = (2.0 * total).max(4.0);
    // Insertion order keeps ties deterministic (`HashSet` order in Java).
    let mut open: Vec<Pos> = vec![start];
    let mut closed = std::collections::HashSet::new();
    let mut g: HashMap<Pos, f64> = HashMap::from([(start, 0.0)]);
    let mut f: HashMap<Pos, f64> = HashMap::from([(start, total)]);
    let mut nav: HashMap<Pos, Pos> = HashMap::new();
    while !open.is_empty() {
        let mut best = 0;
        for (i, n) in open.iter().enumerate() {
            if f[n] < f[&open[best]] {
                best = i;
            }
        }
        let current = open.remove(best);
        closed.insert(current);
        if distance(start, current) > max_search {
            continue;
        }
        let current_score = g[&current];
        for d in 0..6 {
            let neighbor = offset(current, d);
            if net.can_enter(color, current, d) {
                let tentative = current_score + net.nodes[&neighbor].cost;
                if closed.contains(&neighbor) && tentative >= g.get(&neighbor).copied().unwrap_or(f64::MAX) {
                    continue;
                }
                if !open.contains(&neighbor) || tentative < g.get(&neighbor).copied().unwrap_or(f64::MAX) {
                    nav.insert(neighbor, current);
                    g.insert(neighbor, tentative);
                    f.insert(neighbor, tentative + distance(neighbor, dest));
                    if !open.contains(&neighbor) {
                        open.push(neighbor);
                    }
                }
            } else if let Some(side) = valid_dest(current, d) {
                let mut path = vec![current];
                let mut at = current;
                while let Some(&p) = nav.get(&at) {
                    path.push(p);
                    at = p;
                }
                path.reverse();
                return Some(Found { path, side, score: current_score + distance(current, dest) });
            }
        }
    }
    None
}

/// The order destinations are tried in (`Destination.compareTo`): the lowest score, then the shortest path.
pub fn sort_found(found: &mut [Found]) {
    found.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal).then(a.path.len().cmp(&b.path.len())));
}

/// What a stack does when it has nowhere to go (`IdlePathData`): its path, from the start on, and whether it
/// goes home (`Path.HOME`) or just wanders (`Path.NONE`). Updates `idle_dir`.
pub struct Idle {
    pub path: Vec<Pos>,
    pub home: bool,
    /// Set when the stack found somewhere to go after all: the path to it.
    pub dest: Option<Found>,
}

/// `IdlePath.find` without a home: the stack keeps going in its idle direction along transporters it may enter,
/// and turns when it cannot. `side_closest` is the side the stack is nearest (`TransporterStack.getSide`).
/// A destination the stack could reach after all is looked for by `reroute`, which gets the start and returns its path.
pub fn idle_path(
    net: &Network,
    start: Pos,
    color: Option<u8>,
    idle_dir: &mut Option<usize>,
    side_closest: usize,
    reroute: &mut dyn FnMut() -> Option<Found>,
) -> Option<Idle> {
    let start_node = net.nodes.get(&start);
    let loop_side = |side: usize| -> Vec<Pos> {
        let mut list = vec![start];
        let mut last = start;
        while net.can_enter(color, last, side) {
            last = offset(last, side);
            list.push(last);
        }
        list
    };
    let can_go = |d: usize| net.can_enter(color, start, d);
    let find_side = |idle: Option<usize>| -> Option<usize> {
        match idle {
            None => (0..6).find(|&d| can_go(d)),
            Some(dir) => (0..6).filter(|&d| d != opposite(dir)).find(|&d| can_go(d)).or_else(|| can_go(opposite(dir)).then_some(opposite(dir))),
        }
    };
    let finish = |list: Vec<Pos>| Idle { path: list, home: false, dest: None };
    let get_destination = |idle_dir: &mut Option<usize>| -> Option<Idle> {
        if let Some(side) = find_side(*idle_dir) {
            *idle_dir = Some(side);
            return Some(finish(loop_side(side)));
        }
        let node = start_node?;
        let closest = idle_dir.unwrap_or(side_closest);
        for side in (0..6).filter(|&d| d != closest) {
            if node.conn[side] != Conn::None {
                *idle_dir = Some(side);
                return Some(finish(vec![start, offset(start, side)]));
            }
        }
        None
    };
    let Some(dir) = *idle_dir else {
        return get_destination(idle_dir);
    };
    if can_go(dir) {
        return Some(finish(loop_side(dir)));
    }
    if start_node.is_some()
        && let Some(found) = reroute()
    {
        *idle_dir = None;
        return Some(Idle { path: found.path.clone(), home: false, dest: Some(found) });
    }
    get_destination(idle_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: i32, cost: f64) -> Network {
        let mut net = Network::default();
        for x in 0..n {
            net.nodes.insert([x, 0, 0], Node { cost, color: None, conn: [Conn::Normal; 6], blocked: [false; 6] });
        }
        net
    }

    #[test]
    fn straight_line_reaches_the_container() {
        let net = line(4, 1.0);
        let found = find_path(&net, [0, 0, 0], [4, 0, 0], None, None, &|_| true).expect("path");
        assert_eq!(found.path, vec![[0, 0, 0], [1, 0, 0], [2, 0, 0], [3, 0, 0]]);
        assert_eq!(found.side, 5);
    }

    #[test]
    fn colours_must_match_the_stack() {
        let mut net = line(3, 1.0);
        net.nodes.get_mut(&[1, 0, 0]).unwrap().color = Some(2);
        assert!(find_path(&net, [0, 0, 0], [3, 0, 0], None, None, &|_| true).is_none());
        assert!(find_path(&net, [0, 0, 0], [3, 0, 0], Some(2), None, &|_| true).is_some());
        assert!(find_path(&net, [0, 0, 0], [3, 0, 0], Some(3), None, &|_| true).is_none());
    }

    #[test]
    fn restrictive_costs_steer_the_path_around() {
        // A detour above the line: through the restrictive block (cost 1000) or around it.
        let mut net = line(3, 1.0);
        net.nodes.get_mut(&[1, 0, 0]).unwrap().cost = 1000.0;
        for p in [[0, 1, 0], [1, 1, 0], [2, 1, 0]] {
            net.nodes.insert(p, Node { cost: 1.0, color: None, conn: [Conn::Normal; 6], blocked: [false; 6] });
        }
        let found = find_path(&net, [0, 0, 0], [3, 0, 0], None, None, &|_| true).expect("path");
        assert!(!found.path.contains(&[1, 0, 0]), "{:?}", found.path);
        assert!(found.score < 1000.0);
    }

    #[test]
    fn a_pull_face_is_not_a_destination_unless_it_is_home() {
        let mut net = line(1, 1.0);
        net.nodes.get_mut(&[0, 0, 0]).unwrap().conn[5] = Conn::Pull;
        assert!(find_path(&net, [0, 0, 0], [1, 0, 0], None, None, &|_| true).is_none());
        assert!(find_path(&net, [0, 0, 0], [1, 0, 0], None, Some([1, 0, 0]), &|_| true).is_some());
    }

    #[test]
    fn disabled_faces_do_not_join() {
        let mut net = line(2, 1.0);
        net.nodes.get_mut(&[0, 0, 0]).unwrap().conn[5] = Conn::None;
        assert_eq!(net.component([0, 0, 0]).len(), 1);
    }

    #[test]
    fn colour_cycle_ends_in_none() {
        let mut c = None;
        for _ in 0..COLORS.len() {
            c = next_color(c);
            assert!(c.is_some());
        }
        assert_eq!(next_color(c), None);
    }

    #[test]
    fn idle_stacks_run_along_the_line() {
        let net = line(4, 1.0);
        let mut dir = None;
        let idle = idle_path(&net, [0, 0, 0], None, &mut dir, 0, &mut || None).expect("idle");
        assert_eq!(idle.path.last(), Some(&[3, 0, 0]));
        assert_eq!(dir, Some(5));
    }
}
