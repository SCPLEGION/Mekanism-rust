//! Redstone control (`IRedstoneControl.RedstoneControl`): whether a machine runs, from the redstone
//! signal it receives. The screen's redstone tab steps through the modes, a left click forward and a
//! right click back (`GuiInteraction.NEXT_REDSTONE_CONTROL` / `PREVIOUS_REDSTONE_CONTROL`).

/// A machine's redstone mode, in Mekanism's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedstoneControl {
    /// "IGNORED": runs whatever the signal.
    Disabled,
    /// "NORMAL": runs while powered.
    High,
    /// "INVERTED": runs while unpowered.
    Low,
    /// "PULSE": runs for the tick the signal rises.
    Pulse,
}

const ALL: [RedstoneControl; 4] = [
    RedstoneControl::Disabled,
    RedstoneControl::High,
    RedstoneControl::Low,
    RedstoneControl::Pulse,
];

impl RedstoneControl {
    /// The mode saved as its ordinal (`controlType`); anything unknown is `Disabled`.
    pub fn from_ordinal(i: u32) -> Self {
        ALL.get(i as usize)
            .copied()
            .unwrap_or(RedstoneControl::Disabled)
    }

    pub fn ordinal(self) -> u32 {
        self as u32
    }

    /// `RedstoneControl.getNext` (a left click on the tab).
    pub fn next(self) -> Self {
        ALL[(self as usize + 1) % ALL.len()]
    }

    /// `RedstoneControl.getPrevious` (a right click).
    pub fn previous(self) -> Self {
        ALL[(self as usize + ALL.len() - 1) % ALL.len()]
    }

    /// `TileEntityMekanism.isRedstoneActivated`: whether the machine may run this tick, from the
    /// signal now and last tick.
    pub fn allows(self, powered: bool, powered_last_tick: bool) -> bool {
        match self {
            RedstoneControl::Disabled => true,
            RedstoneControl::High => powered,
            RedstoneControl::Low => !powered,
            RedstoneControl::Pulse => powered && !powered_last_tick,
        }
    }

    /// The tab's tooltip (`redstone_control.mekanism.*`).
    pub fn name(self) -> &'static str {
        match self {
            RedstoneControl::Disabled => "Redstone Detection: IGNORED",
            RedstoneControl::High => "Redstone Detection: NORMAL",
            RedstoneControl::Low => "Redstone Detection: INVERTED",
            RedstoneControl::Pulse => "Redstone Detection: PULSE",
        }
    }

    /// The tab's overlay sprite (`redstone_control/*`).
    pub fn sprite(self) -> &'static str {
        match self {
            RedstoneControl::Disabled => "redstone_control/disabled",
            RedstoneControl::High => "redstone_control/high",
            RedstoneControl::Low => "redstone_control/low",
            RedstoneControl::Pulse => "redstone_control/pulse",
        }
    }

    /// Whether the tab warns that the machine waits for a signal (`GuiMekanismTile.addGenericTabs`).
    pub fn warns(self, powered: bool, powered_last_tick: bool) -> bool {
        self != RedstoneControl::Disabled && !self.allows(powered, powered_last_tick)
    }
}

#[cfg(test)]
mod tests {
    use super::RedstoneControl::*;
    use super::*;

    #[test]
    fn cycles_both_ways() {
        assert_eq!(Disabled.next(), High);
        assert_eq!(Pulse.next(), Disabled);
        assert_eq!(Disabled.previous(), Pulse);
        assert_eq!(Low.previous(), High);
        assert_eq!(RedstoneControl::from_ordinal(7), Disabled);
        assert_eq!(RedstoneControl::from_ordinal(Low.ordinal()), Low);
    }

    #[test]
    fn modes_gate_on_the_signal() {
        assert!(Disabled.allows(false, false));
        assert!(High.allows(true, true) && !High.allows(false, true));
        assert!(Low.allows(false, false) && !Low.allows(true, false));
        assert!(
            Pulse.allows(true, false) && !Pulse.allows(true, true) && !Pulse.allows(false, false)
        );
        assert!(High.warns(false, false) && !Disabled.warns(false, false));
    }
}
