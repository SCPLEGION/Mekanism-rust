//! Side configuration, simplified: each face of a machine is left as the machine makes it, turned off,
//! or limited to taking in or giving out, for each transmission type (items, chemicals, energy), as
//! Mekanism's `TileComponentConfig` keeps a mode per face and type. There is no screen: the Configurator
//! cycles a face of the type it is set to.

/// The kinds of thing a face passes (`TransmissionType`), in the Configurator's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transmission {
    #[default]
    Item,
    Chemical,
    Energy,
}

impl Transmission {
    pub const ALL: [Transmission; 3] = [Self::Item, Self::Chemical, Self::Energy];

    pub fn name(self) -> &'static str {
        match self {
            Self::Item => "items",
            Self::Chemical => "chemicals",
            Self::Energy => "energy",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The block entity NBT key of this type's faces. Energy keeps `sides`, the key the single
    /// configuration used before.
    pub fn key(self) -> &'static str {
        match self {
            Self::Item => "sides_item",
            Self::Chemical => "sides_chemical",
            Self::Energy => "sides",
        }
    }

    /// The Configurator's next mode (`ConfiguratorMode.adjust`).
    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }
}

/// What one face of a block does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideMode {
    /// As the machine does it without configuration.
    Default,
    /// Neither takes in nor gives out.
    Off,
    /// Takes in only.
    Input,
    /// Gives out only.
    Output,
}

impl SideMode {
    const ALL: [SideMode; 4] = [
        SideMode::Default,
        SideMode::Off,
        SideMode::Input,
        SideMode::Output,
    ];

    pub fn next(self) -> SideMode {
        Self::ALL[(self as usize + 1) % 4]
    }

    pub fn name(self) -> &'static str {
        match self {
            SideMode::Default => "Default",
            SideMode::Off => "None",
            SideMode::Input => "Input",
            SideMode::Output => "Output",
        }
    }

    pub fn takes_in(self) -> bool {
        matches!(self, SideMode::Default | SideMode::Input)
    }

    pub fn gives_out(self) -> bool {
        matches!(self, SideMode::Default | SideMode::Output)
    }
}

/// The six faces' modes packed two bits each, in the order down, up, north, south, west, east. Zero is
/// every face at its default, so a block that was never configured needs no NBT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sides(pub u32);

impl Sides {
    pub fn get(self, face: usize) -> SideMode {
        SideMode::ALL[((self.0 >> (2 * face)) & 3) as usize]
    }

    pub fn set(&mut self, face: usize, mode: SideMode) {
        self.0 = (self.0 & !(3 << (2 * face))) | ((mode as u32) << (2 * face));
    }

    /// The Configurator's click: the face's next mode.
    pub fn cycle(&mut self, face: usize) -> SideMode {
        let mode = self.get(face).next();
        self.set(face, mode);
        mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmission_types_cycle_and_keep_their_own_keys() {
        use Transmission::*;
        assert_eq!(
            [Item.next(), Chemical.next(), Energy.next()],
            [Chemical, Energy, Item]
        );
        assert_eq!(Transmission::from_name("chemicals"), Some(Chemical));
        assert_eq!(Energy.key(), "sides");
        assert_ne!(Item.key(), Chemical.key());
    }

    #[test]
    fn faces_cycle_independently_and_back_to_default() {
        let mut s = Sides::default();
        assert_eq!(s.cycle(2), SideMode::Off);
        assert_eq!(s.cycle(2), SideMode::Input);
        assert_eq!(s.cycle(5), SideMode::Off);
        assert_eq!(s.get(2), SideMode::Input);
        assert!(s.get(2).takes_in() && !s.get(2).gives_out());
        assert_eq!(s.cycle(2), SideMode::Output);
        assert_eq!(s.cycle(2), SideMode::Default);
        s.cycle(5);
        s.cycle(5);
        s.cycle(5);
        assert_eq!(s, Sides::default());
    }
}
