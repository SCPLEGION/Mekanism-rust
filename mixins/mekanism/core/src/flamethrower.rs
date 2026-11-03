//! The Flamethrower (`ItemFlamethrower`, `EntityFlame`): its modes, the flame's launch and the quads it is drawn as.

pub const ITEM: &str = "mekanism:flamethrower";
/// `flamethrower.capacity`: 24 buckets of hydrogen.
pub const CAPACITY: u64 = 24_000;
/// `EntityFlame.LIFESPAN`: four seconds.
pub const LIFESPAN: u32 = 80;
/// `EntityFlame.DAMAGE`.
pub const DAMAGE: f32 = 10.0;
/// The flame's speed in blocks a tick (`shootFromRotation(..., 0.5F, 1)`).
pub const SPEED: f64 = 0.5;
/// `flamethrower.destroyItems`.
pub const DESTROY_ITEMS: bool = true;
/// How long a burning mob burns: five seconds, in ticks.
pub const BURN_TICKS: i32 = 100;
/// An item entity this old (five seconds) is smelted by the heat mode.
pub const SMELT_AGE: u32 = 100;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Combat,
    Heat,
    Inferno,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Combat => "combat",
            Self::Heat => "heat",
            Self::Inferno => "inferno",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Combat, Self::Heat, Self::Inferno].into_iter().find(|m| m.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Combat => "Combat",
            Self::Heat => "Heat",
            Self::Inferno => "Inferno",
        }
    }

    /// `IIncrementalEnum.adjust(1)`, wrapping.
    pub fn next(self) -> Self {
        match self {
            Self::Combat => Self::Heat,
            Self::Heat => Self::Inferno,
            Self::Inferno => Self::Combat,
        }
    }
}

/// The view vector (`Entity.calculateViewVector`) for a yaw and pitch in degrees.
pub fn look(yaw: f32, pitch: f32) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
}

/// Where a flame starts (`EntityFlame.create`): a block ahead of the eyes (a tenth lower), turned ten degrees
/// toward the hand the flamethrower is in.
pub fn start(eye: [f64; 3], yaw: f32, pitch: f32, right_handed: bool) -> [f64; 3] {
    let v = look(yaw, pitch);
    let angle = 10f64.to_radians() * if right_handed { -1.0 } else { 1.0 };
    // `Vec3.yRot`.
    let (c, s) = (angle.cos(), angle.sin());
    let turned = [v[0] * c + v[2] * s, v[1], v[2] * c - v[0] * s];
    [eye[0] + turned[0], eye[1] - 0.1 + turned[1], eye[2] + turned[2]]
}

/// The flame's velocity (`Projectile.shootFromRotation(owner, xRot, yRot, 0, 0.5F, 1)`): along the look at the
/// speed, plus the shooter's own movement and `noise`, the spread (a gaussian times 0.0075 a axis).
pub fn velocity(yaw: f32, pitch: f32, shooter: [f64; 3], noise: [f64; 3]) -> [f64; 3] {
    let v = look(yaw, pitch);
    [0, 1, 2].map(|i| v[i] * SPEED + noise[i] * 0.0075 + shooter[i])
}

/// The flame's opacity at `age` ticks (`RenderFlame`).
pub fn alpha(age: f32) -> f32 {
    1.0 - age / LIFESPAN as f32
}

/// The two crossed planes of `FlameModel`, as four corners each (tail low, head low, head high, tail high) around
/// the flame at `at` flying along `velocity`: 16 by 4 pixels, 0.9 scaled, the head a quarter block ahead.
pub fn quads(at: [f64; 3], velocity: [f64; 3]) -> [[[f64; 3]; 4]; 2] {
    let length = (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]).sqrt();
    let f = if length < 1e-9 { [0.0, 0.0, 1.0] } else { velocity.map(|v| v / length) };
    // A vertical and a horizontal direction across the flight.
    let side = {
        let s = [f[2], 0.0, -f[0]];
        let n = (s[0] * s[0] + s[2] * s[2]).sqrt();
        if n < 1e-9 { [1.0, 0.0, 0.0] } else { [s[0] / n, 0.0, s[2] / n] }
    };
    let up = [f[1] * side[2], f[2] * side[0] - f[0] * side[2], -f[1] * side[0]];
    let (tail, head, half) = (-12.0 * 0.9 / 16.0, 4.0 * 0.9 / 16.0, 2.0 * 0.9 / 16.0);
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let plane = |across: [f64; 3]| {
        let p = |x: f64, y: f64| [0, 1, 2].map(|i| at[i] + f[i] * x + across[i] * y);
        [p(tail, -half), p(head, -half), p(head, half), p(tail, half)]
    };
    let a = [0, 1, 2].map(|i| (up[i] + side[i]) * h);
    let b = [0, 1, 2].map(|i| (up[i] - side[i]) * h);
    [plane(a), plane(b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_and_round_trip() {
        assert_eq!(Mode::Combat.next().next().next(), Mode::Combat);
        for m in [Mode::Combat, Mode::Heat, Mode::Inferno] {
            assert_eq!(Mode::from_name(m.name()), Some(m));
        }
    }

    #[test]
    fn a_flame_starts_a_block_ahead_and_to_the_hand() {
        // Looking south (yaw 0): ahead is +z; a right hand is on the west (-x)... turned toward -yRot.
        let s = start([0.0, 1.62, 0.0], 0.0, 0.0, true);
        assert!((s[1] - 1.52).abs() < 1e-9);
        assert!(s[2] > 0.98 && s[2] <= 1.0);
        assert!(s[0] < 0.0 && (s[0].abs() - 10f64.to_radians().sin()).abs() < 1e-9);
        let left = start([0.0, 1.62, 0.0], 0.0, 0.0, false);
        assert!((left[0] + s[0]).abs() < 1e-9);
    }

    #[test]
    fn a_flame_flies_along_the_look_at_half_a_block_a_tick() {
        let v = velocity(90.0, 0.0, [0.0; 3], [0.0; 3]);
        assert!((v[0] + 0.5).abs() < 1e-9 && v[1].abs() < 1e-9 && v[2].abs() < 1e-6);
        let up = velocity(0.0, -90.0, [0.0; 3], [0.0; 3]);
        assert!((up[1] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_planes_lie_along_the_flight() {
        let q = quads([0.0, 0.0, 0.0], [0.0, 0.0, 0.5]);
        for plane in q {
            // Tail behind, head ahead, along z.
            assert!(plane[0][2] < 0.0 && plane[1][2] > 0.0);
            assert!((plane[0][2] - plane[3][2]).abs() < 1e-9);
        }
    }

    #[test]
    fn it_fades_over_its_lifespan() {
        assert_eq!(alpha(0.0), 1.0);
        assert!(alpha(LIFESPAN as f32).abs() < 1e-6);
    }
}
