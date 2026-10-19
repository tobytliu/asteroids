use std::f32::consts::{PI, TAU};

use crate::{DT, Rng, overlap, wrap};

/// Average radii of the starting field, in px.
const FIELD: [f32; 8] = [48.0, 48.0, 48.0, 30.0, 30.0, 30.0, 18.0, 18.0];
const MIN_SPEED: f32 = 15.0; // px/s
const MAX_SPEED: f32 = 45.0; // px/s
/// Bounces never leave a rock faster than this, in px/s, so a big rock can't
/// fling a small one across the screen.
pub(crate) const SPEED_LIMIT: f32 = 60.0;
/// Rocks slower than this (friction in collisions slowly drains motion) are
/// eased back up to it, so the field never grinds to a halt.
const DRIFT_FLOOR: f32 = MIN_SPEED;
/// How fast that easing is, in px/s².
const DRIFT_RECOVERY: f32 = 4.0;
const MAX_SPAWN_SPIN: f32 = 0.6; // rad/s
/// Collisions never leave a rock spinning faster than this, in rad/s.
pub(crate) const SPIN_LIMIT: f32 = 3.0;
/// Friction between rocks: how much of a hit's push can act sideways along the
/// surfaces, turning glancing blows into spin.
const FRICTION: f32 = 0.4;
/// Rocks spawn at least this far from the ship, in px.
const SPAWN_CLEARANCE: f32 = 120.0;
/// How far each rock's size strays from its `FIELD` entry, as a fraction.
const SIZE_JITTER: f32 = 0.15;
/// Rocks averaging less than this, in px, are too small to show a bite out of
/// their edge or a pinched waist.
const BITE_MIN_MEAN: f32 = 25.0;
/// How many radii describe a rock's outline.
pub const SHAPE_POINTS: usize = 32;
/// How many times a new outline is blurred, which rounds off every corner
/// (each pass reaches one more point either side).
const SMOOTHING: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Asteroid {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    /// Distance from the center to the outline, in px, at `SHAPE_POINTS`
    /// evenly spaced angles in the rock's own frame, starting along `angle`.
    /// The outline runs straight between them.
    pub shape: [f32; SHAPE_POINTS],
    /// The farthest the outline reaches, in px.
    pub radius: f32,
    pub angle: f32,
    /// rad/s, counterclockwise.
    pub spin: f32,
    /// Picks this rock's colors and surface details when drawn.
    pub look: u32,
}

impl Asteroid {
    /// A rock with outline `shape`, not spinning.
    pub fn new(pos: [f32; 2], vel: [f32; 2], shape: [f32; SHAPE_POINTS]) -> Self {
        Self {
            pos,
            vel,
            shape,
            radius: shape.iter().copied().fold(0.0, f32::max),
            angle: 0.0,
            spin: 0.0,
            look: 0,
        }
    }

    pub fn step(&mut self, size: [f32; 2]) {
        let speed = self.vel[0].hypot(self.vel[1]);
        if speed < DRIFT_FLOOR {
            let target = (speed + DRIFT_RECOVERY * DT).min(DRIFT_FLOOR);
            // Keep its direction, or head the way it's facing if it's stopped.
            let dir = if speed > 1e-3 {
                self.vel.map(|v| v / speed)
            } else {
                [self.angle.cos(), self.angle.sin()]
            };
            self.vel = dir.map(|d| d * target);
        }
        for i in 0..2 {
            self.pos[i] = wrap(self.pos[i] + self.vel[i] * DT, size[i]);
        }
        self.angle = (self.angle + self.spin * DT).rem_euclid(TAU);
    }

    /// Distance from the center to the outline in world direction `dir`
    /// (radians), in px.
    pub fn surface(&self, dir: f32) -> f32 {
        let t = (dir - self.angle).rem_euclid(TAU) / TAU * SHAPE_POINTS as f32;
        let i = (t as usize).min(SHAPE_POINTS - 1);
        let next = self.shape[(i + 1) % SHAPE_POINTS];
        self.shape[i] + (next - self.shape[i]) * (t - i as f32)
    }

    /// The outline's area, as a fan of triangles from the center.
    pub(crate) fn mass(&self) -> f32 {
        let sin = (TAU / SHAPE_POINTS as f32).sin();
        (0..SHAPE_POINTS)
            .map(|i| 0.5 * self.shape[i] * self.shape[(i + 1) % SHAPE_POINTS] * sin)
            .sum()
    }

    /// Moment of inertia about the center, for a uniform slab filling the
    /// outline. A long or lopsided rock is harder to spin than a round one of
    /// the same area.
    pub(crate) fn inertia(&self) -> f32 {
        let (sin, cos) = (TAU / SHAPE_POINTS as f32).sin_cos();
        (0..SHAPE_POINTS)
            .map(|i| {
                let (a, b) = (self.shape[i], self.shape[(i + 1) % SHAPE_POINTS]);
                // A triangle with corners at the center, a and b: area / 6
                // times (|a|² + |b|² + a·b).
                0.5 * a * b * sin / 6.0 * (a * a + b * b + a * b * cos)
            })
            .sum()
    }

    fn limit_motion(&mut self) {
        let speed = self.vel[0].hypot(self.vel[1]);
        if speed > SPEED_LIMIT {
            self.vel = self.vel.map(|v| v * SPEED_LIMIT / speed);
        }
        self.spin = self.spin.clamp(-SPIN_LIMIT, SPIN_LIMIT);
    }
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
    let d = [wrap(b[0] - a[0], size[0]), wrap(b[1] - a[1], size[1])];
    let dir = normal[1].atan2(normal[0]);
    let reach = [a_surface(dir), b_surface(dir + PI)];
    let depth = reach[0] + reach[1] - d[0].hypot(d[1]);
    (depth > 0.0).then_some(Contact {
        normal,
        depth,
        reach,
    })
}

/// Lay out the starting rocks, clear of the ship and of each other.
pub(crate) fn spawn_field(rng: &mut Rng, size: [f32; 2], ship_pos: [f32; 2]) -> Vec<Asteroid> {
    let mut rocks: Vec<Asteroid> = Vec::with_capacity(FIELD.len());
    for mean in FIELD {
        let mean = mean * rng.range(1.0 - SIZE_JITTER, 1.0 + SIZE_JITTER);
        let mut rock = Asteroid::new([0.0; 2], [0.0; 2], random_shape(rng, mean));
        // Try random spots until one is clear; give up after a while so a tiny
        // world still gets all its rocks (collisions will push them apart).
        for _ in 0..100 {
            rock.pos = [
                rng.range(-size[0] / 2.0, size[0] / 2.0),
                rng.range(-size[1] / 2.0, size[1] / 2.0),
            ];
            let clear_of_ship =
                overlap(rock.pos, ship_pos, rock.radius + SPAWN_CLEARANCE, size).is_none();
            let clear_of_rocks = rocks
                .iter()
                .all(|r| overlap(rock.pos, r.pos, rock.radius + r.radius + 4.0, size).is_none());
            if clear_of_ship && clear_of_rocks {
                break;
            }
        }
        let heading = rng.range(0.0, TAU);
        let speed = rng.range(MIN_SPEED, MAX_SPEED);
        rock.vel = [heading.cos() * speed, heading.sin() * speed];
        rock.angle = rng.range(0.0, TAU);
        rock.spin = rng.range(-MAX_SPAWN_SPIN, MAX_SPAWN_SPIN);
        rock.look = rng.next_u32();
        rocks.push(rock);
    }
    rocks
}

/// The families of rock outline.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Roughly round, with smooth lumps.
    Round,
    /// Long and lumpy.
    Potato,
    /// Two lobes with a pinched waist, one often bigger than the other.
    Peanut,
    /// Four to six broad, flattish faces with rounded corners, like a
    /// weathered chunk.
    Block,
}

/// A random outline averaging about `mean` px from the center.
fn random_shape(rng: &mut Rng, mean: f32) -> [f32; SHAPE_POINTS] {
    let pick = rng.range(0.0, 1.0);
    let kind = match pick {
        p if p < 0.25 => Kind::Round,
        p if p < 0.6 => Kind::Potato,
        // A waist on a small rock is only an art pixel or two deep.
        p if p < 0.75 && mean < BITE_MIN_MEAN => Kind::Potato,
        p if p < 0.75 => Kind::Peanut,
        _ => Kind::Block,
    };
    let mut shape = shape_of(rng, kind, mean);
    if mean >= BITE_MIN_MEAN && rng.range(0.0, 1.0) < 0.4 {
        let bites = if rng.range(0.0, 1.0) < 0.25 { 2 } else { 1 };
        for _ in 0..bites {
            bite(rng, &mut shape, mean);
        }
    }
    smooth(shape)
}

/// An outline of `kind` averaging `mean` px from the center.
fn shape_of(rng: &mut Rng, kind: Kind, mean: f32) -> [f32; SHAPE_POINTS] {
    // How much each point is nudged at random, which roughens the edge.
    let chips;
    let mut shape: [f32; SHAPE_POINTS] = match kind {
        Kind::Round => {
            chips = 0.02;
            let stretch = rng.range(1.0, 1.15);
            let amount = rng.range(0.06, 0.11);
            let bumps = lumps(rng, amount);
            std::array::from_fn(|i| ellipse(angle_of(i), stretch) * bumps(angle_of(i)))
        }
        Kind::Potato => {
            chips = 0.02;
            let stretch = rng.range(1.3, 1.8);
            let amount = rng.range(0.04, 0.07);
            let bumps = lumps(rng, amount);
            std::array::from_fn(|i| ellipse(angle_of(i), stretch) * bumps(angle_of(i)))
        }
        Kind::Peanut => {
            chips = 0.02;
            let stretch = rng.range(1.4, 1.7);
            // How deep the waist is, and how much bigger one lobe is.
            let pinch = rng.range(0.25, 0.4);
            let lopsided = rng.range(-0.15, 0.15);
            let bumps = lumps(rng, 0.04);
            std::array::from_fn(|i| {
                let theta = angle_of(i);
                ellipse(theta, stretch)
                    * (1.0 - pinch * theta.sin().powi(4))
                    * (1.0 + lopsided * theta.cos())
                    * bumps(theta)
            })
        }
        Kind::Block => {
            chips = 0.04;
            let corners = 4 + (rng.range(0.0, 3.0) as usize).min(2);
            polygon(rng, corners, 0.85, 1.1)
        }
    };
    for r in &mut shape {
        *r *= 1.0 + rng.range(-chips, chips);
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
    1.0 / (theta.cos().powi(2) + (stretch * theta.sin()).powi(2)).sqrt()
}

/// A random smooth bumpiness around 1, of about `amount`, by direction.
fn lumps(rng: &mut Rng, amount: f32) -> impl Fn(f32) -> f32 + use<> {
    let amps: [f32; 3] = std::array::from_fn(|_| rng.range(0.3, 1.0) * amount);
    let phases: [f32; 3] = std::array::from_fn(|_| rng.range(0.0, TAU));
    move |theta| {
        1.0 + (0..3)
            .map(|k| amps[k] * ((k + 2) as f32 * theta + phases[k]).sin())
            .sum::<f32>()
    }
}

/// A polygon with `corners` (at least 4) corners, each `lo` to `hi` from the
/// center, placed on outline points.
fn polygon(rng: &mut Rng, corners: usize, lo: f32, hi: f32) -> [f32; SHAPE_POINTS] {
    let spacing = SHAPE_POINTS as f32 / corners as f32;
    let start = rng.range(0.0, SHAPE_POINTS as f32);
    // Evenly spread, then nudged, never so far that two corners end up half a
    // turn apart, which would leave the center outside the polygon.
    let jitter = spacing / 6.0;
    let at: Vec<usize> = (0..corners)
        .map(|c| {
            let i = start + c as f32 * spacing + rng.range(-jitter, jitter);
            i.round().rem_euclid(SHAPE_POINTS as f32) as usize % SHAPE_POINTS
        })
        .collect();
    let radii: Vec<f32> = (0..corners).map(|_| rng.range(lo, hi)).collect();
    let mut shape = [0.0; SHAPE_POINTS];
    for c in 0..corners {
        let (a, b) = (at[c], at[(c + 1) % corners]);
        let (ra, rb) = (radii[c], radii[(c + 1) % corners]);
        let span = (b + SHAPE_POINTS - a) % SHAPE_POINTS;
        let wedge = angle_of(span);
        // Where a ray from the center meets the straight side from corner a to
        // corner b.
        for k in 0..span {
            let t = angle_of(k);
            shape[(a + k) % SHAPE_POINTS] =
                ra * rb * wedge.sin() / (ra * t.sin() + rb * (wedge - t).sin());
        }
    }
    shape
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
pub(crate) fn collide_all(rocks: &mut [Asteroid], size: [f32; 2]) {
    for j in 1..rocks.len() {
        let (head, tail) = rocks.split_at_mut(j);
        let b = &mut tail[0];
        for a in head {
            let Some(Contact {
                normal: n,
                depth,
                reach: [ra, rb],
            }) = contact(
                a.pos,
                a.radius,
                |dir| a.surface(dir),
                b.pos,
                b.radius,
                |dir| b.surface(dir),
                size,
            )
            else {
                continue;
            };
            let (ma, mb) = (a.mass(), b.mass());
            let (ia, ib) = (a.inertia(), b.inertia());
            // Push apart, the lighter rock moving further.
            for k in 0..2 {
                a.pos[k] = wrap(a.pos[k] - n[k] * depth * mb / (ma + mb), size[k]);
                b.pos[k] = wrap(b.pos[k] + n[k] * depth * ma / (ma + mb), size[k]);
            }

            let rel = [b.vel[0] - a.vel[0], b.vel[1] - a.vel[1]];
            let closing = rel[0] * n[0] + rel[1] * n[1];
            if closing >= 0.0 {
                continue;
            }
            let push = -2.0 * closing / (1.0 / ma + 1.0 / mb);

            // How fast the surfaces slide past each other where they touch,
            // counting spin, along the tangent (the normal turned 90° left).
            let t = [-n[1], n[0]];
            let slide = rel[0] * t[0] + rel[1] * t[1] - b.spin * rb - a.spin * ra;
            // The sideways impulse that would stop the sliding, limited by
            // friction.
            let resistance = 1.0 / ma + 1.0 / mb + ra * ra / ia + rb * rb / ib;
            let rub = (-slide / resistance).clamp(-FRICTION * push, FRICTION * push);

            for k in 0..2 {
                let impulse = push * n[k] + rub * t[k];
                a.vel[k] -= impulse / ma;
                b.vel[k] += impulse / mb;
            }
            a.spin -= ra * rub / ia;
            b.spin -= rb * rub / ib;
            a.limit_motion();
            b.limit_motion();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [Kind; 4] = [Kind::Round, Kind::Potato, Kind::Peanut, Kind::Block];

    fn mean(shape: &[f32; SHAPE_POINTS]) -> f32 {
        shape.iter().sum::<f32>() / SHAPE_POINTS as f32
    }

    #[test]
    fn every_kind_has_the_asked_for_size_and_a_sane_outline() {
        for kind in KINDS {
            for seed in 0..200 {
                let shape = shape_of(&mut Rng::new(seed), kind, 30.0);
                assert!((mean(&shape) - 30.0).abs() < 1e-3, "{kind:?} {seed}");
                for r in shape {
                    assert!(r > 0.4 * 30.0 && r < 1.8 * 30.0, "{kind:?} {seed}: {r}");
                }
            }
        }
    }

    #[test]
    fn every_outline_has_smooth_corners() {
        for mean in [18.0, 30.0, 48.0] {
            for seed in 0..500 {
                let s = random_shape(&mut Rng::new(seed), mean);
                for i in 0..SHAPE_POINTS {
                    let (l, r) = (
                        s[(i + SHAPE_POINTS - 1) % SHAPE_POINTS],
                        s[(i + 1) % SHAPE_POINTS],
                    );
                    // How sharply the outline bends at this point. The tip of a
                    // long potato is about 0.15; an unrounded block corner 0.75.
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
        let rock = Asteroid::new([0.0; 2], [0.0; 2], [30.0; SHAPE_POINTS]);
        let area = PI * 30.0 * 30.0;
        assert!((rock.mass() - area).abs() < 0.01 * area);
        let disc = 0.5 * area * 30.0 * 30.0;
        assert!((rock.inertia() - disc).abs() < 0.02 * disc);
    }

    #[test]
    fn long_rock_is_harder_to_spin_than_a_round_one_of_the_same_mass() {
        let long = Asteroid::new(
            [0.0; 2],
            [0.0; 2],
            shape_of(&mut Rng::new(1), Kind::Potato, 30.0),
        );
        let r = (long.mass() / PI).sqrt();
        let round = Asteroid::new([0.0; 2], [0.0; 2], [r; SHAPE_POINTS]);
        assert!((long.mass() - round.mass()).abs() < 0.02 * round.mass());
        assert!(long.inertia() > 1.05 * round.inertia());
    }
}
