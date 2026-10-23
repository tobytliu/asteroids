use std::f32::consts::{PI, TAU};

use crate::{Body, DT, Rng, math, overlap};

/// Bounces never leave a rock faster than this, in px/s, so a big rock can't
/// fling a small one across the map.
pub(crate) const SPEED_LIMIT: f32 = 60.0;
/// Collisions never leave a rock spinning faster than this, in rad/s.
pub(crate) const SPIN_LIMIT: f32 = 3.0;
/// Friction between rocks: how much of a hit's push can act sideways along the
/// surfaces, turning glancing blows into spin.
const FRICTION: f32 = 0.4;
/// Rocks spawn at least this far from the spawn point, in px.
pub(crate) const SPAWN_CLEARANCE: f32 = 120.0;
/// How far a small or big rock's size strays from its class's usual
/// size, as a fraction.
const SIZE_JITTER: f32 = 0.15;
/// How many radii describe a rock's outline.
pub const SHAPE_POINTS: usize = 32;
/// How many times a new outline is blurred, which rounds off every corner
/// (each pass reaches one more point either side).
const SMOOTHING: usize = 2;

/// What kind of rock an asteroid is. Each class has its own size and motion
/// (`Class::params`) and its own way of making an outline (`random_shape`),
/// and the client draws each class's surface by its own rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Class {
    Small,
    Medium,
    Big,
    /// 120–250 px mean radius, always above a big rock's.
    Large,
}

/// How a class of rock is sized and moves.
pub(crate) struct Params {
    /// Range of the outline's mean radius, in px.
    pub mean: (f32, f32),
    /// Range of spawn speeds, in px/s.
    pub speed: (f32, f32),
    /// Spawn spin is up to this either way, in rad/s.
    pub max_spin: f32,
    /// Rocks slower than this, in px/s (friction in collisions slowly drains
    /// motion), are eased back up to it, so the field never grinds to a halt.
    pub drift_floor: f32,
    /// Whether the outline may have bites scooped out of its edge.
    pub bites: bool,
}

/// Usual mean radius of a small and a big rock, in px.
const SMALL_MEAN: f32 = 22.0;
const BIG_MEAN: f32 = 100.0;

/// A mean radius range of `mean` give or take `SIZE_JITTER`.
const fn jittered(mean: f32) -> (f32, f32) {
    (mean * (1.0 - SIZE_JITTER), mean * (1.0 + SIZE_JITTER))
}

impl Class {
    pub(crate) const fn params(self) -> Params {
        // Small, medium and big rocks move alike.
        const SPEED: (f32, f32) = (15.0, 45.0);
        const SPIN: f32 = 0.6;
        match self {
            Class::Small => Params {
                mean: jittered(SMALL_MEAN),
                speed: SPEED,
                max_spin: SPIN,
                drift_floor: SPEED.0,
                // Too small to show one.
                bites: false,
            },
            Class::Medium => Params {
                // A wide range, so medium rocks come in many sizes.
                mean: (32.0, 60.0),
                speed: SPEED,
                max_spin: SPIN,
                drift_floor: SPEED.0,
                bites: true,
            },
            Class::Big => Params {
                mean: jittered(BIG_MEAN),
                speed: SPEED,
                max_spin: SPIN,
                drift_floor: SPEED.0,
                bites: true,
            },
            // Slow and barely turning: its rim is far from its center, so even
            // a little spin sweeps it fast.
            Class::Large => Params {
                mean: (120.0, 250.0),
                speed: (8.0, 20.0),
                max_spin: 0.15,
                drift_floor: 8.0,
                bites: true,
            },
        }
    }
}

/// What makes a rock a rock: the parts of it that never change. Where it is
/// and how it moves are in its `Body` and `Spin`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Asteroid {
    pub class: Class,
    /// Distance from the center to the outline, in px, at `SHAPE_POINTS`
    /// evenly spaced angles in the rock's own frame, starting along its body's
    /// angle. The outline runs straight between them.
    pub shape: [f32; SHAPE_POINTS],
    /// The farthest the outline reaches, in px.
    pub radius: f32,
    /// Picks this rock's colors and surface details when drawn.
    pub look: u32,
}

/// How fast a rock turns, in rad/s, counterclockwise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spin(pub f32);

impl Asteroid {
    /// A rock of `class` with outline `shape`.
    pub fn new(class: Class, shape: [f32; SHAPE_POINTS]) -> Self {
        Self {
            class,
            shape,
            radius: shape.iter().copied().fold(0.0, f32::max),
            look: 0,
        }
    }

    /// Distance from the center to the outline in world direction `dir`
    /// (radians), in px, when the rock is turned to `angle`.
    pub fn surface(&self, angle: f32, dir: f32) -> f32 {
        let t = (dir - angle).rem_euclid(TAU) / TAU * SHAPE_POINTS as f32;
        let i = (t as usize).min(SHAPE_POINTS - 1);
        let next = self.shape[(i + 1) % SHAPE_POINTS];
        self.shape[i] + (next - self.shape[i]) * (t - i as f32)
    }

    /// The outline's area, as a fan of triangles from the center.
    pub(crate) fn mass(&self) -> f32 {
        let sin = math::sin(TAU / SHAPE_POINTS as f32);
        (0..SHAPE_POINTS)
            .map(|i| 0.5 * self.shape[i] * self.shape[(i + 1) % SHAPE_POINTS] * sin)
            .sum()
    }

    /// Moment of inertia about the center, for a uniform slab filling the
    /// outline. A long or lopsided rock is harder to spin than a round one of
    /// the same area.
    pub(crate) fn inertia(&self) -> f32 {
        let (sin, cos) = math::sin_cos(TAU / SHAPE_POINTS as f32);
        (0..SHAPE_POINTS)
            .map(|i| {
                let (a, b) = (self.shape[i], self.shape[(i + 1) % SHAPE_POINTS]);
                // A triangle with corners at the center, a and b: area / 6
                // times (|a|² + |b|² + a·b).
                0.5 * a * b * sin / 6.0 * (a * a + b * b + a * b * cos)
            })
            .sum()
    }
}

/// Turn a rock by its spin over one tick.
pub(crate) fn turn(body: &mut Body, spin: Spin) {
    body.angle = (body.angle + spin.0 * DT).rem_euclid(TAU);
}

/// Cap a rock's speed and spin after a collision.
fn limit_motion(body: &mut Body, spin: &mut Spin) {
    let speed = math::hypot(body.vel[0], body.vel[1]);
    if speed > SPEED_LIMIT {
        body.vel = body.vel.map(|v| v * SPEED_LIMIT / speed);
    }
    spin.0 = spin.0.clamp(-SPIN_LIMIT, SPIN_LIMIT);
}

/// One rock, as the rock-rock collision system sees it.
pub(crate) struct Rock<'a> {
    pub body: &'a mut Body,
    pub spin: &'a mut Spin,
    pub asteroid: &'a Asteroid,
}

/// Where two outlines touch.
pub(crate) struct Contact {
    /// Unit vector from the first center toward the second.
    pub normal: [f32; 2],
    /// How far the outlines overlap, in px.
    pub depth: f32,
    /// Each center's distance to its outline along the normal, in px.
    pub reach: [f32; 2],
}

/// Whether two outlines overlap (measuring across the wrapped edges), each
/// given by its center, its farthest reach, and its distance to the outline in
/// a world direction. Outlines are compared along the line between centers.
pub(crate) fn contact(
    a: [f32; 2],
    a_radius: f32,
    a_surface: impl Fn(f32) -> f32,
    b: [f32; 2],
    b_radius: f32,
    b_surface: impl Fn(f32) -> f32,
    size: [f32; 2],
) -> Option<Contact> {
    let (normal, _) = overlap(a, b, a_radius + b_radius, size)?;
    let d = crate::wrapped_offset(a, b, size);
    let dir = math::atan2(normal[1], normal[0]);
    let reach = [a_surface(dir), b_surface(dir + PI)];
    let depth = reach[0] + reach[1] - math::hypot(d[0], d[1]);
    (depth > 0.0).then_some(Contact {
        normal,
        depth,
        reach,
    })
}

/// A random outline for a rock of `class`, sized within its mean radius range.
/// Each class makes its outline its own way; for now they all make a
/// `bitten_potato`.
pub(crate) fn random_shape(rng: &mut Rng, class: Class) -> [f32; SHAPE_POINTS] {
    let params = class.params();
    let mean = rng.range(params.mean.0, params.mean.1);
    match class {
        Class::Small | Class::Medium | Class::Big | Class::Large => {
            bitten_potato(rng, mean, params.bites)
        }
    }
}

/// A smoothed `potato` averaging about `mean` px from the center, sometimes
/// with bites out of its edge if `bites` allows.
fn bitten_potato(rng: &mut Rng, mean: f32, bites: bool) -> [f32; SHAPE_POINTS] {
    let mut shape = potato(rng, mean);
    if bites && rng.range(0.0, 1.0) < 0.4 {
        let bites = if rng.range(0.0, 1.0) < 0.25 { 2 } else { 1 };
        for _ in 0..bites {
            bite(rng, &mut shape, mean);
        }
    }
    smooth(shape)
}

/// A lumpy potato outline averaging `mean` px from the center. Every rock is
/// one, but each varies in how long it is, which end and side are fatter, and
/// where its lumps fall, so no two look alike.
fn potato(rng: &mut Rng, mean: f32) -> [f32; SHAPE_POINTS] {
    let stretch = rng.range(1.1, 1.75);
    // How much fatter one end is than the other, like an egg, and one long
    // side than the other.
    let taper = rng.range(-0.15, 0.15);
    let side = rng.range(-0.1, 0.1);
    let lumpiness = rng.range(0.05, 0.1);
    let bumps = lumps(rng, lumpiness);
    let mut shape: [f32; SHAPE_POINTS] = std::array::from_fn(|i| {
        let theta = angle_of(i);
        ellipse(theta, stretch)
            * (1.0 + taper * math::cos(theta))
            * (1.0 + side * math::sin(theta))
            * bumps(theta)
    });
    // Nudge each point at random, which roughens the edge.
    for r in &mut shape {
        *r *= 1.0 + rng.range(-0.02, 0.02);
    }
    let scale = mean * SHAPE_POINTS as f32 / shape.iter().sum::<f32>();
    shape.map(|r| r * scale)
}

/// The angle of outline point `i` in the rock's own frame.
fn angle_of(i: usize) -> f32 {
    i as f32 / SHAPE_POINTS as f32 * TAU
}

/// Distance to a unit-wide ellipse in direction `theta`, `stretch` times
/// longer along the heading than across it.
fn ellipse(theta: f32, stretch: f32) -> f32 {
    let (x, y) = (math::cos(theta), stretch * math::sin(theta));
    1.0 / (x * x + y * y).sqrt()
}

/// A random smooth bumpiness around 1, of about `amount`, by direction. Made
/// of 2, 3 and 5 bumps per turn: an even count above 2 could dent both long
/// sides at once, pinching a waist like a peanut.
fn lumps(rng: &mut Rng, amount: f32) -> impl Fn(f32) -> f32 + use<> {
    // Finer bumps are kept smaller, so they wrinkle the edge without denting
    // it.
    const BUMPS: [(f32, f32); 3] = [(2.0, 1.0), (3.0, 0.8), (5.0, 0.4)];
    let amps: [f32; 3] = std::array::from_fn(|k| rng.range(0.3, 1.0) * BUMPS[k].1 * amount);
    let phases: [f32; 3] = std::array::from_fn(|_| rng.range(0.0, TAU));
    move |theta| {
        1.0 + (0..3)
            .map(|k| amps[k] * math::sin(BUMPS[k].0 * theta + phases[k]))
            .sum::<f32>()
    }
}

/// `shape` blurred `SMOOTHING` times, so no corner is sharp. Keeps the mean.
fn smooth(mut shape: [f32; SHAPE_POINTS]) -> [f32; SHAPE_POINTS] {
    for _ in 0..SMOOTHING {
        let prev = shape;
        for i in 0..SHAPE_POINTS {
            let (l, r) = (
                (i + SHAPE_POINTS - 1) % SHAPE_POINTS,
                (i + 1) % SHAPE_POINTS,
            );
            shape[i] = 0.25 * prev[l] + 0.5 * prev[i] + 0.25 * prev[r];
        }
    }
    shape
}

/// Scoop a round dent out of the outline, like a crater on the edge.
fn bite(rng: &mut Rng, shape: &mut [f32; SHAPE_POINTS], mean: f32) {
    let center = rng.range(0.0, TAU);
    let half_width = rng.range(20f32, 35f32).to_radians();
    let depth = rng.range(0.12, 0.22) * mean;
    for (i, r) in shape.iter_mut().enumerate() {
        let off = (angle_of(i) - center + PI).rem_euclid(TAU) - PI;
        let x = off / half_width;
        if x.abs() < 1.0 {
            *r = (*r - depth * (1.0 - x * x).sqrt()).max(0.4 * mean);
        }
    }
}

/// Separate overlapping rocks and bounce them off each other. The bounce is
/// perfectly elastic, with mass proportional to area; friction where they touch
/// turns glancing blows into spin. Speed and spin are then capped.
pub(crate) fn collide_all(rocks: &mut [Rock], size: [f32; 2]) {
    for j in 1..rocks.len() {
        let (head, tail) = rocks.split_at_mut(j);
        let b = &mut tail[0];
        for a in head {
            let Some(Contact {
                normal: n,
                depth,
                reach: [ra, rb],
            }) = contact(
                a.body.pos,
                a.asteroid.radius,
                |dir| a.asteroid.surface(a.body.angle, dir),
                b.body.pos,
                b.asteroid.radius,
                |dir| b.asteroid.surface(b.body.angle, dir),
                size,
            )
            else {
                continue;
            };
            let (ma, mb) = (a.asteroid.mass(), b.asteroid.mass());
            let (ia, ib) = (a.asteroid.inertia(), b.asteroid.inertia());
            // Push apart, the lighter rock moving further.
            for k in 0..2 {
                a.body.pos[k] = crate::wrap(a.body.pos[k] - n[k] * depth * mb / (ma + mb), size[k]);
                b.body.pos[k] = crate::wrap(b.body.pos[k] + n[k] * depth * ma / (ma + mb), size[k]);
            }

            let rel = [b.body.vel[0] - a.body.vel[0], b.body.vel[1] - a.body.vel[1]];
            let closing = rel[0] * n[0] + rel[1] * n[1];
            if closing >= 0.0 {
                continue;
            }
            let push = -2.0 * closing / (1.0 / ma + 1.0 / mb);

            // How fast the surfaces slide past each other where they touch,
            // counting spin, along the tangent (the normal turned 90° left).
            let t = [-n[1], n[0]];
            let slide = rel[0] * t[0] + rel[1] * t[1] - b.spin.0 * rb - a.spin.0 * ra;
            // The sideways impulse that would stop the sliding, limited by
            // friction.
            let resistance = 1.0 / ma + 1.0 / mb + ra * ra / ia + rb * rb / ib;
            let rub = (-slide / resistance).clamp(-FRICTION * push, FRICTION * push);

            for k in 0..2 {
                let impulse = push * n[k] + rub * t[k];
                a.body.vel[k] -= impulse / ma;
                b.body.vel[k] += impulse / mb;
            }
            a.spin.0 -= ra * rub / ia;
            b.spin.0 -= rb * rub / ib;
            limit_motion(a.body, a.spin);
            limit_motion(b.body, b.spin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean(shape: &[f32; SHAPE_POINTS]) -> f32 {
        shape.iter().sum::<f32>() / SHAPE_POINTS as f32
    }

    #[test]
    fn every_potato_has_the_asked_for_size_and_a_sane_outline() {
        for seed in 0..500 {
            let shape = potato(&mut Rng::new(seed), 30.0);
            assert!((mean(&shape) - 30.0).abs() < 1e-3, "{seed}");
            for r in shape {
                assert!(r > 0.4 * 30.0 && r < 1.8 * 30.0, "{seed}: {r}");
            }
        }
    }

    /// How much of its convex hull an outline fills.
    fn solidity(shape: &[f32; SHAPE_POINTS]) -> f32 {
        let pts: Vec<[f32; 2]> = (0..SHAPE_POINTS)
            .map(|i| {
                [
                    shape[i] * math::cos(angle_of(i)),
                    shape[i] * math::sin(angle_of(i)),
                ]
            })
            .collect();
        let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
            (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
        };
        // The points already go around the center in order, so drop any that
        // make a right turn until none do.
        let mut hull = pts.clone();
        loop {
            let n = hull.len();
            let Some(i) =
                (0..n).find(|&i| cross(hull[(i + n - 1) % n], hull[i], hull[(i + 1) % n]) < 0.0)
            else {
                break;
            };
            hull.remove(i);
        }
        let area = |p: &[[f32; 2]]| {
            (0..p.len())
                .map(|i| cross([0.0; 2], p[i], p[(i + 1) % p.len()]))
                .sum::<f32>()
                / 2.0
        };
        area(&pts) / area(&hull)
    }

    #[test]
    fn no_potato_has_a_pinched_waist() {
        // A peanut with a waist like the old peanut rocks' fills less of its
        // hull than any potato may.
        let peanut: [f32; SHAPE_POINTS] = std::array::from_fn(|i| {
            let theta = angle_of(i);
            let s = math::sin(theta);
            30.0 * ellipse(theta, 1.5) * (1.0 - 0.3 * s * s * s * s)
        });
        assert!(solidity(&peanut) < 0.98);
        for seed in 0..1000 {
            let s = potato(&mut Rng::new(seed), 30.0);
            assert!(solidity(&s) > 0.98, "{seed}: {}", solidity(&s));
        }
    }

    const CLASSES: [Class; 4] = [Class::Small, Class::Medium, Class::Big, Class::Large];

    #[test]
    fn every_outline_is_sized_for_its_class() {
        for class in CLASSES {
            let (lo, hi) = class.params().mean;
            for seed in 0..500 {
                let m = mean(&random_shape(&mut Rng::new(seed), class));
                // Bites take a little off the mean.
                assert!(m > 0.9 * lo && m < hi + 1e-3, "{class:?} {seed}: {m}");
            }
        }
    }

    #[test]
    fn large_rocks_are_always_wider_than_big_ones() {
        let (lo, hi) = Class::Large.params().mean;
        assert_eq!((lo, hi), (120.0, 250.0));
        assert!(lo > Class::Big.params().mean.1);
    }

    #[test]
    fn every_outline_has_smooth_corners() {
        for class in CLASSES {
            for seed in 0..500 {
                let s = random_shape(&mut Rng::new(seed), class);
                let mean = mean(&s);
                for i in 0..SHAPE_POINTS {
                    let (l, r) = (
                        s[(i + SHAPE_POINTS - 1) % SHAPE_POINTS],
                        s[(i + 1) % SHAPE_POINTS],
                    );
                    // How sharply the outline bends at this point. The tip of a
                    // long potato is about 0.15.
                    let bend = (l - 2.0 * s[i] + r).abs() / mean;
                    assert!(bend < 0.2, "{mean} {seed}: {bend} at {i}");
                }
            }
        }
    }

    #[test]
    fn a_bite_is_a_shallow_dent_in_one_part_of_the_edge() {
        for seed in 0..200 {
            let mut shape = [40.0; SHAPE_POINTS];
            bite(&mut Rng::new(seed), &mut shape, 40.0);
            let dented: Vec<f32> = shape.iter().copied().filter(|&r| r < 40.0).collect();
            // 40–70° wide is 3 to 6 of the 32 points.
            assert!((3..=7).contains(&dented.len()), "{seed}: {shape:?}");
            for r in dented {
                assert!(r >= 40.0 * (1.0 - 0.22), "{seed}: {r}");
            }
        }
    }

    #[test]
    fn round_rock_weighs_and_turns_like_a_disc() {
        let rock = Asteroid::new(Class::Medium, [30.0; SHAPE_POINTS]);
        let area = PI * 30.0 * 30.0;
        assert!((rock.mass() - area).abs() < 0.01 * area);
        let disc = 0.5 * area * 30.0 * 30.0;
        assert!((rock.inertia() - disc).abs() < 0.02 * disc);
    }

    #[test]
    fn long_rock_is_harder_to_spin_than_a_round_one_of_the_same_mass() {
        let long = Asteroid::new(Class::Medium, potato(&mut Rng::new(1), 30.0));
        let r = (long.mass() / PI).sqrt();
        let round = Asteroid::new(Class::Medium, [r; SHAPE_POINTS]);
        assert!((long.mass() - round.mass()).abs() < 0.02 * round.mass());
        assert!(long.inertia() > 1.05 * round.inertia());
    }
}
