// The Fusion Reactor (mekanism.generators.common.content.fusion.FusionReactorMultiblockData): a 5×5×5
// frame with its corners cut off, a controller in the middle of its top and ports in its walls. Deuterium
// and tritium are injected as D-T fuel into hot plasma; the fuel burns while the plasma is over 100 MK,
// the plasma heats the casing, and the casing's heat leaves as energy (the thermocouples) and, with water
// in the reactor, as steam.

pub const SIZE: i32 = 5;
/// `FusionReactorValidator.ALLOWED_GRID`: on each face, 0 = anything, 1 = a frame block, 2 = any part
/// of the reactor (frame, port, controller, logic adapter, laser focus matrix) or reactor glass.
pub const ALLOWED_GRID: [[u8; 5]; 5] = [
    [0, 0, 1, 0, 0],
    [0, 1, 2, 1, 0],
    [1, 2, 2, 2, 1],
    [0, 1, 2, 1, 0],
    [0, 0, 1, 0, 0],
];

pub const FRAME: &str = "mekanismgenerators:fusion_reactor_frame";
pub const PORT: &str = "mekanismgenerators:fusion_reactor_port";
pub const CONTROLLER: &str = "mekanismgenerators:fusion_reactor_controller";
pub const DEUTERIUM: &str = "mekanismgenerators:deuterium";
pub const TRITIUM: &str = "mekanismgenerators:tritium";

pub const DEFAULT_INJECTION: u64 = 2;
pub const MAX_INJECTION: u64 = 98;
pub const BURN_TEMPERATURE: f64 = 100_000_000.0;
const PLASMA_HEAT_CAPACITY: f64 = 100.0;
const PLASMA_CASE_CONDUCTIVITY: f64 = 0.2;
/// `HeatAPI.AMBIENT_TEMP`, as biomes do not change it here.
pub const AMBIENT: f64 = 300.0;
/// `GeneratorsConfig`: `fuelEnergy`, `thermocoupleEfficiency`, `casingThermalConductivity`,
/// `waterHeatingRatio`, `fuelCapacity`, `energyCapacity`, `waterPerInjection`, `steamPerInjection`.
pub const ENERGY_PER_FUEL: f64 = 10_000_000.0;
const THERMOCOUPLE_EFFICIENCY: f64 = 0.05;
const CASING_CONDUCTIVITY: f64 = 0.1;
const WATER_HEATING_RATIO: f64 = 0.3;
pub const FUEL_CAPACITY: u64 = 1_000;
pub const ENERGY_CAPACITY: u64 = 1_000_000_000;
pub const WATER_PER_INJECTION: u64 = 1_000_000;
pub const STEAM_PER_INJECTION: u64 = 100_000_000;
/// `HeatUtils.getSteamEnergyEfficiency` and `getWaterThermalEnthalpy` (`maxEnergyPerSteam`).
const STEAM_EFFICIENCY: f64 = 0.2;
const WATER_ENTHALPY: f64 = 10.0;

/// The block a face of the reactor needs at `rel` (0..5 on each axis from the reactor's low corner):
/// Some(grid value) on the shell, None inside.
pub fn requirement(rel: [i32; 3]) -> Option<u8> {
    let edge = |c: i32| c == 0 || c == SIZE - 1;
    let k = (0..3).find(|&k| edge(rel[k]))?;
    let (u, v) = match k {
        0 => (rel[1], rel[2]),
        1 => (rel[0], rel[2]),
        _ => (rel[0], rel[1]),
    };
    Some(ALLOWED_GRID[u as usize][v as usize])
}

/// The reactor blocks that may stand where the grid says 2.
pub fn is_part(name: &str) -> bool {
    matches!(
        name,
        FRAME
            | PORT
            | CONTROLLER
            | "mekanismgenerators:fusion_reactor_logic_adapter"
            | "mekanismgenerators:laser_focus_matrix"
            | "mekanismgenerators:reactor_glass"
    )
}

pub fn water_capacity(injection: u64) -> u64 {
    injection * WATER_PER_INJECTION
}

pub fn steam_capacity(injection: u64) -> u64 {
    injection * STEAM_PER_INJECTION
}

/// The next injection rate a click sets: up in steps of 2, wrapping back to 2 past 98.
pub fn next_injection(now: u64) -> u64 {
    if now + 2 > MAX_INJECTION {
        DEFAULT_INJECTION
    } else {
        now + 2
    }
}

/// The plasma and the casing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plasma {
    pub plasma: f64,
    pub case: f64,
    pub burning: bool,
}

impl Default for Plasma {
    fn default() -> Self {
        Self {
            plasma: AMBIENT,
            case: AMBIENT,
            burning: false,
        }
    }
}

/// The reactor's chemical tanks and water, in mB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tanks {
    pub deuterium: u64,
    pub tritium: u64,
    pub fuel: u64,
    pub water: u64,
    pub steam: u64,
}

/// What one tick gave: energy (J), and fuel burned (mB).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub energy: u64,
    pub burned: u64,
}

/// What the reactor's lasers and hohlraum do to start it, simplified: the plasma is heated to
/// `getMaxPlasmaTemperature` (air cooled) for the injection rate, hot enough to keep burning while the
/// casing warms up.
pub fn ignite(p: &mut Plasma, injection: u64) {
    let k = CASING_CONDUCTIVITY;
    let max = injection.max(DEFAULT_INJECTION) as f64 * ENERGY_PER_FUEL / PLASMA_CASE_CONDUCTIVITY
        * (PLASMA_CASE_CONDUCTIVITY + k)
        / k;
    p.plasma = p.plasma.max(max);
    p.burning = true;
}

/// `FusionReactorMultiblockData.tick`: inject and burn while burning and hot enough, then
/// `transferHeat`. The casing's heat changes apply at the end of the tick, as Mekanism's heat
/// capacitors do.
pub fn tick(p: &mut Plasma, t: &mut Tanks, injection: u64, steam_capacity: u64) -> Output {
    let mut burned = 0;
    if p.plasma >= BURN_TEMPERATURE {
        if p.burning {
            // injectFuel: equal deuterium and tritium, an even amount, up to the injection rate.
            let mut inject = (FUEL_CAPACITY - t.fuel)
                .min(2 * t.deuterium.min(t.tritium))
                .min(injection);
            inject -= inject % 2;
            t.deuterium -= inject / 2;
            t.tritium -= inject / 2;
            t.fuel += inject;
            // burnFuel
            let can = ((p.plasma - BURN_TEMPERATURE).floor().max(0.0) as u64).min(t.fuel);
            t.fuel -= can;
            p.plasma += ENERGY_PER_FUEL * can as f64 / PLASMA_HEAT_CAPACITY;
            burned = can;
        }
        if burned == 0 {
            p.burning = false;
        }
    } else {
        p.burning = false;
    }
    // Plasma to casing.
    let mut case_heat = 0.0;
    let plasma_case = PLASMA_CASE_CONDUCTIVITY * (p.plasma - p.case);
    p.plasma -= plasma_case / PLASMA_HEAT_CAPACITY;
    case_heat += plasma_case;
    // Casing to water.
    let case_water = WATER_HEATING_RATIO * (p.case - AMBIENT);
    if t.water > 0 && case_water > 0.0 {
        let boil = ((STEAM_EFFICIENCY * case_water / WATER_ENTHALPY) as u64)
            .min(steam_capacity.saturating_sub(t.steam))
            .min(t.water);
        t.water -= boil;
        t.steam += boil;
        case_heat -= boil as f64 * WATER_ENTHALPY / STEAM_EFFICIENCY;
    }
    // Casing to air, through the thermocouples.
    let case_air = CASING_CONDUCTIVITY * (p.case - AMBIENT);
    case_heat -= case_air;
    let energy = (case_air * THERMOCOUPLE_EFFICIENCY).max(0.0) as u64;
    p.case += case_heat;
    Output { energy, burned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_cuts_the_corners_and_wants_the_controller_on_top() {
        assert_eq!(requirement([0, 0, 0]), Some(0));
        assert_eq!(requirement([2, 4, 2]), Some(2));
        assert_eq!(requirement([2, 0, 0]), Some(1));
        assert_eq!(requirement([1, 1, 4]), Some(1));
        assert_eq!(requirement([2, 2, 2]), None);
        assert!(is_part(CONTROLLER) && !is_part("minecraft:stone"));
        assert_eq!((next_injection(2), next_injection(98)), (4, 2));
    }

    #[test]
    fn a_cold_reactor_does_not_burn() {
        let mut p = Plasma::default();
        let mut t = Tanks {
            deuterium: 100,
            tritium: 100,
            ..Default::default()
        };
        let out = tick(&mut p, &mut t, 2, steam_capacity(2));
        assert_eq!(out, Output::default());
        assert_eq!((t.deuterium, p.burning), (100, false));
    }

    #[test]
    fn an_ignited_reactor_burns_its_injection_rate_and_warms_to_mekanisms_figures() {
        let mut p = Plasma::default();
        let mut t = Tanks {
            deuterium: u64::MAX / 4,
            tritium: u64::MAX / 4,
            ..Default::default()
        };
        ignite(&mut p, 2);
        let mut out = Output::default();
        for _ in 0..20_000 {
            out = tick(&mut p, &mut t, 2, steam_capacity(2));
        }
        assert!(p.burning);
        assert_eq!(out.burned, 2);
        // getMaxCasingTemperature(air cooled) = 2 × 10 MJ / 0.1 = 200 MK, and getPassiveGeneration
        // = 0.05 × 0.1 × 200 MK = 1 MJ a tick.
        assert!((p.case - 200_000_300.0).abs() < 1_000.0, "{}", p.case);
        assert!(out.energy.abs_diff(1_000_000) < 100, "{}", out.energy);
    }

    #[test]
    fn water_takes_heat_as_steam() {
        let mut p = Plasma {
            plasma: 3e8,
            case: 2e8,
            burning: true,
        };
        let mut t = Tanks {
            deuterium: 1_000,
            tritium: 1_000,
            water: 2_000_000,
            ..Default::default()
        };
        tick(&mut p, &mut t, 2, steam_capacity(2));
        // 0.2 × 0.3 × 200 MK / 10 = 1.2 million mB a tick.
        assert!(t.steam.abs_diff(1_200_000) < 10, "{}", t.steam);
        assert_eq!(t.water + t.steam, 2_000_000);
    }
}
