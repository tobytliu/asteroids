//! Shots: a flash at the muzzle when a bolt leaves it, and a burst of sparks
//! where one hits something. Both read only what the sim reports (new bolts
//! and `Snapshot::hits`), and how each weapon's look is all in `ShotStyle`.
//!
//! It's purely visual. The sim knows nothing about it.

use std::f32::consts::PI;

use sim::{EntityId, Hit, Snapshot, WeaponKind};

use super::Emitter;
use super::particles::{Particle, Particles};

/// One kind of spray: how many particles, which way and how fast they go,
/// and how they burn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spray {
    pub count: usize,
    /// How far either side of its direction a particle can go, in radians.
    pub spread: f32,
    /// In px/s.
    pub speed: (f32, f32),
    /// In seconds.
    pub life: (f32, f32),
    /// How hot a particle is when thrown and when it burns out (see
    /// `heat_color` in heat.wgsl).
    pub heat: (f32, f32),
    pub drag: f32,
}

/// How a weapon's shots look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotStyle {
    /// Thrown forward from the muzzle as a bolt leaves it.
    pub muzzle: Spray,
    /// Thrown back the way a bolt came, from where it hit.
    pub sparks: Spray,
    /// A tight white-hot knot at the hit, gone almost at once.
    pub flash: Spray,
}

impl ShotStyle {
    pub fn of(kind: WeaponKind) -> &'static Self {
        match kind {
            WeaponKind::Laser => &LASER,
        }
    }
}

const LASER: ShotStyle = ShotStyle {
    muzzle: Spray {
        count: 4,
        spread: 0.5,
        speed: (40.0, 160.0),
        life: (0.05, 0.12),
        heat: (0.5, 0.05),
        drag: 6.0,
    },
    sparks: Spray {
        count: 14,
        spread: 1.2,
        speed: (80.0, 340.0),
        life: (0.15, 0.4),
        heat: (0.5, 0.0),
        drag: 5.0,
    },
    flash: Spray {
        count: 8,
        spread: PI,
        speed: (0.0, 50.0),
        life: (0.05, 0.1),
        heat: (1.0, 0.6),
        drag: 8.0,
    },
};

/// Muzzle flashes and hits, for every weapon.
#[derive(Default)]
pub struct Shots {
    /// The newest bolt flashed for so far. Bolt IDs only go up, so any bolt
    /// past it is new.
    newest: Option<EntityId>,
}

impl Emitter for Shots {
    /// `world.hits` must be every hit since the last update, each given once.
    fn update(&mut self, world: &Snapshot, _dt: f32, out: &mut Particles) {
        for bolt in &world.bolts {
            if self.newest.is_some_and(|newest| bolt.id <= newest) {
                continue;
            }
            self.newest = Some(bolt.id);
            let body = bolt.body;
            // The bolt's own speed would fling the flash far ahead; it keeps
            // just the ship's.
            let muzzle_speed = bolt.bolt.kind.params().muzzle_speed;
            let ship_vel = [0, 1].map(|i| body.vel[i] - dir(body.angle)[i] * muzzle_speed);
            let style = ShotStyle::of(bolt.bolt.kind);
            throw(&style.muzzle, body.pos, ship_vel, body.angle, out);
        }
        for &Hit { pos, vel, kind } in &world.hits {
            let style = ShotStyle::of(kind);
            let back = vel[1].atan2(vel[0]) + PI;
            throw(&style.sparks, pos, [0.0; 2], back, out);
            throw(&style.flash, pos, [0.0; 2], back, out);
        }
    }
}

/// Throw `spray`'s particles from `pos` around direction `angle`, on top of
/// velocity `base`.
fn throw(spray: &Spray, pos: [f32; 2], base: [f32; 2], angle: f32, out: &mut Particles) {
    for _ in 0..spray.count {
        let a = angle + out.rng.range((-spray.spread, spray.spread));
        let speed = out.rng.range(spray.speed);
        let d = dir(a);
        let particle = Particle {
            pos,
            vel: [0, 1].map(|i| base[i] + d[i] * speed),
            age: 0.0,
            life: out.rng.range(spray.life),
            heat: spray.heat,
            drag: spray.drag,
            seed: out.rng.unit() * 1000.0,
        };
        out.spawn(particle);
    }
}

fn dir(angle: f32) -> [f32; 2] {
    [angle.cos(), angle.sin()]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn shoot(world: &mut sim::World, ship: EntityId) {
        let shoot = sim::Input {
            shoot: true,
            ..sim::Input::default()
        };
        world.step(&BTreeMap::from([(ship, shoot)]));
    }

    #[test]
    fn each_new_bolt_flashes_once() {
        let mut world = sim::World::new(1, [4000.0, 4000.0]);
        let ship = world.spawn_ship();
        shoot(&mut world, ship);
        let (mut shots, mut out) = (Shots::default(), Particles::default());
        let snapshot = world.snapshot();
        shots.update(&snapshot, 0.01, &mut out);
        let flashed = out.len();
        assert_eq!(flashed, LASER.muzzle.count);
        // The same bolt seen again, in a later frame or tick.
        shots.update(&snapshot, 0.01, &mut out);
        world.step(&BTreeMap::new());
        shots.update(&world.snapshot(), 0.01, &mut out);
        assert_eq!(out.len(), flashed);
    }

    #[test]
    fn hits_burst_and_sparks_fly_back_the_way_the_bolt_came() {
        let mut world = sim::World::new(1, [4000.0, 4000.0]).snapshot();
        world.rocks.clear();
        world.hits.push(Hit {
            pos: [100.0, 0.0],
            vel: [1200.0, 0.0],
            kind: WeaponKind::Laser,
        });
        let (mut shots, mut out) = (Shots::default(), Particles::default());
        shots.update(&world, 0.01, &mut out);
        assert_eq!(out.len(), LASER.sparks.count + LASER.flash.count);
        let sparks = out.iter().take(LASER.sparks.count);
        assert!(
            sparks
                .into_iter()
                .all(|p| p.vel[0] <= 0.0 && p.pos == [100.0, 0.0])
        );
    }
}
