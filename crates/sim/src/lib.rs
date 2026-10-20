//! Pure, deterministic game logic.
//!
//! World units are CSS pixels, so things look and handle the same on any
//! screen. The origin is the map's center, +y is up, and angles are radians
//! with 0 pointing +x. The map's opposite edges join, so positions wrap to
//! [-size/2, size/2) on each axis.

// Per-axis `for i in 0..2` loops read most clearly for 2D vector math.
#![allow(clippy::needless_range_loop)]

mod asteroid;
mod rng;
mod ship;

pub use asteroid::{Asteroid, Class, SHAPE_POINTS};
pub use ship::{ACCEL, DRAG, SHIP_RADIUS, Ship, TURN};

use rng::Rng;

pub const DT: f32 = 1.0 / 60.0;
/// The map's width and height in px. Opposite edges join (ADR 0003).
pub const MAP_SIZE: [f32; 2] = [4000.0, 4000.0];
const SHIP_COLLISION_PASSES: usize = 4;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Input {
    pub thrust: bool,
    pub left: bool,
    pub right: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct World {
    /// The map's width and height in px: `MAP_SIZE` in the game, smaller in
    /// tests.
    pub size: [f32; 2],
    pub ship: Ship,
    pub asteroids: Vec<Asteroid>,
}

impl World {
    /// The ship at the center and a field of asteroids laid out from `seed`.
    pub fn new(seed: u64, size: [f32; 2]) -> Self {
        let ship = Ship::default();
        let asteroids = asteroid::spawn_field(&mut Rng::new(seed), size, ship.pos);
        Self {
            size,
            ship,
            asteroids,
        }
    }

    pub fn step(&mut self, input: Input) {
        self.ship.step(input, self.size);
        for rock in &mut self.asteroids {
            rock.step(self.size);
        }
        asteroid::collide_all(&mut self.asteroids, self.size);
        // After the rocks have moved, so a rock drifting into the ship pushes it.
        // A few passes, so a ship squeezed between rocks settles between them
        // instead of being pushed out of one and into the other.
        for _ in 0..SHIP_COLLISION_PASSES {
            for rock in &self.asteroids {
                self.ship.collide(rock, self.size);
            }
        }
    }
}

/// Wrap a coordinate into [-size/2, size/2).
fn wrap(x: f32, size: f32) -> f32 {
    let half = size / 2.0;
    (x + half).rem_euclid(size) - half
}

/// The shortest offset from `a` to `b` in a world of `size`, which may cross
/// the wrapped edges. Each axis is in [-size/2, size/2).
pub fn wrapped_offset(a: [f32; 2], b: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    [wrap(b[0] - a[0], size[0]), wrap(b[1] - a[1], size[1])]
}

/// The point `t` of the way from `a` to `b` (0 is `a`, 1 is `b`), going the
/// short way across the wrapped edges, wrapped into a world of `size`.
pub fn lerp_wrapped(a: [f32; 2], b: [f32; 2], t: f32, size: [f32; 2]) -> [f32; 2] {
    let d = wrapped_offset(a, b, size);
    [0, 1].map(|i| wrap(a[i] + d[i] * t, size[i]))
}

/// The angle `t` of the way from `a` to `b`, in radians, turning the short way
/// round. Unlike `a` and `b` it's measured from `a`, so it stays close to it
/// even if they're many turns apart.
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    a + wrap(b - a, std::f32::consts::TAU) * t
}

/// If circles at `a` and `b` whose radii add up to `radii` overlap (measuring
/// across the wrapped edges), the unit normal pointing from `a` to `b` and how
/// deep they overlap.
fn overlap(a: [f32; 2], b: [f32; 2], radii: f32, size: [f32; 2]) -> Option<([f32; 2], f32)> {
    let d = wrapped_offset(a, b, size);
    let dist_sq = d[0] * d[0] + d[1] * d[1];
    if dist_sq >= radii * radii {
        return None;
    }
    let dist = dist_sq.sqrt();
    let normal = if dist > 1e-6 {
        [d[0] / dist, d[1] / dist]
    } else {
        [1.0, 0.0]
    };
    Some((normal, radii - dist))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: [f32; 2] = [1280.0, 800.0];

    /// A round rock.
    fn rock(pos: [f32; 2], vel: [f32; 2], radius: f32) -> Asteroid {
        Asteroid::new(Class::Medium, pos, vel, [radius; SHAPE_POINTS])
    }

    /// The ship parked out of the way.
    fn far_ship() -> Ship {
        Ship {
            pos: [0.0, 300.0],
            ..Ship::default()
        }
    }

    fn world(ship: Ship, asteroids: Vec<Asteroid>) -> World {
        World {
            size: SIZE,
            ship,
            asteroids,
        }
    }

    #[test]
    fn new_world_spawns_rocks_clear_of_ship_and_each_other() {
        for seed in 0..20 {
            let w = World::new(seed, SIZE);
            assert_eq!(w.asteroids.len(), 9);
            assert_eq!(
                w.asteroids
                    .iter()
                    .filter(|a| a.class == Class::Large)
                    .count(),
                1
            );
            for (i, a) in w.asteroids.iter().enumerate() {
                assert!(overlap(a.pos, w.ship.pos, a.radius + SHIP_RADIUS, SIZE).is_none());
                for b in &w.asteroids[i + 1..] {
                    assert!(overlap(a.pos, b.pos, a.radius + b.radius, SIZE).is_none());
                }
            }
        }
    }

    #[test]
    fn rock_count_scales_with_map_area() {
        let w = World::new(7, MAP_SIZE);
        let count = |class| w.asteroids.iter().filter(|a| a.class == class).count();
        assert_eq!(count(Class::Large), 16);
        assert_eq!(count(Class::Big), 64);
        assert_eq!(count(Class::Medium), 48);
        assert_eq!(count(Class::Small), 16);
        for a in &w.asteroids {
            assert!(overlap(a.pos, w.ship.pos, a.radius + SHIP_RADIUS, MAP_SIZE).is_none());
        }
    }

    #[test]
    fn wrapped_offset_takes_the_short_way_across_the_edge() {
        let d = wrapped_offset([600.0, -390.0], [-600.0, 390.0], SIZE);
        assert_eq!(d, [80.0, -20.0]);
        assert_eq!(wrapped_offset([0.0, 0.0], [10.0, -5.0], SIZE), [10.0, -5.0]);
    }

    #[test]
    fn lerp_wrapped_blends_the_short_way_across_the_edge() {
        assert_eq!(
            lerp_wrapped([0.0, 0.0], [10.0, -20.0], 0.5, SIZE),
            [5.0, -10.0]
        );
        // 630 to -630 is 20 px to the right across the edge at 640.
        assert_eq!(
            lerp_wrapped([630.0, 0.0], [-630.0, 0.0], 0.25, SIZE),
            [635.0, 0.0]
        );
        assert_eq!(
            lerp_wrapped([630.0, 0.0], [-630.0, 0.0], 0.75, SIZE),
            [-635.0, 0.0]
        );
    }

    #[test]
    fn lerp_angle_turns_the_short_way() {
        use std::f32::consts::{PI, TAU};
        assert!((lerp_angle(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
        // From just below a full turn to just above zero: through 2π, not back.
        let a = lerp_angle(TAU - 0.1, 0.1, 0.5);
        assert!((a - TAU).abs() < 1e-5, "{a}");
        // Many turns apart still blends from `a` by the short way.
        let a = lerp_angle(10.0 * TAU, 0.2 - 4.0 * TAU, 0.5);
        assert!((a - (10.0 * TAU + 0.1)).abs() < 1e-4, "{a}");
        assert!(lerp_angle(0.0, PI - 0.1, 1.0) > 0.0);
    }

    #[test]
    fn rocks_drift_and_wrap() {
        let mut w = world(Ship::default(), vec![rock([630.0, 0.0], [30.0, 0.0], 20.0)]);
        for _ in 0..60 {
            w.step(Input::default());
        }
        // 630 + 30 = 660, which wraps past the right edge (640) to -620.
        assert!((w.asteroids[0].pos[0] + 620.0).abs() < 0.01);
    }

    #[test]
    fn equal_rocks_bounce_head_on() {
        let far = Ship {
            pos: [0.0, 300.0],
            ..Ship::default()
        };
        let mut w = world(
            far,
            vec![
                rock([-100.0, 0.0], [40.0, 0.0], 30.0),
                rock([100.0, 0.0], [-40.0, 0.0], 30.0),
            ],
        );
        for _ in 0..180 {
            w.step(Input::default());
        }
        // Equal masses in a perfectly elastic head-on hit swap velocities.
        assert!((w.asteroids[0].vel[0] + 40.0).abs() < 0.01);
        assert!((w.asteroids[1].vel[0] - 40.0).abs() < 0.01);
        let gap = w.asteroids[1].pos[0] - w.asteroids[0].pos[0];
        assert!(gap >= 60.0);
    }

    /// Linear momentum (m v) and angular momentum about the origin
    /// (m (x vy - y vx) + I ω).
    fn momenta(rocks: &[Asteroid]) -> ([f32; 2], f32) {
        rocks.iter().fold(([0.0; 2], 0.0), |(p, l), r| {
            let m = r.mass();
            (
                [p[0] + m * r.vel[0], p[1] + m * r.vel[1]],
                l + m * (r.pos[0] * r.vel[1] - r.pos[1] * r.vel[0]) + r.inertia() * r.spin,
            )
        })
    }

    #[test]
    fn rock_collisions_conserve_momentum_and_angular_momentum() {
        // Just touching, off-center and spinning, so friction comes into play.
        let mut big = rock([-30.0, -200.0], [25.0, 0.0], 40.0);
        big.spin = -1.0;
        let mut rocks = vec![big, rock([29.0, -190.0], [-20.0, 5.0], 20.0)];
        let (p0, l0) = momenta(&rocks);
        asteroid::collide_all(&mut rocks, SIZE);
        let (p1, l1) = momenta(&rocks);
        assert!(rocks[1].vel[0] > 0.0, "small rock should rebound");
        assert!(rocks[1].spin.abs() > 0.05, "friction should spin it");
        for i in 0..2 {
            assert!((p0[i] - p1[i]).abs() < 1.0, "{p0:?} vs {p1:?}");
        }
        assert!((l1 - l0).abs() < 0.005 * l0.abs(), "{l0} vs {l1}");
    }

    #[test]
    fn big_rock_cannot_fling_a_small_one_too_fast() {
        let far = Ship {
            pos: [0.0, 300.0],
            ..Ship::default()
        };
        let mut w = world(
            far,
            vec![
                rock([-100.0, 0.0], [45.0, 0.0], 48.0),
                rock([100.0, 0.0], [-45.0, 0.0], 18.0),
            ],
        );
        for _ in 0..240 {
            w.step(Input::default());
        }
        let small = w.asteroids[1].vel;
        assert!(small[0] > 0.0, "small rock should rebound");
        assert!(small[0].hypot(small[1]) <= asteroid::SPEED_LIMIT + 1e-3);
    }

    #[test]
    fn glancing_hit_spins_both_rocks() {
        // A passes just under B (moving the other way), rubbing B's underside
        // to the right and its own top to the left: both turn counterclockwise.
        let mut w = world(
            far_ship(),
            vec![
                rock([-100.0, -100.0], [40.0, 0.0], 30.0),
                rock([100.0, -60.0], [-15.0, 0.0], 30.0),
            ],
        );
        for _ in 0..240 {
            w.step(Input::default());
        }
        assert!(w.asteroids[0].spin > 0.05, "{}", w.asteroids[0].spin);
        assert!(w.asteroids[1].spin > 0.05, "{}", w.asteroids[1].spin);
    }

    #[test]
    fn spinning_rock_kicks_another_sideways_and_counter_spins_it() {
        let mut spinner = rock([-100.0, 0.0], [40.0, 0.0], 30.0);
        spinner.spin = 2.0;
        let mut w = world(
            far_ship(),
            vec![spinner, rock([100.0, 0.0], [-15.0, 0.0], 30.0)],
        );
        for _ in 0..240 {
            w.step(Input::default());
        }
        let (a, b) = (w.asteroids[0], w.asteroids[1]);
        // Like meshing gears: A's right side moves up, dragging B's left side
        // up, so B turns clockwise and heads up as well as away.
        assert!(b.spin < -0.05, "{}", b.spin);
        assert!(b.vel[1] > 0.5, "{:?}", b.vel);
        assert!(a.spin < 2.0);
    }

    #[test]
    fn spin_is_capped() {
        let mut spinner = rock([-100.0, 0.0], [45.0, 0.0], 48.0);
        spinner.spin = 3.0;
        let mut w = world(
            far_ship(),
            vec![spinner, rock([100.0, 0.0], [-45.0, 0.0], 18.0)],
        );
        for _ in 0..240 {
            w.step(Input::default());
        }
        for r in &w.asteroids {
            assert!(r.spin.abs() <= asteroid::SPIN_LIMIT + 1e-6);
        }
    }

    #[test]
    fn rocks_collide_across_the_wrapped_edge() {
        let far = Ship {
            pos: [0.0, 300.0],
            ..Ship::default()
        };
        let mut w = world(
            far,
            vec![
                rock([600.0, 0.0], [40.0, 0.0], 20.0),
                rock([-600.0, 0.0], [-40.0, 0.0], 20.0),
            ],
        );
        for _ in 0..120 {
            w.step(Input::default());
        }
        assert!(w.asteroids[0].vel[0] < 0.0);
        assert!(w.asteroids[1].vel[0] > 0.0);
    }

    #[test]
    fn ship_cannot_fly_through_a_rock_and_does_not_move_it() {
        let rocks = vec![rock([150.0, 0.0], [-15.0, 0.0], 30.0)];
        let mut w = world(Ship::default(), rocks.clone());
        let mut without_ship = world(far_ship(), rocks);
        let thrust = Input {
            thrust: true,
            ..Input::default()
        };
        for _ in 0..300 {
            w.step(thrust);
            without_ship.step(Input::default());
            assert_eq!(w.asteroids, without_ship.asteroids);
            let gap = w.asteroids[0].pos[0] - w.ship.pos[0];
            assert!(gap >= 30.0 + SHIP_RADIUS - 1e-3, "{gap}");
        }
    }

    #[test]
    fn slowed_rocks_ease_back_up_to_a_drift() {
        let mut w = world(far_ship(), vec![rock([0.0, 0.0], [2.0, 0.0], 30.0)]);
        for _ in 0..600 {
            w.step(Input::default());
        }
        let v = w.asteroids[0].vel;
        assert!((v[0] - 15.0).abs() < 1e-3 && v[1] == 0.0, "{v:?}");
    }

    #[test]
    fn drifting_rock_pushes_the_ship() {
        let mut w = world(
            Ship::default(),
            vec![rock([-100.0, 0.0], [60.0, 0.0], 30.0)],
        );
        for _ in 0..120 {
            w.step(Input::default());
        }
        let rock = w.asteroids[0];
        assert_eq!(rock.vel, [60.0, 0.0]);
        assert!(w.ship.pos[0] - rock.pos[0] >= 30.0 + SHIP_RADIUS - 1e-3);
    }

    #[test]
    fn same_seed_and_inputs_are_bit_identical() {
        let run = || {
            let mut w = World::new(42, SIZE);
            for tick in 0u32..1000 {
                w.step(Input {
                    thrust: tick % 5 < 3,
                    left: tick % 13 < 4,
                    right: tick % 17 < 3,
                });
            }
            w
        };
        assert_eq!(run(), run());
    }
}
