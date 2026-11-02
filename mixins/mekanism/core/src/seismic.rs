// The Seismic Vibrator (TileEntitySeismicVibrator) and Seismic Reader (ItemSeismicReader): while a
// vibrator runs, a reader used in its chunk shows the column of blocks under the player.

pub const VIBRATOR: &str = "mekanism:seismic_vibrator";
pub const READER: &str = "mekanism:seismic_reader";
/// `UsageConfig.seismicVibrator`, per tick; it stores four ticks of it.
pub const VIBRATOR_USAGE: u64 = 5;
pub const VIBRATOR_STORAGE: u64 = 4 * VIBRATOR_USAGE;
/// `GearConfig.seismicReader*`: capacity, charge rate and the energy one reading costs.
pub const READER_CAPACITY: u64 = 12_000;
pub const READER_CHARGE_RATE: u64 = 60;
pub const READER_USAGE: u64 = 250;

/// Whether the block at `vibrator` shakes the chunk the player at `player` (block x and z) stands in.
pub fn same_chunk(vibrator: [i32; 3], player: [i32; 2]) -> bool {
    vibrator[0] >> 4 == player[0] >> 4 && vibrator[2] >> 4 == player[1] >> 4
}

/// A column from top to bottom as runs of one block: (block, top y, bottom y).
pub fn runs(column: &[(i32, String)]) -> Vec<(String, i32, i32)> {
    let mut out: Vec<(String, i32, i32)> = Vec::new();
    for (y, name) in column {
        match out.last_mut() {
            Some(last) if last.0 == *name && last.2 == y + 1 => last.2 = *y,
            _ => out.push((name.clone(), *y, *y)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_and_runs() {
        assert!(same_chunk([16, 70, -1], [31, -16]));
        assert!(!same_chunk([16, 70, -1], [15, -16]));
        let col: Vec<(i32, String)> = [(5, "a"), (4, "a"), (3, "b"), (2, "a"), (1, "a")]
            .into_iter()
            .map(|(y, n)| (y, n.to_owned()))
            .collect();
        assert_eq!(
            runs(&col),
            vec![("a".into(), 5, 4), ("b".into(), 3, 3), ("a".into(), 2, 1)]
        );
    }
}
