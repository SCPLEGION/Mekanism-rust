// Energy containers (mekanism.common.capabilities.energy.BasicEnergyContainer).
//
// Mekanism 10.8 counts energy in FE. A container holds up to `capacity`; automation may move at most
// `max_rate` in or out per tick, as the Java container's RateLimitTracker allows. Rates reset with
// `start_tick`.

/// Whether an operation changes the container or only reports what it would do (`Action`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Execute,
    Simulate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyContainer {
    pub stored: u64,
    pub capacity: u64,
    /// The most automation can insert or extract in one tick (`u64::MAX`: unlimited).
    pub max_rate: u64,
    inserted_this_tick: u64,
    extracted_this_tick: u64,
}

impl EnergyContainer {
    pub fn new(capacity: u64, max_rate: u64) -> Self {
        Self {
            stored: 0,
            capacity,
            max_rate,
            inserted_this_tick: 0,
            extracted_this_tick: 0,
        }
    }

    /// Resets the per-tick rate limits; call once at the start of each server tick of the owner.
    pub fn start_tick(&mut self) {
        self.inserted_this_tick = 0;
        self.extracted_this_tick = 0;
    }

    pub fn needed(&self) -> u64 {
        self.capacity.saturating_sub(self.stored)
    }

    /// Inserts up to `amount`; returns what was not accepted (the Java `insert` returns the remainder).
    pub fn insert(&mut self, amount: u64, action: Action) -> u64 {
        let rate_left = self.max_rate.saturating_sub(self.inserted_this_tick);
        let accepted = amount.min(self.needed()).min(rate_left);
        if action == Action::Execute {
            self.stored += accepted;
            self.inserted_this_tick += accepted;
        }
        amount - accepted
    }

    /// Extracts up to `amount`; returns what was extracted.
    pub fn extract(&mut self, amount: u64, action: Action) -> u64 {
        let rate_left = self.max_rate.saturating_sub(self.extracted_this_tick);
        let taken = amount.min(self.stored).min(rate_left);
        if action == Action::Execute {
            self.stored -= taken;
            self.extracted_this_tick += taken;
        }
        taken
    }

    /// The machine's own use, not automation: no rate limit (a machine drawing its per-tick cost).
    pub fn consume(&mut self, amount: u64) -> bool {
        if self.stored < amount {
            return false;
        }
        self.stored -= amount;
        true
    }
}

/// Moves as much energy as both sides allow from `from` to `to`; returns the amount moved.
pub fn transfer(from: &mut EnergyContainer, to: &mut EnergyContainer, limit: u64) -> u64 {
    let offered = from.extract(limit, Action::Simulate);
    let refused = to.insert(offered, Action::Simulate);
    let amount = offered - refused;
    if amount > 0 {
        from.extract(amount, Action::Execute);
        to.insert(amount, Action::Execute);
    }
    amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_returns_the_remainder_and_respects_capacity() {
        let mut c = EnergyContainer::new(100, u64::MAX);
        assert_eq!(c.insert(60, Action::Execute), 0);
        assert_eq!(c.insert(60, Action::Execute), 20);
        assert_eq!(c.stored, 100);
    }

    #[test]
    fn simulate_changes_nothing() {
        let mut c = EnergyContainer::new(100, u64::MAX);
        assert_eq!(c.insert(30, Action::Simulate), 0);
        assert_eq!(c.stored, 0);
    }

    #[test]
    fn the_rate_limit_applies_per_tick() {
        let mut c = EnergyContainer::new(1_000, 10);
        assert_eq!(c.insert(25, Action::Execute), 15);
        assert_eq!(c.insert(5, Action::Execute), 5);
        c.start_tick();
        assert_eq!(c.insert(5, Action::Execute), 0);
        assert_eq!(c.stored, 15);
    }

    #[test]
    fn transfer_moves_what_both_sides_allow() {
        let mut a = EnergyContainer::new(100, 40);
        a.stored = 100;
        let mut b = EnergyContainer::new(30, u64::MAX);
        assert_eq!(transfer(&mut a, &mut b, 1_000), 30);
        assert_eq!((a.stored, b.stored), (70, 30));
    }
}

/// An amount of FE with an SI prefix, as Mekanism shows energy (`UnitDisplayUtils`): "500 FE",
/// "12.5 kFE", "4 MFE", up to two decimals.
pub fn format_fe(fe: u64) -> String {
    format_unit(fe as f64, "FE")
}

/// A value with an SI prefix before `unit` (`UnitDisplayUtils.getDisplayShort`): "300 K", "1.5 kK".
pub fn format_unit(value: f64, unit: &str) -> String {
    const PREFIXES: [&str; 7] = ["", "k", "M", "G", "T", "P", "E"];
    let mut value = value;
    let mut i = 0;
    while value >= 1000.0 && i + 1 < PREFIXES.len() {
        value /= 1000.0;
        i += 1;
    }
    let mut text = format!("{value:.2}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    format!("{text} {}{unit}", PREFIXES[i])
}

#[cfg(test)]
mod format_tests {
    #[test]
    fn energy_reads_with_si_prefixes() {
        assert_eq!(super::format_fe(500), "500 FE");
        assert_eq!(super::format_fe(12_500), "12.5 kFE");
        assert_eq!(super::format_fe(4_000_000), "4 MFE");
        assert_eq!(super::format_fe(0), "0 FE");
        assert_eq!(super::format_unit(300.0, "K"), "300 K");
        assert_eq!(super::format_unit(1_534.567, "K"), "1.53 kK");
    }
}
