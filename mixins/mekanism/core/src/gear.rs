// Items that hold energy (mekanism.common.registries.MekanismItems' energy containers, GearConfig), the
// energy slots that charge and discharge them (EnergyInventorySlot), and the Atomic Disassembler's
// mining (ItemAtomicDisassembler).

use crate::energy::{Action, EnergyContainer};
use crate::tier::{self, BaseTier};

/// An item's energy container: how much it holds, how fast it charges, and whether machines may
/// take energy back out of it (`canExtract`: the tablet and energy cubes; tools only spend theirs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemEnergy {
    pub capacity: u64,
    pub rate: u64,
    pub can_discharge: bool,
}

/// `GearConfig` defaults: energy tablet and Atomic Disassembler 1 MFE at 5 kFE/t, electric bow
/// 120 kFE at 600 FE/t. An energy cube item holds what its block does, at its output rate.
pub fn item_energy(id: &str) -> Option<ItemEnergy> {
    let (capacity, rate, can_discharge) = match id {
        "mekanism:energy_tablet" => (1_000_000, 5_000, true),
        "mekanism:atomic_disassembler" => (1_000_000, 5_000, false),
        "mekanism:electric_bow" => (120_000, 600, false),
        crate::seismic::READER => (
            crate::seismic::READER_CAPACITY,
            crate::seismic::READER_CHARGE_RATE,
            false,
        ),
        crate::teleporter::PORTABLE => (
            crate::teleporter::PORTABLE_CAPACITY,
            crate::teleporter::PORTABLE_CHARGE_RATE,
            false,
        ),
        crate::modules::MEKA_TOOL
        | crate::modules::HELMET
        | crate::modules::BODYARMOR
        | crate::modules::PANTS
        | crate::modules::BOOTS => (
            crate::modules::BASE_CAPACITY,
            crate::modules::BASE_CHARGE_RATE,
            false,
        ),
        "mekanism:free_runners" | "mekanism:free_runners_armored" => {
            (FREE_RUNNER_CAPACITY, FREE_RUNNER_CHARGE_RATE, false)
        }
        _ => {
            let name = id.strip_prefix("mekanism:")?.strip_suffix("_energy_cube")?;
            let tier = BaseTier::ALL.into_iter().find(|t| t.name() == name)?;
            let (capacity, rate) = tier::energy_cube(tier);
            (capacity, rate, true)
        }
    };
    let (capacity, rate) = crate::config::energy_override(id).unwrap_or((capacity, rate));
    Some(ItemEnergy {
        capacity,
        rate,
        can_discharge,
    })
}

/// The ids of every item that holds energy (`item_energy` knows them): what an energy slot takes.
pub fn energy_item_ids() -> Vec<String> {
    let mut ids: Vec<String> = [
        "mekanism:energy_tablet",
        "mekanism:atomic_disassembler",
        "mekanism:electric_bow",
        crate::seismic::READER,
        crate::teleporter::PORTABLE,
        crate::modules::MEKA_TOOL,
        crate::modules::HELMET,
        crate::modules::BODYARMOR,
        crate::modules::PANTS,
        crate::modules::BOOTS,
        "mekanism:free_runners",
        "mekanism:free_runners_armored",
    ]
    .map(str::to_owned)
    .into();
    ids.extend(
        BaseTier::ALL
            .into_iter()
            .map(|t| format!("mekanism:{}_energy_cube", t.name())),
    );
    ids
}

/// A charge slot (an energy cube's left slot) fills the item from `source`, at most the item's rate
/// a tick. Returns the item's new energy.
pub fn charge(item: ItemEnergy, stored: u64, source: &mut EnergyContainer) -> u64 {
    let room = item.capacity.saturating_sub(stored).min(item.rate);
    let moved = source.extract(room, Action::Execute);
    stored + moved
}

/// A discharge slot (a machine's energy slot, a cube's right slot) moves the item's energy into
/// `target`, at most the item's rate a tick. Returns the item's new energy.
pub fn discharge(item: ItemEnergy, stored: u64, target: &mut EnergyContainer) -> u64 {
    if !item.can_discharge {
        return stored;
    }
    let offered = stored.min(item.rate);
    let left = target.insert(offered, Action::Execute);
    stored - (offered - left)
}

/// `ItemAtomicDisassembler.DisassemblerMode`: normal, slow, fast, vein and off; slow, fast and vein can be
/// switched off by `GearConfig` (vein is off by default).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisassemblerMode {
    #[default]
    Normal,
    Slow,
    Fast,
    Vein,
    Off,
}

impl DisassemblerMode {
    pub const ALL: [DisassemblerMode; 5] = [Self::Normal, Self::Slow, Self::Fast, Self::Vein, Self::Off];

    pub fn efficiency(self) -> u64 {
        match self {
            Self::Normal => 20,
            Self::Slow => 8,
            Self::Fast => 128,
            Self::Vein => 20,
            Self::Off => 0,
        }
    }

    /// `isEnabled`.
    pub fn enabled(self, config: &crate::config::GearConfig) -> bool {
        match self {
            Self::Slow => config.disassembler_slow_mode,
            Self::Fast => config.disassembler_fast_mode,
            Self::Vein => config.disassembler_vein_mining,
            Self::Normal | Self::Off => true,
        }
    }

    /// The serialized name (`StringRepresentable`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Slow => "slow",
            Self::Fast => "fast",
            Self::Vein => "vein",
            Self::Off => "off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Slow => "Slow",
            Self::Fast => "Fast",
            Self::Vein => "Vein",
            Self::Off => "Off",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    /// `adjust(1)` with every mode enabled.
    pub fn next(self) -> Self {
        self.next_enabled(&crate::config::GearConfig { disassembler_vein_mining: true, ..Default::default() })
    }

    /// `IDisableableEnum.adjust(1)`: the next enabled mode, wrapping around.
    pub fn next_enabled(self, config: &crate::config::GearConfig) -> Self {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        (1..=Self::ALL.len()).map(|k| Self::ALL[(i + k) % Self::ALL.len()]).find(|m| m.enabled(config)).unwrap_or(self)
    }
}

/// `GearConfig.disassemblerEnergyUsage`, FE per point of efficiency a block costs.
pub const DISASSEMBLER_ENERGY_USAGE: u64 = 10;

/// The energy breaking a block costs (`getDestroyEnergy`): half for a block that breaks instantly.
pub fn destroy_energy(mode: DisassemblerMode, hardness: f32) -> u64 {
    destroy_energy_with(DISASSEMBLER_ENERGY_USAGE, mode, hardness)
}

/// `destroy_energy` with the config's `energyUsage`.
pub fn destroy_energy_with(usage: u64, mode: DisassemblerMode, hardness: f32) -> u64 {
    let base = usage * mode.efficiency();
    if hardness == 0.0 {
        (base / 2).max(1)
    } else {
        base
    }
}

/// `ItemAtomicDisassembler.getDestroySpeed`: the mode's efficiency while the energy for the block is
/// there; short of it, normal mode's efficiency scaled down by how much of it is.
pub fn destroy_speed(mode: DisassemblerMode, stored: u64, hardness: f32) -> f32 {
    destroy_speed_with(DISASSEMBLER_ENERGY_USAGE, mode, stored, hardness)
}

/// `destroy_speed` with the config's `energyUsage`.
pub fn destroy_speed_with(usage: u64, mode: DisassemblerMode, stored: u64, hardness: f32) -> f32 {
    let required = destroy_energy_with(usage, mode, hardness);
    if stored < required {
        let normal = DisassemblerMode::Normal.efficiency() as f32;
        return normal * (stored as f32 / required as f32);
    }
    mode.efficiency() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_hold_what_gear_config_says() {
        let t = item_energy("mekanism:energy_tablet").unwrap();
        assert_eq!(
            (t.capacity, t.rate, t.can_discharge),
            (1_000_000, 5_000, true)
        );
        assert!(
            !item_energy("mekanism:atomic_disassembler")
                .unwrap()
                .can_discharge
        );
        let cube = item_energy("mekanism:elite_energy_cube").unwrap();
        assert_eq!((cube.capacity, cube.rate), (64_000_000, 64_000));
        assert_eq!(item_energy("minecraft:stick"), None);
    }

    #[test]
    fn a_cube_charges_a_tablet_at_the_tablets_rate() {
        let tablet = item_energy("mekanism:energy_tablet").unwrap();
        let mut cube = EnergyContainer::new(4_000_000, 4_000);
        cube.stored = 100_000;
        let after = charge(tablet, 998_000, &mut cube);
        assert_eq!(after, 1_000_000, "only the room left");
        assert_eq!(cube.stored, 98_000);
        let mut cube = EnergyContainer::new(4_000_000, 4_000);
        cube.stored = 100_000;
        let after = charge(tablet, 0, &mut cube);
        assert_eq!(after, 4_000, "the cube's own output rate limits it");
    }

    #[test]
    fn a_machine_runs_off_a_tablet_but_not_off_a_disassembler() {
        let mut machine = EnergyContainer::new(20_000, u64::MAX);
        let tablet = item_energy("mekanism:energy_tablet").unwrap();
        assert_eq!(discharge(tablet, 12_000, &mut machine), 7_000);
        assert_eq!(machine.stored, 5_000);
        let drill = item_energy("mekanism:atomic_disassembler").unwrap();
        assert_eq!(discharge(drill, 12_000, &mut machine), 12_000);
    }

    #[test]
    fn the_disassembler_slows_down_as_it_runs_out() {
        use DisassemblerMode::*;
        assert_eq!(destroy_energy(Normal, 1.5), 200);
        assert_eq!(destroy_energy(Normal, 0.0), 100);
        assert_eq!(destroy_speed(Normal, 1_000, 50.0), 20.0);
        assert_eq!(destroy_speed(Normal, 50, 3.0), 5.0);
        assert_eq!(destroy_speed(Normal, 0, 3.0), 0.0);
    }

    #[test]
    fn modes_cycle_and_cost_by_efficiency() {
        use DisassemblerMode::*;
        assert_eq!(
            [Normal.next(), Slow.next(), Fast.next(), Off.next()],
            [Slow, Fast, Vein, Normal]
        );
        assert_eq!(destroy_energy(Fast, 2.0), 1_280);
        assert_eq!(destroy_speed(Fast, 2_000, 2.0), 128.0);
        // Short of fast mode's cost, it mines as normal mode would with what it has.
        assert_eq!(destroy_speed(Fast, 640, 2.0), 10.0);
        // Off costs nothing and mines at 0; an instant block still costs 1 FE.
        assert_eq!(
            (destroy_energy(Off, 2.0), destroy_speed(Off, 0, 2.0)),
            (0, 0.0)
        );
        assert_eq!(destroy_energy(Off, 0.0), 1);
        assert_eq!(DisassemblerMode::from_name("slow"), Some(Slow));
    }
}

/// The jetpack (`ItemJetpack`, GearConfig's jetpack capacity): hydrogen it holds.
pub const JETPACK_CAPACITY: u64 = 24_000;
/// `ItemJetpack.useJetpackFuel`: thrust per mB of hydrogen, one mB a tick.
pub const JETPACK_THRUST: f64 = 0.15;

/// `IJetpackItem.handleJetpackMotion` in its normal mode: the velocity a tick of thrust adds, from the
/// player's velocity.
pub fn jetpack_normal(motion: [f64; 3], thrust: f64) -> [f64; 3] {
    [
        0.08 * motion[0],
        thrust * (-motion[1]).exp().min(1.0),
        0.08 * motion[2],
    ]
}

#[cfg(test)]
mod jetpack_tests {
    use super::*;

    #[test]
    fn thrust_fades_as_the_player_rises() {
        let still = jetpack_normal([0.0, 0.0, 0.0], JETPACK_THRUST);
        assert_eq!(still, [0.0, 0.15, 0.0]);
        let rising = jetpack_normal([1.0, 1.0, 0.0], JETPACK_THRUST);
        assert!((rising[1] - 0.15 * (-1f64).exp()).abs() < 1e-12);
        assert!((rising[0] - 0.08).abs() < 1e-12);
        // Falling, the thrust is full.
        assert_eq!(jetpack_normal([0.0, -2.0, 0.0], JETPACK_THRUST)[1], 0.15);
    }
}

/// The free runners (`ItemFreeRunners`, `GearConfig`'s free runners): `maxEnergy`, `chargeRate`,
/// `fallEnergyCost` (FE per point of fall damage absorbed), and the step height their normal mode adds
/// (`CommonPlayerTickHandler.getStepBoost`).
pub const FREE_RUNNER_CAPACITY: u64 = 64_000;
pub const FREE_RUNNER_CHARGE_RATE: u64 = 320;
pub const FREE_RUNNER_FALL_COST: u64 = 50;
pub const FREE_RUNNER_STEP: f64 = 0.5;

/// `IFreeRunnerItem.FreeRunnerMode`: normal (no fall damage, step assist), safety (no fall damage),
/// disabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FreeRunnerMode {
    #[default]
    Normal,
    Safety,
    Disabled,
}

impl FreeRunnerMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Safety => "safety",
            Self::Disabled => "disabled",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Regular",
            Self::Safety => "Safety",
            Self::Disabled => "Disabled",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Normal, Self::Safety, Self::Disabled]
            .into_iter()
            .find(|m| m.name() == name)
    }

    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::Safety,
            Self::Safety => Self::Disabled,
            Self::Disabled => Self::Normal,
        }
    }

    pub fn prevents_fall_damage(self) -> bool {
        self != Self::Disabled
    }

    pub fn step_boost(self) -> bool {
        self == Self::Normal
    }
}

/// `CommonPlayerTickHandler.livingFall` for the free runners, taken a tick at a time: the fall so far
/// (blocks past the safe distance count as damage points) costs `fallEnergyCost` FE a point. Returns
/// the energy it takes, or None when the runners cannot pay for all of it.
pub fn fall_cost(fallen: f64, stored: u64) -> Option<u64> {
    let cost = (fallen.max(0.0) * crate::config::gear_int("free_runner.fallEnergyCost", FREE_RUNNER_FALL_COST) as f64).ceil() as u64;
    (cost <= stored).then_some(cost)
}

/// `CommonPlayerTickHandler.livingFall` with its ratio: `fallen` points of damage, `cost` FE a point and
/// `ratio` of the damage absorbable. Returns the energy taken (at most `stored`) and the fall left over.
pub fn fall_absorb(fallen: f64, stored: u64, cost: u64, ratio: f64) -> (u64, f64) {
    if fallen <= 0.0 || ratio <= 0.0 {
        return (0, fallen.max(0.0));
    }
    let required = (cost as f64 * fallen * ratio).ceil() as u64;
    let (spent, absorbed) = if required == 0 {
        (0, ratio)
    } else {
        let taken = required.min(stored);
        (taken, ratio * taken as f64 / required as f64)
    };
    (spent, fallen * (1.0 - absorbed).max(0.0))
}

#[cfg(test)]
mod fall_absorb_tests {
    use super::*;

    #[test]
    fn fall_absorb_is_partial_when_short_of_energy() {
        assert_eq!(fall_absorb(4.0, 1_000, 50, 1.0), (200, 0.0));
        let (spent, left) = fall_absorb(4.0, 100, 50, 1.0);
        assert_eq!((spent, left), (100, 2.0));
        assert_eq!(fall_absorb(4.0, 1_000, 50, 0.5), (100, 2.0));
    }
}

#[cfg(test)]
mod free_runner_tests {
    use super::*;

    #[test]
    fn modes_cycle_and_guard_falls() {
        let m = FreeRunnerMode::default();
        assert_eq!(m.next(), FreeRunnerMode::Safety);
        assert_eq!(m.next().next().next(), m);
        assert!(m.step_boost() && !m.next().step_boost());
        assert!(!FreeRunnerMode::Disabled.prevents_fall_damage());
        assert_eq!(
            FreeRunnerMode::from_name("safety"),
            Some(FreeRunnerMode::Safety)
        );
    }

    #[test]
    fn falls_cost_fifty_fe_a_block() {
        assert_eq!(fall_cost(2.5, 1_000), Some(125));
        assert_eq!(fall_cost(0.0, 0), Some(0));
        assert_eq!(fall_cost(10.0, 499), None);
        assert_eq!(
            item_energy("mekanism:free_runners").map(|e| e.capacity),
            Some(FREE_RUNNER_CAPACITY)
        );
    }
}

#[cfg(test)]
mod energy_item_tests {
    use super::*;

    #[test]
    fn every_listed_item_holds_energy() {
        for id in energy_item_ids() {
            assert!(item_energy(&id).is_some(), "{id}");
        }
    }
}
