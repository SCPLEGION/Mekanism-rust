//! Block security (`SecurityFrequency`, `SecurityUtils`): an owner's default mode, the override that makes it
//! apply to all their blocks, and the players they trust. The Security Desk edits it.

pub use crate::frequency::Security as Mode;

pub const DESK: &str = "mekanism:security_desk";
/// The most characters in a player's name (`SharedConstants.MAX_PLAYER_NAME_LENGTH`).
pub const MAX_NAME: usize = 16;
/// The key of the security an item or block keeps in its custom data.
pub const KEY: &str = "mekanism:security";

/// A security frequency: the owner's default mode, whether it overrides their blocks' own modes, and the
/// players they trust as (uuid, name).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Data {
    pub mode: Mode,
    pub overridden: bool,
    pub trusted: Vec<(String, String)>,
}

impl Data {
    pub fn save(&self) -> String {
        let trusted: Vec<String> = self.trusted.iter().map(|(u, n)| format!("{u}={n}")).collect();
        format!("{}|{}|{}", self.mode.ordinal(), u8::from(self.overridden), trusted.join(";"))
    }

    pub fn load(text: &str) -> Data {
        let mut parts = text.splitn(3, '|');
        let mode = Mode::from_ordinal(parts.next().and_then(|m| m.parse().ok()).unwrap_or(0));
        let overridden = parts.next() == Some("1");
        let trusted = parts
            .next()
            .unwrap_or("")
            .split(';')
            .filter_map(|e| e.split_once('='))
            .map(|(u, n)| (u.to_owned(), n.to_owned()))
            .collect();
        Data { mode, overridden, trusted }
    }

    /// `SecurityFrequency.addTrusted`: a player not trusted yet.
    pub fn add_trusted(&mut self, uuid: &str, name: &str) -> bool {
        if self.trusted.iter().any(|(u, _)| u == uuid) {
            return false;
        }
        self.trusted.push((uuid.to_owned(), name.to_owned()));
        true
    }

    /// `SecurityFrequency.removeTrusted`: the uuid removed.
    pub fn remove_trusted(&mut self, index: usize) -> Option<String> {
        (index < self.trusted.len()).then(|| self.trusted.remove(index).0)
    }

    pub fn names(&self) -> Vec<String> {
        self.trusted.iter().map(|(_, n)| n.clone()).collect()
    }
}

/// `StringUtil.isValidPlayerName`: 1 to 16 printable characters, no spaces.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= MAX_NAME && name.chars().all(|c| c.is_ascii_graphic())
}

/// `InputValidator.USERNAME`: a character a name may hold.
pub fn valid_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `SecurityUtils.getEffectiveSecurityMode`: the owner's override replaces the block's own mode.
pub fn effective(own: Mode, data: Option<&Data>) -> (Mode, bool) {
    match data {
        Some(d) if d.overridden => (d.mode, true),
        _ => (own, false),
    }
}

/// `SecurityUtils.canAccess`: whether `player` may use a block of `owner` in `mode`.
pub fn can_access(mode: Mode, owner: &str, player: &str, data: Option<&Data>) -> bool {
    if owner.is_empty() || owner == player {
        return true;
    }
    match mode {
        Mode::Public => true,
        Mode::Private => false,
        Mode::Trusted => data.is_some_and(|d| d.trusted.iter().any(|(u, _)| u == player)),
    }
}

/// `SecurityMode` stepped for the security tab (`NEXT_SECURITY_MODE` and `PREVIOUS_SECURITY_MODE`).
pub fn step(mode: Mode, forward: bool) -> Mode {
    let n = i64::from(mode.ordinal());
    Mode::from_ordinal(if forward { (n + 1) % 3 } else { (n + 2) % 3 })
}

/// `ISecurityUtils.moreRestrictive`: public is the loosest, then trusted, then private.
pub fn more_restrictive(old: Mode, new: Mode) -> bool {
    let rank = |m: Mode| match m {
        Mode::Public => 0,
        Mode::Trusted => 1,
        Mode::Private => 2,
    };
    rank(new) > rank(old)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    SetMode(Mode),
    ToggleOverride,
    Remove(u8),
    Add(String),
}

pub fn encode_request(op: &Op, pos: [i32; 3]) -> Vec<u8> {
    let (tag, arg, name): (u8, u8, &str) = match op {
        Op::SetMode(m) => (0, m.ordinal(), ""),
        Op::ToggleOverride => (1, 0, ""),
        Op::Remove(i) => (2, *i, ""),
        Op::Add(n) => (3, 0, n),
    };
    let mut out = vec![tag, arg];
    for c in pos {
        out.extend(c.to_be_bytes());
    }
    out.extend(name.as_bytes());
    out
}

pub fn decode_request(data: &[u8]) -> Option<(Op, [i32; 3])> {
    if data.len() < 14 {
        return None;
    }
    let arg = data[1];
    let int = |i: usize| i32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    let op = match data[0] {
        0 => Op::SetMode(Mode::from_ordinal(i64::from(arg))),
        1 => Op::ToggleOverride,
        2 => Op::Remove(arg),
        3 => Op::Add(std::str::from_utf8(&data[14..]).ok()?.to_owned()),
        _ => return None,
    };
    Some((op, [int(2), int(6), int(10)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let mut d = Data { mode: Mode::Trusted, overridden: true, ..Data::default() };
        assert!(d.add_trusted("ab", "Alex"));
        assert!(!d.add_trusted("ab", "Alex"));
        assert!(d.add_trusted("cd", "Steve"));
        assert_eq!(Data::load(&d.save()), d);
        assert_eq!(d.names(), ["Alex", "Steve"]);
        assert_eq!(d.remove_trusted(0).as_deref(), Some("ab"));
        assert_eq!(d.remove_trusted(5), None);
        assert_eq!(Data::load(""), Data::default());
    }

    #[test]
    fn names_are_checked() {
        assert!(valid_name("Steve_1"));
        assert!(!valid_name(""));
        assert!(!valid_name("a name"));
        assert!(!valid_name("12345678901234567"));
    }

    #[test]
    fn access_follows_the_mode() {
        let mut d = Data::default();
        d.add_trusted("friend", "F");
        assert!(can_access(Mode::Private, "me", "me", None));
        assert!(!can_access(Mode::Private, "me", "friend", Some(&d)));
        assert!(can_access(Mode::Trusted, "me", "friend", Some(&d)));
        assert!(!can_access(Mode::Trusted, "me", "other", Some(&d)));
        assert!(can_access(Mode::Public, "me", "other", None));
        assert!(can_access(Mode::Private, "", "other", None));
    }

    #[test]
    fn the_override_replaces_the_blocks_mode() {
        let d = Data { mode: Mode::Private, overridden: true, ..Data::default() };
        assert_eq!(effective(Mode::Public, Some(&d)), (Mode::Private, true));
        assert_eq!(effective(Mode::Public, None), (Mode::Public, false));
        let off = Data { overridden: false, ..d };
        assert_eq!(effective(Mode::Trusted, Some(&off)), (Mode::Trusted, false));
    }

    #[test]
    fn modes_step_and_rank() {
        assert_eq!(step(Mode::Public, true), Mode::Private);
        assert_eq!(step(Mode::Public, false), Mode::Trusted);
        assert_eq!(step(Mode::Trusted, true), Mode::Public);
        assert!(more_restrictive(Mode::Public, Mode::Trusted));
        assert!(more_restrictive(Mode::Trusted, Mode::Private));
        assert!(!more_restrictive(Mode::Private, Mode::Public));
    }

    #[test]
    fn requests_round_trip() {
        for op in [Op::SetMode(Mode::Trusted), Op::ToggleOverride, Op::Remove(3), Op::Add("Alex".into())] {
            assert_eq!(decode_request(&encode_request(&op, [1, -2, 3])), Some((op, [1, -2, 3])));
        }
        assert_eq!(decode_request(&[9; 14]), None);
    }
}
