//! `BoltEffect` and `BoltRenderer`, as Mekanism's Antiprotonic Nucleosynthesizer screen uses them: the
//! bolt's quads (a random walk from the start to the end, with the same draws in the same order) and
//! the renderer that keeps one bolt alive for its lifespan and spawns the next at once (`CONSECUTIVE`).

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct V3(pub f32, pub f32, pub f32);

impl V3 {
    fn add(self, o: V3) -> V3 {
        V3(self.0 + o.0, self.1 + o.1, self.2 + o.2)
    }
    fn sub(self, o: V3) -> V3 {
        V3(self.0 - o.0, self.1 - o.1, self.2 - o.2)
    }
    fn mul(self, k: f32) -> V3 {
        V3(self.0 * k, self.1 * k, self.2 * k)
    }
    fn length(self) -> f32 {
        (self.0 * self.0 + self.1 * self.1 + self.2 * self.2).sqrt()
    }
    fn normalize(self) -> V3 {
        self.mul(1.0 / self.length())
    }
    fn cross(self, o: V3) -> V3 {
        V3(
            self.1 * o.2 - self.2 * o.1,
            self.2 * o.0 - self.0 * o.2,
            self.0 * o.1 - self.1 * o.0,
        )
    }
}

/// `RandomSource.create()`: unseeded, so any generator with the same distributions will do.
pub struct Rng(u64, Option<f64>);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x5DEE_CE66_D1CE_4E5B, None)
    }
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn float(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn double(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// `nextGaussian`: the polar method, two values per pair of draws.
    pub fn gaussian(&mut self) -> f64 {
        if let Some(g) = self.1.take() {
            return g;
        }
        loop {
            let (v1, v2) = (2.0 * self.double() - 1.0, 2.0 * self.double() - 1.0);
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let m = (-2.0 * s.ln() / s).sqrt();
                self.1 = Some(v2 * m);
                return v1 * m;
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum SpreadFunction {
    Sine,
}
#[derive(Clone, Copy)]
pub enum RandomFunction {
    Gaussian,
}
#[derive(Clone, Copy)]
pub enum SegmentSpreader {
    NoMemory,
    Memory(f32),
}

#[derive(Clone, Copy)]
pub struct BoltRenderInfo {
    pub parallel_noise: f32,
    pub spread_factor: f32,
    pub branch_initiation: f32,
    pub branch_continuation: f32,
    /// Alpha, red, green, blue.
    pub color: u32,
    pub random: RandomFunction,
    pub spread: SpreadFunction,
    pub spreader: SegmentSpreader,
}

impl BoltRenderInfo {
    /// `BoltRenderInfo.DEFAULT` with `color(rgbad(r, g, b, a))`.
    pub fn with_color(r: f32, g: f32, b: f32, a: f32) -> BoltRenderInfo {
        let c = |v: f32| (v * 255.0).round() as u32;
        BoltRenderInfo {
            parallel_noise: 0.1,
            spread_factor: 0.1,
            branch_initiation: 0.0,
            branch_continuation: 0.0,
            color: c(a) << 24 | c(r) << 16 | c(g) << 8 | c(b),
            random: RandomFunction::Gaussian,
            spread: SpreadFunction::Sine,
            spreader: SegmentSpreader::NoMemory,
        }
    }
}

#[derive(Clone, Copy)]
pub struct BoltEffect {
    pub info: BoltRenderInfo,
    pub start: V3,
    pub end: V3,
    pub segments: i32,
    pub count: i32,
    pub size: f32,
}

type Quads = Vec<V3>;

#[derive(Clone, Copy)]
struct Cache {
    prev_end: V3,
    prev_end_right: V3,
    prev_end_back: V3,
}

struct Instr {
    start: V3,
    progress: f32,
    perpendicular: V3,
    cache: Option<Cache>,
    branch: bool,
}

fn orthogonal(vec: V3, rng: &mut Rng) -> V3 {
    let (a, b, c) = (-0.5 + rng.float(), -0.5 + rng.float(), -0.5 + rng.float());
    vec.cross(V3(a, b, c)).normalize()
}

impl BoltEffect {
    fn spread(&self, progress: f32) -> f32 {
        match self.info.spread {
            SpreadFunction::Sine => (f64::from(PI) * f64::from(progress)).sin() as f32,
        }
    }

    fn random(&self, rng: &mut Rng) -> f32 {
        match self.info.random {
            RandomFunction::Gaussian => rng.gaussian() as f32,
        }
    }

    fn segment_add(&self, perpendicular: V3, rand_vec: V3, max_diff: f32, rand: f32) -> V3 {
        match self.info.spreader {
            SegmentSpreader::NoMemory => rand_vec.mul(max_diff * rand),
            SegmentSpreader::Memory(memory) => {
                let next = max_diff * (1.0 - memory) * rand;
                let cur = rand_vec.mul(next);
                let mut dist = perpendicular.add(cur);
                let length = dist.length();
                if length > max_diff {
                    dist = dist.mul(max_diff / length);
                }
                dist.add(cur)
            }
        }
    }

    /// `generate`: one list of quad corners (four per quad, four quads per segment) for each segment.
    pub fn generate(&self, rng: &mut Rng) -> Vec<Quads> {
        let mut quads = Vec::new();
        let diff = self.end.sub(self.start);
        let total = diff.length();
        for _ in 0..self.count {
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(Instr {
                start: self.start,
                progress: 0.0,
                perpendicular: V3::default(),
                cache: None,
                branch: false,
            });
            while let Some(data) = queue.pop_front() {
                let mut perpendicular = data.perpendicular;
                let info = &self.info;
                let progress = data.progress
                    + (1.0 / self.segments as f32)
                        * (1.0 - info.parallel_noise + rng.float() * info.parallel_noise * 2.0);
                let scale = self.spread(progress);
                let segment_end = if progress >= 1.0 && scale <= 0.0 {
                    self.end
                } else {
                    let max_diff = info.spread_factor * scale * total;
                    let rand_vec = orthogonal(diff, rng);
                    let rand = self.random(rng);
                    perpendicular = self.segment_add(perpendicular, rand_vec, max_diff, rand);
                    self.start.add(diff.mul(progress)).add(perpendicular)
                };
                let bolt_size = self.size * (0.5 + (1.0 - progress) * 0.5);
                let (q, cache) = create_quads(data.cache, data.start, segment_end, bolt_size);
                quads.push(q);
                if progress >= 1.0 {
                    break;
                } else if !data.branch {
                    queue.push_back(Instr {
                        start: segment_end,
                        progress,
                        perpendicular,
                        cache: Some(cache),
                        branch: false,
                    });
                } else if rng.float() < info.branch_continuation {
                    queue.push_back(Instr {
                        start: segment_end,
                        progress,
                        perpendicular,
                        cache: Some(cache),
                        branch: true,
                    });
                }
                while rng.float() < info.branch_initiation * (1.0 - progress) {
                    queue.push_back(Instr {
                        start: segment_end,
                        progress,
                        perpendicular,
                        cache: Some(cache),
                        branch: true,
                    });
                }
            }
        }
        quads
    }
}

fn create_quads(cache: Option<Cache>, start_pos: V3, end: V3, size: f32) -> (Quads, Cache) {
    let diff = end.sub(start_pos);
    let right_add = diff.cross(V3(0.5, 0.5, 0.5)).normalize().mul(size);
    let back_add = diff.cross(right_add).normalize().mul(size);
    let right_split = right_add.mul(0.5);
    let start = cache.map_or(start_pos, |c| c.prev_end);
    let start_right = cache.map_or(start.add(right_add), |c| c.prev_end_right);
    let start_back = cache.map_or(start.add(right_split).add(back_add), |c| c.prev_end_back);
    let end_right = end.add(right_add);
    let end_back = end.add(right_split).add(back_add);
    let quads = vec![
        start,
        end,
        end_right,
        start_right, //
        start_right,
        end_right,
        end,
        start, //
        start_right,
        end_right,
        end_back,
        start_back, //
        start_back,
        end_back,
        end_right,
        start_right,
    ];
    (
        quads,
        Cache {
            prev_end: end,
            prev_end_right: end_right,
            prev_end_back: end_back,
        },
    )
}

/// `BoltRenderer.Timestamp`.
#[derive(Clone, Copy, Default)]
pub struct Timestamp {
    pub ticks: i64,
    pub partial: f32,
}

impl Timestamp {
    fn is_passed(self, prev: Timestamp, mut duration: f64) -> bool {
        let passed = self.ticks - prev.ticks;
        if passed as f64 > duration {
            return true;
        }
        duration -= passed as f64;
        if duration >= 1.0 {
            return false;
        }
        f64::from(self.partial - prev.partial) >= duration
    }
}

/// One owner's bolts under `SpawnFunction.CONSECUTIVE` and `FadeFunction.NONE`: a bolt lives `lifespan`
/// ticks, and a new one is generated as soon as it expires. The refresh rate is three per tick.
pub struct BoltRenderer {
    refresh: Timestamp,
    bolt: Option<(Vec<Quads>, Timestamp)>,
    rng: Rng,
}

impl BoltRenderer {
    pub fn new(seed: u64) -> BoltRenderer {
        BoltRenderer {
            refresh: Timestamp::default(),
            bolt: None,
            rng: Rng::new(seed),
        }
    }

    /// `update` then `collectBoltStates`: the quads to draw now.
    pub fn states(&mut self, effect: &BoltEffect, lifespan: f64, now: Timestamp) -> &[Quads] {
        let created = |this: &mut Self| (effect.generate(&mut this.rng), now);
        if self.bolt.is_none() {
            self.bolt = Some(created(self));
        }
        if now.is_passed(self.refresh, 1.0 / 3.0) {
            self.refresh = now;
            if self
                .bolt
                .as_ref()
                .is_some_and(|(_, at)| now.is_passed(*at, lifespan))
            {
                self.bolt = Some(created(self));
            }
        }
        self.bolt.as_ref().map_or(&[], |(q, _)| q.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(count: i32) -> BoltEffect {
        BoltEffect {
            info: BoltRenderInfo::with_color(0.45, 0.45, 0.5, 1.0),
            start: V3(47.0, 52.0, 0.0),
            end: V3(147.0, 52.0, 0.0),
            segments: 15,
            count,
            size: 1.0,
        }
    }

    #[test]
    fn a_bolt_runs_from_the_start_to_the_end() {
        let quads = effect(1).generate(&mut Rng::new(1));
        assert!(quads.len() >= 14, "{}", quads.len());
        let first = &quads[0];
        assert_eq!(first[0], V3(47.0, 52.0, 0.0));
        let last = quads.last().unwrap();
        assert!(last.iter().all(|v| v.0.is_finite() && v.1.is_finite()));
        assert!((last[1].0 - 147.0).abs() < 10.0);
        assert!(quads.iter().all(|q| q.len() == 16));
    }

    #[test]
    fn each_bolt_of_the_count_is_its_own_walk() {
        let one = effect(1).generate(&mut Rng::new(2)).len();
        let three = effect(3).generate(&mut Rng::new(2)).len();
        assert!(three > one * 2);
    }

    #[test]
    fn the_renderer_makes_a_new_bolt_once_the_old_one_is_a_tick_old() {
        let e = effect(1);
        let mut r = BoltRenderer::new(3);
        let a = r
            .states(
                &e,
                1.0,
                Timestamp {
                    ticks: 10,
                    partial: 0.0,
                },
            )
            .to_vec();
        let same = r
            .states(
                &e,
                1.0,
                Timestamp {
                    ticks: 10,
                    partial: 0.2,
                },
            )
            .to_vec();
        assert_eq!(a, same);
        let later = r
            .states(
                &e,
                1.0,
                Timestamp {
                    ticks: 12,
                    partial: 0.0,
                },
            )
            .to_vec();
        assert_ne!(a, later);
    }
}
