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

/// The Configurator's modes (`ItemConfigurator.ConfiguratorMode`): configure a type, empty a machine, rotate it or
/// wrench it. (Fluids and heat have no configuration here, so no mode of their own.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfMode {
    Configure(Transmission),
    Empty,
    Rotate,
    Wrench,
}

impl ConfMode {
    pub const ALL: [ConfMode; 6] = [
        Self::Configure(Transmission::Item),
        Self::Configure(Transmission::Chemical),
        Self::Configure(Transmission::Energy),
        Self::Empty,
        Self::Rotate,
        Self::Wrench,
    ];

    /// What the item keeps: the type's name, or the mode's.
    pub fn name(self) -> &'static str {
        match self {
            Self::Configure(t) => t.name(),
            Self::Empty => "empty",
            Self::Rotate => "rotate",
            Self::Wrench => "wrench",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    /// `adjust(1)`.
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// The mode's text (`getTextComponent`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Configure(Transmission::Item) => "Configurate (Items)",
            Self::Configure(Transmission::Chemical) => "Configurate (Chemicals)",
            Self::Configure(Transmission::Energy) => "Configurate (Energy)",
            Self::Empty => "Empty",
            Self::Rotate => "Rotate",
            Self::Wrench => "Wrench",
        }
    }

    pub fn transmission(self) -> Option<Transmission> {
        match self {
            Self::Configure(t) => Some(t),
            _ => None,
        }
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

impl SideMode {
    /// The mode as the side configuration window shows it (`DataType`): its name and colour
    /// (`EnumColor`). A face left at its default takes in and gives out, which is Mekanism's
    /// "Input/Output".
    pub fn data_type(self) -> (&'static str, u32, char) {
        match self {
            SideMode::Off => ("None", 0xCFCFCF, '7'),
            SideMode::Input => ("Input", 0xC9071F, '4'),
            SideMode::Output => ("Output", 0x366BD0, '1'),
            SideMode::Default => ("Input/Output", 0xA460D9, '5'),
        }
    }

    /// The window's order (`DataType`'s): None, Input, Output, Input/Output.
    const WINDOW: [SideMode; 4] = [
        SideMode::Off,
        SideMode::Input,
        SideMode::Output,
        SideMode::Default,
    ];

    /// A left click on a face in the window (`DataType.getNext`).
    pub fn window_next(self) -> SideMode {
        let i = Self::WINDOW.iter().position(|m| *m == self).unwrap_or(0);
        Self::WINDOW[(i + 1) % 4]
    }

    /// A right click (`DataType.getPrevious`).
    pub fn window_previous(self) -> SideMode {
        let i = Self::WINDOW.iter().position(|m| *m == self).unwrap_or(0);
        Self::WINDOW[(i + 3) % 4]
    }
}

/// The sides relative to a block's front, in `RelativeSide`'s order.
pub const RELATIVE: [&str; 6] = ["Front", "Left", "Right", "Back", "Top", "Bottom"];

/// The face (down, up, north, south, west, east) that relative side `side` is on a block facing
/// face `facing` (`RelativeSide.getDirection`).
pub fn relative_face(facing: usize, side: usize) -> usize {
    const OPPOSITE: [usize; 6] = [1, 0, 3, 2, 5, 4];
    // Direction.getClockWise / getCounterClockWise around the vertical axis.
    let clockwise = |f: usize| [0, 1, 5, 4, 2, 3][f];
    let counter = |f: usize| [0, 1, 4, 5, 3, 2][f];
    let vertical = facing < 2;
    match side {
        0 => facing,
        1 if vertical => 5,
        1 => clockwise(facing),
        2 if vertical => 4,
        2 => counter(facing),
        3 => OPPOSITE[facing],
        4 => match facing {
            0 => 2,
            1 => 3,
            _ => 1,
        },
        _ => match facing {
            0 => 3,
            1 => 2,
            _ => 0,
        },
    }
}

/// The six faces' modes packed two bits each, in the order down, up, north, south, west, east. Zero is
/// every face at its default, so a block that was never configured needs no NBT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sides(pub u32);

impl Sides {
    /// The modes in relative-side order for a block facing `facing`, two bits each, for the window.
    pub fn relative(self, facing: usize) -> u32 {
        (0..6).fold(0, |acc, side| {
            acc | ((self.get(relative_face(facing, side)) as u32) << (2 * side))
        })
    }

    /// One relative side's mode from `relative`'s packing.
    pub fn relative_get(packed: u32, side: usize) -> SideMode {
        Sides(packed).get(side)
    }

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
    fn relative_sides_follow_the_front() {
        // Facing north (2): left is east, right west, back south.
        assert_eq!(
            (0..6).map(|s| relative_face(2, s)).collect::<Vec<_>>(),
            [2, 5, 4, 3, 1, 0]
        );
        // Facing east (5): left is south.
        assert_eq!(relative_face(5, 1), 3);
        // Facing up: top is south, left east.
        assert_eq!((relative_face(1, 4), relative_face(1, 1)), (3, 5));
        let mut s = Sides::default();
        s.set(3, SideMode::Output);
        assert_eq!(Sides::relative_get(s.relative(2), 3), SideMode::Output);
        assert_eq!(SideMode::Off.window_next(), SideMode::Input);
        assert_eq!(SideMode::Default.window_next(), SideMode::Off);
        assert_eq!(SideMode::Off.window_previous(), SideMode::Default);
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
