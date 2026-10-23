//! Fire: a flame drawn as a plume, throwing sparks and embers into the shared
//! particle pool. Fire never knows what it's attached to. It sees a `Pose`
//! (where something is and how it moves) and a `Mount` (where on it the fire
//! sits, which way it points, its style and what lights it), so anything can
//! burn: today a ship's engine, later missiles and manoeuvring thrusters.
//!
//! It's purely visual. The sim knows nothing about it.

use std::collections::BTreeMap;
use std::f32::consts::PI;

use sim::{EntityId, ShipState, Snapshot};

use super::Emitter;
use super::particles::{Particle, Particles};
use crate::PIXEL;

/// Springs are stepped this often, in seconds, however long a frame is, so
/// they behave the same at any frame rate.
const SUBSTEP: f32 = 1.0 / 240.0;
/// Anything that moved further than this in one frame, in px, jumped (it
/// respawned, say), so no sparks are strewn along the gap.
const MAX_JUMP: f32 = 200.0;

/// Where something that's burning is and how it moves, in map px and
/// radians, as fire sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub angle: f32,
    /// How fast it's turning, in rad/s, counterclockwise.
    pub spin: f32,
}

impl Pose {
    fn of_ship(ship: &ShipState) -> Self {
        Self {
            pos: ship.body.pos,
            vel: ship.body.vel,
            angle: ship.body.angle,
            spin: ship.spin,
        }
    }
}

/// Where a fire sits on what's burning, and what it looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mount {
    /// The nozzle, in px from the center, in the burner's frame (+x along
    /// its heading, +y to its left).
    pub offset: [f32; 2],
    /// Which way the fire points, in radians from the burner's heading.
    pub dir: f32,
    pub style: &'static FireStyle,
    pub drive: Drive,
}

/// What lights a fire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drive {
    /// Burns while its burner thrusts.
    Thrust,
}

/// What a burner's pilot is doing this frame, as far as its fires care.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    pub thrust: bool,
    /// How fast the burner is going, 0 at rest to 1 at its top speed.
    pub pace: f32,
}

impl Drive {
    /// How bright a fire on this drive wants to be, 0 to 1.
    fn target(self, controls: Controls) -> f32 {
        match self {
            Drive::Thrust => {
                if controls.thrust {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// How a kind of fire looks and moves. New kinds of fire are new constants.
#[derive(Debug, PartialEq)]
pub struct FireStyle {
    /// How fast it flares up when lit and dies down when not, per second.
    pub ignite_rate: f32,
    pub fade_rate: f32,
    pub plume: PlumeStyle,
    pub sparks: Spray,
    pub embers: Spray,
}

/// The flame itself, drawn in fire.wgsl.
#[derive(Debug, PartialEq)]
pub struct PlumeStyle {
    /// Longest flame, in art pixels.
    pub len: f32,
    /// How wide it billows out, in art pixels either side.
    pub width: f32,
    /// How fast gas leaves the nozzle, in art pixels per second. Slower gas
    /// trails further behind a turn.
    pub exhaust_speed: f32,
    /// The most its tail can swing from where it points, in radians.
    pub max_swing: f32,
    /// The plume's tail is a spring that chases how fast its burner turns:
    /// how fast it swings, in rad/s, and how quickly that dies away (1 eases
    /// in without overshooting; less whips past and back).
    pub whip_freq: f32,
    pub whip_damping: f32,
}

/// Particles a fire throws out of its nozzle.
#[derive(Debug, PartialEq)]
pub struct Spray {
    /// Particles per second at full intensity.
    pub rate: f32,
    /// Below this intensity it throws none.
    pub min_intensity: f32,
    /// How fast they leave the nozzle, in px/s, on top of the nozzle's own
    /// velocity.
    pub speed: (f32, f32),
    /// How far either side of the nozzle's center they're thrown from, in
    /// art pixels, so they come off the flame's width rather than a point.
    pub width: f32,
    /// The most each strays from where the fire points, in radians. Ones
    /// thrown from the edges lean outward, so the spray fans like the flame.
    pub cone: f32,
    /// How much of that lean follows where across the nozzle it was thrown
    /// from, 0 to 1. The rest is random scatter.
    pub fan: f32,
    /// How long they last, in seconds, with the burner at rest and at its
    /// top speed: the faster it goes, the further they reach.
    pub life: (f32, f32),
    pub life_at_speed: (f32, f32),
    /// How hot they are when thrown and when they burn out, at rest and at
    /// top speed: the faster it goes, the more of them burn yellow and white.
    pub heat: (f32, f32),
    pub heat_at_speed: (f32, f32),
    pub drag: f32,
}

/// A ship's main engine.
pub const ENGINE: FireStyle = FireStyle {
    ignite_rate: 14.0,
    fade_rate: 4.0,
    plume: PlumeStyle {
        len: 16.0,
        width: 3.2,
        exhaust_speed: 170.0,
        max_swing: 1.0,
        whip_freq: 12.0,
        whip_damping: 1.0,
    },
    sparks: Spray {
        rate: 160.0,
        min_intensity: 0.3,
        speed: (220.0, 300.0),
        width: 1.5,
        cone: 0.45,
        fan: 0.6,
        life: (0.12, 0.2),
        life_at_speed: (0.3, 0.45),
        heat: (1.0, 0.4),
        heat_at_speed: (1.0, 0.65),
        drag: 1.5,
    },
    embers: Spray {
        rate: 60.0,
        min_intensity: 0.3,
        speed: (100.0, 150.0),
        width: 2.0,
        cone: 0.3,
        fan: 0.5,
        life: (0.22, 0.35),
        life_at_speed: (0.5, 0.75),
        heat: (0.7, 0.35),
        heat_at_speed: (0.85, 0.55),
        drag: 1.0,
    },
};

/// A ship's fires: one engine, its nozzle just inside the sprite's tail,
/// pointing straight back.
const SHIP_MOUNTS: [Mount; 1] = [Mount {
    offset: [-(crate::sprite::SHIP.len() as f32 / 2.0 - 1.5) * PIXEL, 0.0],
    dir: PI,
    style: &ENGINE,
    drive: Drive::Thrust,
}];

/// Something with fires on it, this frame.
struct Burner {
    id: EntityId,
    pose: Pose,
    mounts: &'static [Mount],
    controls: Controls,
}

/// Everything in `world` that has fires. The only place that knows which
/// entities burn and where.
fn burners(world: &Snapshot) -> impl Iterator<Item = Burner> + '_ {
    world.ships.iter().map(|ship| Burner {
        id: ship.id,
        pose: Pose::of_ship(ship),
        mounts: &SHIP_MOUNTS,
        controls: Controls {
            thrust: ship.input.thrust,
            pace: (ship.body.vel[0].hypot(ship.body.vel[1]) / sim::MAX_SPEED).min(1.0),
        },
    })
}

/// One fire's state from frame to frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fire {
    pub mount: Mount,
    /// Its burner, as of the last update.
    pub pose: Pose,
    /// 0 (out) to 1 (full).
    pub intensity: f32,
    /// How fast its burner is going, 0 at rest to 1 at top speed.
    pub pace: f32,
    /// The turn rate the plume's tail is showing, in rad/s, and how fast
    /// that's changing. It springs after the burner's real turn rate, so the
    /// tail trails into a turn and eases back when it stops.
    pub lag: f32,
    lag_rate: f32,
    /// Fractions of a particle owed to the next frame, so a low rate still
    /// sprays evenly: sparks, embers.
    owed: [f32; 2],
    /// Picks its flicker, so two fires never flicker together.
    pub seed: f32,
}

impl Fire {
    /// How far the plume's tail bends per art pixel, in radians.
    pub fn bend(&self) -> f32 {
        self.lag / self.mount.style.plume.exhaust_speed
    }
}

/// Every fire, by its burner and which of the burner's mounts it's on.
#[derive(Default)]
pub struct Fires(BTreeMap<(EntityId, usize), Fire>);

impl Fires {
    /// Burner `id`'s fire on mount `mount`, if it has one.
    pub fn get(&self, id: EntityId, mount: usize) -> Option<&Fire> {
        self.0.get(&(id, mount))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Fire> {
        self.0.values()
    }
}

impl Emitter for Fires {
    fn update(&mut self, world: &Snapshot, dt: f32, out: &mut Particles) {
        let burners: Vec<Burner> = burners(world).collect();
        self.0.retain(|(id, mount), _| {
            burners
                .iter()
                .any(|b| b.id == *id && *mount < b.mounts.len())
        });
        for burner in &burners {
            for (i, mount) in burner.mounts.iter().enumerate() {
                let seed = out.rng.unit();
                let fire = self.0.entry((burner.id, i)).or_insert(Fire {
                    mount: *mount,
                    pose: burner.pose,
                    intensity: 0.0,
                    pace: 0.0,
                    lag: burner.pose.spin,
                    lag_rate: 0.0,
                    owed: [0.0; 2],
                    seed: seed * 1000.0,
                });
                let last = fire.pose;
                fire.mount = *mount;
                fire.pose = burner.pose;
                fire.pace = burner.controls.pace;
                ease_intensity(fire, mount.drive.target(burner.controls), dt);
                whip(fire, dt);
                let style = mount.style;
                for (k, spray) in [&style.sparks, &style.embers].into_iter().enumerate() {
                    spray_particles(fire, k, spray, last, dt, world.size, out);
                }
            }
        }
    }
}

/// Ease a fire's intensity toward `target`.
fn ease_intensity(fire: &mut Fire, target: f32, dt: f32) {
    let style = fire.mount.style;
    let rate = if target > fire.intensity {
        style.ignite_rate
    } else {
        style.fade_rate
    };
    fire.intensity += (target - fire.intensity) * (1.0 - (-rate * dt).exp());
}

/// Spring the plume's tail after its burner's turn rate for `dt` seconds.
fn whip(fire: &mut Fire, dt: f32) {
    let plume = &fire.mount.style.plume;
    let (w, zeta) = (plume.whip_freq, plume.whip_damping);
    let mut left = dt;
    while left > 0.0 {
        let h = left.min(SUBSTEP);
        let pull = w * w * (fire.pose.spin - fire.lag) - 2.0 * zeta * w * fire.lag_rate;
        fire.lag_rate += pull * h;
        fire.lag += fire.lag_rate * h;
        left -= h;
    }
}

/// Throw this frame's particles from one of a fire's sprays. They're strewn
/// along the path its nozzle took since `last`, each as if thrown at its
/// moment in the frame, so a fast turn leaves an unbroken arc.
fn spray_particles(
    fire: &mut Fire,
    k: usize,
    spray: &Spray,
    last: Pose,
    dt: f32,
    size: [f32; 2],
    out: &mut Particles,
) {
    if fire.intensity < spray.min_intensity {
        fire.owed[k] = 0.0;
        return;
    }
    fire.owed[k] += spray.rate * fire.intensity * dt;
    let count = fire.owed[k].floor();
    fire.owed[k] -= count;
    let now = fire.pose;
    let moved = sim::wrapped_offset(last.pos, now.pos, size);
    let from = if moved[0].hypot(moved[1]) > MAX_JUMP {
        now
    } else {
        last
    };
    let mount = fire.mount;
    for n in 0..count as usize {
        // How far through the frame this one was thrown.
        let t = (n as f32 + out.rng.unit()) / count;
        let pos = sim::lerp_wrapped(from.pos, now.pos, t, size);
        let angle = sim::lerp_angle(from.angle, now.angle, t);
        // Where across the nozzle it comes from, -1 (right edge) to 1 (left).
        let across = 2.0 * out.rng.unit() - 1.0;
        let aim = angle + mount.dir;
        let edge = [-aim.sin(), aim.cos()].map(|v| v * across * spray.width * PIXEL);
        let nozzle = rotate(mount.offset, angle);
        let lean = spray.fan * across + (1.0 - spray.fan) * (2.0 * out.rng.unit() - 1.0);
        let dir = aim + spray.cone * lean;
        let speed = out.rng.range(spray.speed);
        // The nozzle's own velocity: its burner's, plus its swing as the
        // burner turns, which flings sparks outward through a turn.
        let vel = [
            now.vel[0] + speed * dir.cos() - now.spin * nozzle[1],
            now.vel[1] + speed * dir.sin() + now.spin * nozzle[0],
        ];
        let mut particle = Particle {
            pos: [0, 1].map(|i| pos[i] + nozzle[i] + edge[i]),
            vel,
            age: 0.0,
            life: out.rng.range(between(spray.life, spray.life_at_speed, fire.pace)),
            heat: between(spray.heat, spray.heat_at_speed, fire.pace),
            drag: spray.drag,
            seed: out.rng.unit() * 1000.0,
        };
        particle.advance((1.0 - t) * dt, size);
        out.spawn(particle);
    }
}

/// The pair `t` of the way from `a` to `b`.
fn between(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// `v` turned counterclockwise by `angle`.
fn rotate(v: [f32; 2], angle: f32) -> [f32; 2] {
    let (sin, cos) = angle.sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// A world with ships `a` and `b` at rest, both pointing along +x.
    fn world() -> (Snapshot, EntityId, EntityId) {
        let mut world = sim::World::new(1, [4000.0, 4000.0]);
        let (a, b) = (world.spawn_ship(), world.spawn_ship());
        let mut snapshot = world.snapshot();
        for ship in &mut snapshot.ships {
            ship.body = sim::Body::default();
        }
        snapshot.ships[1].body.pos = [1000.0, 0.0];
        (snapshot, a, b)
    }

    /// Run fires and particles for `frames` frames, calling `each` before
    /// every frame.
    fn run(
        world: &mut Snapshot,
        fires: &mut Fires,
        particles: &mut Particles,
        frames: usize,
        mut each: impl FnMut(&mut Snapshot),
    ) {
        for _ in 0..frames {
            each(world);
            particles.step(DT, world.size);
            fires.update(world, DT, particles);
        }
    }

    /// The widest angle, in radians, between particles' velocities, for a
    /// ship at rest that started out pointing along +x.
    fn fan(particles: &Particles) -> f32 {
        // Measured from straight back, so it doesn't wrap at ±π.
        let dirs: Vec<f32> = particles
            .iter()
            .map(|p| (-p.vel[1]).atan2(-p.vel[0]))
            .collect();
        let (lo, hi) = dirs
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &d| (lo.min(d), hi.max(d)));
        hi - lo
    }

    #[test]
    fn only_a_thrusting_ship_lights_up() {
        let (mut world, a, b) = world();
        world.ships[0].input.thrust = true;
        let (mut fires, mut particles) = (Fires::default(), Particles::default());
        run(&mut world, &mut fires, &mut particles, 30, |_| {});
        assert!(fires.get(a, 0).unwrap().intensity > 0.9);
        assert_eq!(fires.get(b, 0).unwrap().intensity, 0.0);
        assert!(!particles.is_empty());
        // Every particle came from a's engine, behind it.
        assert!(particles.iter().all(|p| p.pos[0] < 0.0 && p.pos[0] > -200.0));
    }

    #[test]
    fn sparks_from_a_straight_flight_fan_evenly_from_the_flames_width() {
        let (mut world, _, _) = world();
        world.ships[0].input.thrust = true;
        let (mut fires, mut particles) = (Fires::default(), Particles::default());
        run(&mut world, &mut fires, &mut particles, 60, |_| {});
        let sides: Vec<f32> = particles.iter().map(|p| p.pos[1]).collect();
        let mean = sides.iter().sum::<f32>() / sides.len() as f32;
        assert!(mean.abs() < 3.0, "lopsided by {mean} px");
        assert!(sides.iter().any(|&y| y > 5.0) && sides.iter().any(|&y| y < -5.0));
        let cone = ENGINE.sparks.cone.max(ENGINE.embers.cone);
        assert!(fan(&particles) <= 2.0 * cone + 1e-3, "{}", fan(&particles));
    }

    #[test]
    fn sparks_fan_out_further_with_a_turn() {
        let fan_at = |spin: f32| {
            let (mut world, _, _) = world();
            world.ships[0].input.thrust = true;
            world.ships[0].spin = spin;
            let (mut fires, mut particles) = (Fires::default(), Particles::default());
            run(&mut world, &mut fires, &mut particles, 20, |w| {
                w.ships[0].body.angle += spin * DT;
            });
            fan(&particles)
        };
        let (straight, slow, fast) = (fan_at(0.0), fan_at(3.0), fan_at(7.0));
        assert!(straight < slow && slow < fast, "{straight} < {slow} < {fast}");
        assert!(fast > straight + 1.0, "{fast} vs {straight}");
    }

    #[test]
    fn sparks_reach_further_and_burn_brighter_the_faster_the_ship_flies() {
        // The average distance from the nozzle and heat of sparks thrown by a
        // ship at `speed`, measured relative to the ship.
        let spray_at = |speed: f32| {
            let (mut world, _, _) = world();
            world.ships[0].input.thrust = true;
            world.ships[0].body.vel = [speed, 0.0];
            let (mut fires, mut particles) = (Fires::default(), Particles::default());
            run(&mut world, &mut fires, &mut particles, 60, |w| {
                let v = w.ships[0].body.vel;
                w.ships[0].body.pos = [0, 1].map(|i| w.ships[0].body.pos[i] + v[i] * DT);
            });
            let ship = world.ships[0].body.pos;
            let n = particles.len() as f32;
            let reach = particles.iter().map(|p| ship[0] - p.pos[0]).sum::<f32>() / n;
            let heat = particles.iter().map(|p| p.heat_now()).sum::<f32>() / n;
            (reach, heat)
        };
        let (slow, fast) = (spray_at(0.0), spray_at(sim::MAX_SPEED));
        assert!(fast.0 > 1.2 * slow.0, "reach {} vs {}", fast.0, slow.0);
        assert!(fast.1 > slow.1 + 0.1, "heat {} vs {}", fast.1, slow.1);
    }

    #[test]
    fn turning_flings_sparks_the_way_the_nozzle_swings() {
        let (mut world, _, _) = world();
        world.ships[0].input.thrust = true;
        // Spinning counterclockwise, but held still so only the swing counts:
        // the nozzle, behind the ship, swings toward -y.
        world.ships[0].spin = 5.0;
        let (mut fires, mut particles) = (Fires::default(), Particles::default());
        run(&mut world, &mut fires, &mut particles, 20, |_| {});
        let sideways: f32 = particles.iter().map(|p| p.vel[1]).sum::<f32>() / particles.len() as f32;
        assert!(sideways < -50.0, "{sideways} px/s");
    }

    #[test]
    fn the_plume_trails_a_turn_and_eases_back_without_overshooting() {
        let (mut world, a, _) = world();
        world.ships[0].input.thrust = true;
        let (mut fires, mut particles) = (Fires::default(), Particles::default());
        run(&mut world, &mut fires, &mut particles, 30, |w| {
            w.ships[0].spin = sim::TURN_MAX;
        });
        let lag = |fires: &Fires| fires.get(a, 0).unwrap().lag;
        assert!((lag(&fires) - sim::TURN_MAX).abs() < 0.5, "trails the turn");
        world.ships[0].spin = 0.0;
        let mut lowest = f32::MAX;
        for _ in 0..60 {
            run(&mut world, &mut fires, &mut particles, 1, |_| {});
            lowest = lowest.min(lag(&fires));
        }
        assert!(lowest > -0.01 * sim::TURN_MAX, "overshot to {lowest}");
        assert!(lag(&fires).abs() < 0.05, "settles: {}", lag(&fires));
    }

    #[test]
    fn fires_of_gone_ships_are_forgotten() {
        let (mut world, a, b) = world();
        let (mut fires, mut particles) = (Fires::default(), Particles::default());
        run(&mut world, &mut fires, &mut particles, 1, |_| {});
        world.ships.remove(1);
        run(&mut world, &mut fires, &mut particles, 1, |_| {});
        assert!(fires.get(a, 0).is_some());
        assert!(fires.get(b, 0).is_none());
    }
}
