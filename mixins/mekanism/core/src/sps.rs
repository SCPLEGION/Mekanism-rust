// The Supercritical Phase Shifter (mekanism.common.content.sps.SPSMultiblockData): a 7×7×7 box of SPS
// casing and ports with its corners cut off, Supercharged Coils inside against the ports. Energy given
// to a port with a coil behind it turns polonium into antimatter.

pub const SIZE: i32 = 7;
/// `SPSValidator.ALLOWED_GRID`: on each face, 0 = anything, 1 = casing, 2 = casing or a port.
pub const ALLOWED_GRID: [[u8; 7]; 7] = [
    [0, 0, 1, 1, 1, 0, 0],
    [0, 1, 2, 2, 2, 1, 0],
    [1, 2, 2, 2, 2, 2, 1],
    [1, 2, 2, 2, 2, 2, 1],
    [1, 2, 2, 2, 2, 2, 1],
    [0, 1, 2, 2, 2, 1, 0],
    [0, 0, 1, 1, 1, 0, 0],
];

pub const CASING: &str = "mekanism:sps_casing";
pub const PORT: &str = "mekanism:sps_port";
pub const COIL: &str = "mekanism:supercharged_coil";
pub const POLONIUM: &str = "mekanism:polonium";
pub const ANTIMATTER: &str = "mekanism:antimatter";

/// `general.sps`: `inputPerAntimatter`, `outputTankCapacity`, `energyPerInput`; the input tank holds
/// two antimatter's worth (`getMaxInputGas`).
pub const INPUT_PER_ANTIMATTER: u64 = 1_000;

/// `inputPerAntimatter` of the open world's config (`INPUT_PER_ANTIMATTER` is its default).
pub fn input_per_antimatter() -> u64 {
    crate::config_files::active_int("general", "sps", "inputPerAntimatter", INPUT_PER_ANTIMATTER)
}
pub const OUTPUT_CAPACITY: u64 = 1_000;

/// `outputTankCapacity` of the open world's config (`OUTPUT_CAPACITY` is its default).
pub fn output_capacity() -> u64 {
    crate::config_files::active_int("general", "sps", "outputTankCapacity", OUTPUT_CAPACITY)
}
/// The input tank holds two antimatter's worth.
pub fn input_capacity() -> u64 {
    2 * input_per_antimatter()
}
pub const ENERGY_PER_INPUT: u64 = 1_000_000;

/// `energyPerInput` of the open world's config (`ENERGY_PER_INPUT` is its default).
pub fn energy_per_input() -> u64 {
    crate::config_files::active_int("general", "sps", "energyPerInput", ENERGY_PER_INPUT)
}
/// `storage.spsPort`.
pub const PORT_STORAGE: u64 = 1_000_000_000;

/// `spsPort` of the open world's config (`PORT_STORAGE` is its default).
pub fn port_storage() -> u64 {
    crate::config_files::active_int("machine-storage", "", "spsPort", PORT_STORAGE)
}

pub fn is_frame(name: &str) -> bool {
    matches!(name, CASING | PORT)
}

/// The block the shell needs at `rel` (0..7 on each axis from the low corner): Some(grid value) on the
/// shell, None inside.
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

/// The tanks and the progress the master port keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sps {
    pub input: u64,
    pub output: u64,
    pub progress: f64,
    pub input_processed: u64,
}

impl Sps {
    pub fn can_operate(&self) -> bool {
        self.input > 0 && self.output < output_capacity()
    }

    /// `process`: takes up to `operations` mB of polonium, adding an antimatter for each 1,000 mB;
    /// nothing changes when the antimatter has no room.
    fn process(&mut self, operations: u64) -> u64 {
        if operations == 0 || self.input == 0 {
            return 0;
        }
        let taken = operations.min(self.input);
        let mut total = self.input_processed + taken;
        if total >= input_per_antimatter() {
            let add = total / input_per_antimatter();
            if self.output + add > output_capacity() {
                return 0;
            }
            self.output += add;
            total %= input_per_antimatter();
        }
        self.input -= taken;
        self.input_processed = total;
        taken
    }

    /// `SPSMultiblockData.tick` with the energy the coils took this tick: the polonium processed.
    pub fn tick(&mut self, received: u64) -> f64 {
        if !self.can_operate() || received == 0 {
            return 0.0;
        }
        let needed = (input_per_antimatter() - self.input_processed)
            + input_per_antimatter() * (output_capacity() - self.output - 1);
        let processable = received as f64 / energy_per_input() as f64;
        if processable + self.progress >= needed as f64 {
            self.progress = 0.0;
            self.process(needed) as f64
        } else {
            let mut processed = processable;
            self.progress += processable;
            let to_process = self.progress.min(i32::MAX as f64) as u64;
            let actual = self.process(to_process);
            if actual < to_process {
                let dif = (to_process - actual) as f64;
                self.progress -= dif;
                processed -= dif;
            }
            self.progress %= 1.0;
            processed
        }
    }

    /// `getScaledProgress`: how far the next antimatter is.
    pub fn scaled_progress(&self) -> f64 {
        (self.input_processed as f64 + self.progress) / input_per_antimatter() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_cuts_the_corners() {
        assert_eq!(requirement([0, 0, 0]), Some(0));
        assert_eq!(requirement([0, 3, 0]), Some(1));
        assert_eq!(requirement([0, 3, 3]), Some(2));
        assert_eq!(requirement([3, 3, 3]), None);
        assert!(is_frame(PORT) && !is_frame(COIL));
    }

    #[test]
    fn a_million_fe_processes_one_mb_of_polonium() {
        let mut s = Sps {
            input: 2_000,
            ..Default::default()
        };
        assert_eq!(s.tick(500_000), 0.5);
        assert_eq!((s.input, s.input_processed), (2_000, 0));
        s.tick(500_000);
        assert_eq!((s.input, s.input_processed), (1_999, 1));
        assert!((s.progress).abs() < 1e-9);
    }

    #[test]
    fn a_thousand_mb_make_one_antimatter() {
        let mut s = Sps {
            input: 2_000,
            ..Default::default()
        };
        let processed = s.tick(1_000_000_000);
        assert_eq!(processed, 1_000.0);
        assert_eq!((s.input, s.output, s.input_processed), (1_000, 1, 0));
        assert!((s.scaled_progress()).abs() < 1e-9);
    }

    #[test]
    fn a_full_output_stops_it() {
        let mut s = Sps {
            input: 2_000,
            output: output_capacity(),
            ..Default::default()
        };
        assert_eq!(s.tick(1_000_000_000), 0.0);
        assert_eq!(s.input, 2_000);
    }
}
