// Frequencies as `GuiFrequencySelector` shows and sets them (mekanism.common.lib.frequency): a name, a
// security mode and an owner, the text box that names a new one, and the packet a client sends for
// `PacketSetTileFrequency`.

/// `FrequencyLookup.MAX_FREQ_LENGTH`.
pub const MAX_FREQ_LENGTH: usize = 16;

/// `InputValidator.LETTER_OR_DIGIT.or(InputValidator.FREQUENCY_CHARS)`: what a frequency name may hold.
pub fn valid_char(c: char) -> bool {
    c.is_alphanumeric() || "- |'\"_+:()?!/@$`~,.#".contains(c)
}

/// `SecurityMode`, in its ordinal order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Security {
    #[default]
    Public,
    Private,
    Trusted,
}

impl Security {
    pub const ALL: [Security; 3] = [Security::Public, Security::Private, Security::Trusted];

    pub fn from_ordinal(n: i64) -> Security {
        match n {
            1 => Security::Private,
            2 => Security::Trusted,
            _ => Security::Public,
        }
    }

    pub fn ordinal(self) -> u8 {
        self as u8
    }

    /// `security.mekanism.public` and the others.
    pub fn name(self) -> &'static str {
        match self {
            Security::Public => "Public",
            Security::Private => "Private",
            Security::Trusted => "Trusted",
        }
    }
}

/// A frequency as the selector lists it (`Frequency` with its `FrequencyIdentity`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Entry {
    pub name: String,
    pub security: Security,
    pub owner: u128,
    pub owner_name: String,
    /// `IColorableFrequency.getColor`, an `EnumColor` ordinal (the teleporter's).
    pub color: u8,
}

impl Entry {
    /// The frequency's key in a server's tables: a public frequency is one per name, as Mekanism keeps
    /// them (`FrequencyManager` for public frequencies is global); a private or trusted one is its
    /// owner's.
    pub fn key(&self) -> String {
        key(&self.name, self.security, self.owner)
    }
}

pub fn key(name: &str, security: Security, owner: u128) -> String {
    match security {
        Security::Public => name.to_owned(),
        s => format!("{owner:032x}/{}/{name}", s.ordinal()),
    }
}

/// The frequencies the selector lists in `security` mode for player `viewer`
/// (`getPublicCache`/`getPrivateCache`/`getTrustedCache`): every public one, the viewer's own private
/// ones, and the trusted ones of owners who trust the viewer (trust lists are not ported, so the
/// viewer's own).
pub fn visible(entries: &[Entry], security: Security, viewer: u128) -> Vec<&Entry> {
    entries
        .iter()
        .filter(|e| e.security == security && (security == Security::Public || e.owner == viewer))
        .collect()
}

/// A line of the selector's list: the name, and the owner's name unless it is the private list.
pub fn list_line(entry: &Entry, mode: Security) -> String {
    if mode == Security::Private {
        entry.name.clone()
    } else {
        format!("{} ({})", entry.name, entry.owner_name)
    }
}

/// `EnumColor`'s colours a frequency can take (`EnumColor` in ordinal order), with their names and RGB.
pub const COLORS: [(&str, u32); 17] = [
    ("Black", 0x000000),
    ("Dark Blue", 0x0000A8),
    ("Dark Green", 0x00A800),
    ("Dark Aqua", 0x00A8A8),
    ("Dark Red", 0xA80000),
    ("Purple", 0xA800A8),
    ("Orange", 0xFFA800),
    ("Gray", 0xA8A8A8),
    ("Dark Gray", 0x545454),
    ("Indigo", 0x5454FF),
    ("Bright Green", 0x54FF54),
    ("Aqua", 0x54FFFF),
    ("Red", 0xFF5454),
    ("Pink", 0xFF54FF),
    ("Yellow", 0xFFFF54),
    ("White", 0xFFFFFF),
    ("Brown", 0x8B4513),
];

/// `TeleporterFrequency`'s colour when made: purple.
pub const DEFAULT_COLOR: u8 = 5;

/// The next (or previous) colour, as `ColorButton` cycles.
pub fn step_color(color: u8, next: bool) -> u8 {
    let n = COLORS.len() as u8;
    if next {
        (color + 1) % n
    } else {
        (color + n - 1) % n
    }
}

/// What a client asks of a frequency on the block at `pos` (`PacketSetTileFrequency`, `set` or not,
/// and `PacketSetFrequencyColor`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub op: Op,
    pub pos: [i32; 3],
    pub security: Security,
    pub owner: u128,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Set,
    Remove,
    ColorNext,
    ColorPrevious,
}

impl Request {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.op as u8];
        for c in self.pos {
            out.extend(c.to_be_bytes());
        }
        out.push(self.security.ordinal());
        out.extend(self.owner.to_be_bytes());
        out.extend(self.name.as_bytes());
        out
    }

    /// The request, or none for anything malformed: a name that is empty, too long or holds a
    /// character a frequency may not.
    pub fn decode(data: &[u8]) -> Option<Request> {
        let op = match *data.first()? {
            0 => Op::Set,
            1 => Op::Remove,
            2 => Op::ColorNext,
            3 => Op::ColorPrevious,
            _ => return None,
        };
        let int = |i: usize| Some(i32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?));
        let pos = [int(1)?, int(5)?, int(9)?];
        let security = match *data.get(13)? {
            n @ 0..=2 => Security::from_ordinal(i64::from(n)),
            _ => return None,
        };
        let owner = u128::from_be_bytes(data.get(14..30)?.try_into().ok()?);
        let name = std::str::from_utf8(data.get(30..)?).ok()?.to_owned();
        let ok = !name.is_empty()
            && name.chars().count() <= MAX_FREQ_LENGTH
            && name.chars().all(valid_char);
        ok.then_some(Request {
            op,
            pos,
            security,
            owner,
            name,
        })
    }
}

/// A one-line text box (`GuiTextField` over vanilla's `EditBox`), with its caret.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextBox {
    pub value: String,
    /// The caret, in characters.
    pub cursor: usize,
    pub focused: bool,
}

/// What a key did to a text box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typed {
    Nothing,
    Changed,
    /// Enter (`isConfirmation`): the box's enter handler runs.
    Enter,
}

impl TextBox {
    fn byte(&self, chars: usize) -> usize {
        self.value
            .char_indices()
            .nth(chars)
            .map_or(self.value.len(), |(i, _)| i)
    }

    fn len(&self) -> usize {
        self.value.chars().count()
    }

    /// Inserts what passes `valid` at the caret, up to `max` characters.
    pub fn insert(&mut self, text: &str, valid: impl Fn(char) -> bool, max: usize) -> bool {
        let room = max.saturating_sub(self.len());
        let add: String = text.chars().filter(|c| valid(*c)).take(room).collect();
        if add.is_empty() {
            return false;
        }
        let at = self.byte(self.cursor);
        self.value.insert_str(at, &add);
        self.cursor += add.chars().count();
        true
    }

    /// A key on the focused box (`EditBox.keyPressed` and `charTyped`): its name as Horizon gives it
    /// (`key.keyboard.backspace`...) and what it types.
    pub fn key(
        &mut self,
        key: &str,
        text: Option<&str>,
        valid: impl Fn(char) -> bool,
        max: usize,
    ) -> Typed {
        let len = self.len();
        self.cursor = self.cursor.min(len);
        match key {
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => return Typed::Enter,
            "key.keyboard.backspace" if self.cursor > 0 => {
                let (a, b) = (self.byte(self.cursor - 1), self.byte(self.cursor));
                self.value.replace_range(a..b, "");
                self.cursor -= 1;
                return Typed::Changed;
            }
            "key.keyboard.delete" if self.cursor < len => {
                let (a, b) = (self.byte(self.cursor), self.byte(self.cursor + 1));
                self.value.replace_range(a..b, "");
                return Typed::Changed;
            }
            "key.keyboard.left" => self.cursor = self.cursor.saturating_sub(1),
            "key.keyboard.right" => self.cursor = (self.cursor + 1).min(len),
            "key.keyboard.home" => self.cursor = 0,
            "key.keyboard.end" => self.cursor = len,
            _ => {}
        }
        match text {
            Some(t) if self.insert(t, valid, max) => Typed::Changed,
            _ => Typed::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_frequencies_are_one_per_name_and_private_ones_their_owners() {
        let e = Entry {
            name: "Base".into(),
            security: Security::Public,
            owner: 7,
            ..Entry::default()
        };
        assert_eq!(e.key(), "Base");
        let p = Entry {
            security: Security::Private,
            ..e.clone()
        };
        assert_eq!(p.key(), format!("{:032x}/1/Base", 7));
        let all = [
            e.clone(),
            p.clone(),
            Entry {
                owner: 8,
                ..p.clone()
            },
        ];
        assert_eq!(visible(&all, Security::Public, 9), vec![&e]);
        assert_eq!(visible(&all, Security::Private, 7), vec![&p]);
        assert_eq!(
            list_line(
                &Entry {
                    owner_name: "Dev".into(),
                    ..e
                },
                Security::Public
            ),
            "Base (Dev)"
        );
    }

    #[test]
    fn requests_round_trip_and_bad_names_are_refused() {
        let r = Request {
            op: Op::Remove,
            pos: [1, -64, 300],
            security: Security::Trusted,
            owner: u128::MAX - 3,
            name: "Home #2".into(),
        };
        assert_eq!(Request::decode(&r.encode()), Some(r.clone()));
        let long = Request {
            name: "a".repeat(17),
            ..r.clone()
        };
        assert_eq!(Request::decode(&long.encode()), None);
        let bad = Request {
            name: "a\u{a7}b".into(),
            ..r
        };
        assert_eq!(Request::decode(&bad.encode()), None);
    }

    #[test]
    fn the_text_box_edits_at_its_caret() {
        let mut b = TextBox::default();
        assert_eq!(
            b.key("key.keyboard.a", Some("ab"), valid_char, MAX_FREQ_LENGTH),
            Typed::Changed
        );
        b.key("key.keyboard.left", None, valid_char, MAX_FREQ_LENGTH);
        b.key(
            "key.keyboard.x",
            Some("x\u{a7}"),
            valid_char,
            MAX_FREQ_LENGTH,
        );
        assert_eq!(b.value, "axb");
        b.key("key.keyboard.backspace", None, valid_char, MAX_FREQ_LENGTH);
        assert_eq!((b.value.as_str(), b.cursor), ("ab", 1));
        b.key(
            "key.keyboard.v",
            Some(&"z".repeat(30)),
            valid_char,
            MAX_FREQ_LENGTH,
        );
        assert_eq!(b.value.chars().count(), MAX_FREQ_LENGTH);
        assert_eq!(
            b.key("key.keyboard.enter", None, valid_char, MAX_FREQ_LENGTH),
            Typed::Enter
        );
        assert_eq!(step_color(0, false), 16);
    }
}
