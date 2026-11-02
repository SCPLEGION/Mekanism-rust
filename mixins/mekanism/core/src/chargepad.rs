// The Chargepad (`TileEntityChargepad`): a player standing on it gets one item charged a tick, up to
// `usage.chargepad` FE a tick each, from the pad's energy.

pub const CHARGEPAD: &str = "mekanism:chargepad";
/// `usage.chargepad`: FE a tick for each player on the pad.
pub const CHARGE_RATE: u64 = 1_024;
/// The game looks at each player every 10 ticks, so the pad gives ten ticks' worth at a time and
/// holds that much.
pub const CHECK_TICKS: u64 = 10;
pub const STORAGE: u64 = CHARGE_RATE * CHECK_TICKS;
/// The pad's box reaches 0.4 blocks up from its thin plate (`new AABB(..., y + 0.4, ...)`). Here the
/// pad is a full block, so the box starts at its top.
pub const REACH: f64 = 0.4;

/// What one check gives an item holding `stored` of `capacity`, taking `rate` a tick, from a pad
/// holding `pad`.
pub fn charge(pad: u64, capacity: u64, stored: u64, rate: u64) -> u64 {
    pad.min(STORAGE)
        .min(capacity.saturating_sub(stored))
        .min(rate.saturating_mul(CHECK_TICKS))
}

/// The block a pad would be at under feet at height `y`.
pub fn pad_below(y: f64) -> i32 {
    (y - 1.0).floor() as i32
}

/// Whether feet at height `y` stand on the pad whose block is at `pad_y`.
pub fn on_pad(y: f64, pad_y: i32) -> bool {
    let top = f64::from(pad_y) + 1.0;
    (top..top + REACH).contains(&y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_is_limited_by_each_side() {
        assert_eq!(charge(100_000, 1_000_000, 0, 1_000_000), STORAGE);
        assert_eq!(charge(500, 1_000_000, 0, 1_000_000), 500);
        assert_eq!(charge(STORAGE, 1_000, 900, 1_000_000), 100);
        assert_eq!(charge(STORAGE, 1_000_000, 0, 50), 500);
    }

    #[test]
    fn standing_height() {
        assert_eq!(pad_below(65.0), 64);
        assert!(on_pad(65.0, 64));
        assert!(!on_pad(65.5, 64));
        assert!(!on_pad(64.9, 64));
    }
}
