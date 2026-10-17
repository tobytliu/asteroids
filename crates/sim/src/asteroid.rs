use std::f32::consts::{PI, TAU};

use crate::{DT, Rng, overlap, wrap};

/// Average radii of the starting field, in px.
const FIELD: [f32; 8] = [48.0, 48.0, 30.0, 30.0, 30.0, 18.0, 18.0, 18.0];
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
/// How many radii describe a rock's outline.
pub const SHAPE_POINTS: usize = 16;

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

    fn mean_radius(&self) -> f32 {
        self.shape.iter().sum::<f32>() / SHAPE_POINTS as f32
    }

    fn mass(&self) -> f32 {
        self.mean_radius().powi(2)
    }

    /// Moment of inertia, treating the rock as a uniform disc.
    fn inertia(&self) -> f32 {
        0.5 * self.mass() * self.mean_radius().powi(2)
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

/// A random outline averaging `mean` px from the center: round and lumpy, a
/// long potato, or chunky and angular.
fn random_shape(rng: &mut Rng, mean: f32) -> [f32; SHAPE_POINTS] {
    let kind = rng.range(0.0, 1.0);
    // How elongated, how big the smooth bumps are, and how much each point is
    // nudged at random (which makes flat facets and sharp corners).
    let (stretch, lumps, chips) = if kind < 0.4 {
        (rng.range(1.0, 1.12), 0.07, 0.03)
    } else if kind < 0.7 {
        (rng.range(1.3, 1.6), 0.05, 0.03)
    } else {
        (rng.range(1.0, 1.2), 0.04, 0.13)
    };
    let amps: [f32; 3] = std::array::from_fn(|_| rng.range(0.3, 1.0) * lumps);
    let phases: [f32; 3] = std::array::from_fn(|_| rng.range(0.0, TAU));
    let mut shape: [f32; SHAPE_POINTS] = std::array::from_fn(|i| {
        let theta = i as f32 / SHAPE_POINTS as f32 * TAU;
        // An ellipse, longest along the rock's heading.
        let ellipse = 1.0 / (theta.cos().powi(2) + (stretch * theta.sin()).powi(2)).sqrt();
        let bumps: f32 = (0..3)
            .map(|k| amps[k] * ((k + 2) as f32 * theta + phases[k]).sin())
            .sum();
        ellipse * (1.0 + bumps) * (1.0 + rng.range(-chips, chips))
    });
    let scale = mean * SHAPE_POINTS as f32 / shape.iter().sum::<f32>();
    for r in &mut shape {
        *r *= scale;
    }
    shape
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
