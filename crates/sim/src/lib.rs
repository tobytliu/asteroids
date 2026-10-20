//! Pure, deterministic game logic.
//!
//! World units are CSS pixels, so things look and handle the same on any
//! screen. The origin is the map's center, +y is up, and angles are radians
//! with 0 pointing +x. The map's opposite edges join, so positions wrap to
//! [-size/2, size/2) on each axis.

// Per-axis `for i in 0..2` loops read most clearly for 2D vector math.
#![allow(clippy::needless_range_loop)]

mod asteroid;
mod forces;
mod math;
mod rng;
mod ship;

use std::collections::BTreeMap;
use std::f32::consts::TAU;

pub use asteroid::{Asteroid, Class, SHAPE_POINTS, Spin};
pub use ship::{ACCEL, DRAG, SHIP_RADIUS, Ship, TURN};

use asteroid::{Rock, SPAWN_CLEARANCE};
use forces::{Accel, Drift};
use rng::Rng;

pub const DT: f32 = 1.0 / 60.0;
/// The map's width and height in px. Opposite edges join (ADR 0003).
pub const MAP_SIZE: [f32; 2] = [4000.0, 4000.0];
/// Ships join here when it's clear, and the starting rocks leave room around
/// it.
const SPAWN_POINT: [f32; 2] = [0.0, 0.0];
const SHIP_COLLISION_PASSES: usize = 4;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Input {
    pub thrust: bool,
    pub left: bool,
    pub right: bool,
}

/// Names one entity for as long as the world exists. IDs count up from 1 in
/// the order entities appear and are never reused (determinism rule 3), so
/// they're also the order anything order-sensitive happens in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(u64);

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Where something is and how it's moving. Every entity that moves has one.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Body {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    /// Radians counterclockwise from +x. A ship's isn't wrapped, so it can be
    /// many turns either way.
    pub angle: f32,
}

/// Every entity in the game, stepped in fixed ticks.
pub struct World {
    /// The map's width and height in px: `MAP_SIZE` in the game, smaller in
    /// tests.
    size: [f32; 2],
    ecs: hecs::World,
    /// Each entity's hecs handle. Handles never leave the sim (rule 3).
    entities: BTreeMap<EntityId, hecs::Entity>,
    /// The last ID handed out.
    last_id: u64,
    /// The world's only source of randomness (rule 5).
    rng: Rng,
}

impl World {
    /// A field of asteroids laid out from `seed`, leaving room for ships at
    /// the spawn point. There are no ships until `spawn_ship` adds them.
    pub fn new(seed: u64, size: [f32; 2]) -> Self {
        let mut world = Self::empty(seed, size);
        for rock in asteroid::spawn_field(&mut world.rng, size, SPAWN_POINT) {
            world.spawn(rock);
        }
        world
    }

    /// A world with nothing in it.
    fn empty(seed: u64, size: [f32; 2]) -> Self {
        Self {
            size,
            ecs: hecs::World::new(),
            entities: BTreeMap::new(),
            last_id: 0,
            rng: Rng::new(seed),
        }
    }

    /// The map's width and height in px.
    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    /// Add an entity made of `components`, giving it the next ID. Anything
    /// with a `Body` also gets an `Accel`, so forces can act on it.
    fn spawn(&mut self, components: impl hecs::DynamicBundle) -> EntityId {
        self.last_id += 1;
        let id = EntityId(self.last_id);
        let entity = self.ecs.spawn(components);
        self.ecs.insert_one(entity, id).expect("just spawned");
        if self.ecs.satisfies::<&Body>(entity) {
            self.ecs.insert_one(entity, Accel::default()).expect("just spawned");
        }
        self.entities.insert(id, entity);
        id
    }

    /// Add a ship, at rest and facing +x, and return its ID. It goes at the
    /// spawn point if that's clear, otherwise somewhere random that is. Call
    /// between ticks.
    pub fn spawn_ship(&mut self) -> EntityId {
        let mut pos = SPAWN_POINT;
        for _ in 0..100 {
            if self.is_clear_for_ship(pos) {
                break;
            }
            pos = [0, 1].map(|i| self.rng.range(-self.size[i] / 2.0, self.size[i] / 2.0));
        }
        self.spawn((
            Body {
                pos,
                ..Body::default()
            },
            Ship::default(),
        ))
    }

    /// Whether a new ship at `pos` would be well clear of every rock and ship.
    fn is_clear_for_ship(&self, pos: [f32; 2]) -> bool {
        let size = self.size;
        let rocks_clear = self
            .ecs
            .query::<(&Body, &Asteroid)>()
            .iter()
            .all(|(body, rock)| {
                overlap(body.pos, pos, rock.radius + SPAWN_CLEARANCE, size).is_none()
            });
        let ships_clear = self.ecs.query::<(&Body, &Ship)>().iter().all(|(body, _)| {
            overlap(body.pos, pos, 2.0 * SHIP_RADIUS + SPAWN_CLEARANCE, size).is_none()
        });
        rocks_clear && ships_clear
    }

    /// Take ship `id` out of the world. Returns whether there was such a ship.
    /// Call between ticks.
    pub fn remove_ship(&mut self, id: EntityId) -> bool {
        let Some(&entity) = self.entities.get(&id) else {
            return false;
        };
        if !self.ecs.satisfies::<&Ship>(entity) {
            return false;
        }
        self.ecs.despawn(entity).expect("entity is live");
        self.entities.remove(&id);
        true
    }

    /// Advance one tick. Each ship is steered by its entry in `inputs`; a ship
    /// without one coasts as if nothing were pressed, and inputs for ships that
    /// don't exist are ignored.
    ///
    /// The systems run in the order written here (determinism rule 1).
    pub fn step(&mut self, inputs: &BTreeMap<EntityId, Input>) {
        let size = self.size;

        // Ships steer.
        for (id, body, ship) in self.ecs.query_mut::<(&EntityId, &mut Body, &mut Ship)>() {
            ship.input = inputs.get(id).copied().unwrap_or_default();
            ship::steer(body, ship.input);
        }
        // Forces add to each body's `Accel` here, one system each (ADR 0009),
        // and are then applied together.
        for (body, accel) in self.ecs.query_mut::<(&mut Body, &mut Accel)>() {
            forces::integrate(body, accel);
        }
        // Slow rocks pick up speed again.
        for (body, drift) in self.ecs.query_mut::<(&mut Body, &Drift)>() {
            forces::hold_drift_floor(body, *drift);
        }
        // Everything moves, and rocks turn.
        for body in self.ecs.query_mut::<&mut Body>() {
            move_body(body, size);
        }
        for (body, spin) in self.ecs.query_mut::<(&mut Body, &Spin)>() {
            asteroid::turn(body, *spin);
        }

        // Rocks bounce off each other, in ID order: each hit moves the rocks
        // the next one sees.
        let mut rocks: Vec<(EntityId, Rock)> = self
            .ecs
            .query_mut::<(&EntityId, &mut Body, &mut Spin, &Asteroid)>()
            .into_iter()
            .map(|(&id, body, spin, asteroid)| {
                (
                    id,
                    Rock {
                        body,
                        spin,
                        asteroid,
                    },
                )
            })
            .collect();
        rocks.sort_by_key(|(id, _)| *id);
        let mut rocks: Vec<Rock> = rocks.into_iter().map(|(_, rock)| rock).collect();
        asteroid::collide_all(&mut rocks, size);

        // Ships are pushed out of rocks, after the rocks have moved so a rock
        // drifting into a ship pushes it, then bounce off each other. A few
        // passes, so a ship squeezed between things settles between them
        // instead of being pushed out of one and into the other.
        let mut rocks: Vec<(EntityId, Body, Asteroid)> = self
            .ecs
            .query::<(&EntityId, &Body, &Asteroid)>()
            .iter()
            .map(|(&id, &body, &asteroid)| (id, body, asteroid))
            .collect();
        rocks.sort_by_key(|(id, ..)| *id);
        let mut ships: Vec<(EntityId, &mut Body)> = self
            .ecs
            .query_mut::<(&EntityId, &mut Body, &Ship)>()
            .into_iter()
            .map(|(&id, body, _)| (id, body))
            .collect();
        ships.sort_by_key(|(id, _)| *id);
        for _ in 0..SHIP_COLLISION_PASSES {
            for (_, ship) in &mut ships {
                for (_, rock, asteroid) in &rocks {
                    ship::push_out_of_rock(ship, rock, asteroid, size);
                }
            }
            for j in 1..ships.len() {
                let (head, tail) = ships.split_at_mut(j);
                for (_, a) in head {
                    ship::collide_ships(a, tail[0].1, size);
                }
            }
        }
    }

    /// Everything in the world as plain data, in ID order. This is what the
    /// client draws.
    pub fn snapshot(&self) -> Snapshot {
        let mut ships: Vec<ShipState> = self
            .ecs
            .query::<(&EntityId, &Body, &Ship)>()
            .iter()
            .map(|(&id, &body, &ship)| ShipState {
                id,
                body,
                input: ship.input,
            })
            .collect();
        ships.sort_by_key(|s| s.id);
        let mut rocks: Vec<RockState> = self
            .ecs
            .query::<(&EntityId, &Body, &Spin, &Asteroid)>()
            .iter()
            .map(|(&id, &body, &spin, &asteroid)| RockState {
                id,
                body,
                spin,
                asteroid,
            })
            .collect();
        rocks.sort_by_key(|r| r.id);
        Snapshot {
            size: self.size,
            ships,
            rocks,
        }
    }

    /// The checksum of this world's snapshot.
    pub fn checksum(&self) -> u64 {
        self.snapshot().checksum()
    }
}

/// Move a body by its velocity over one tick, wrapping across the map's
/// edges.
pub(crate) fn move_body(body: &mut Body, size: [f32; 2]) {
    for i in 0..2 {
        body.pos[i] = wrap(body.pos[i] + body.vel[i] * DT, size[i]);
    }
}

/// The world at one tick, as plain data: everything in it, each list in ID
/// order.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The map's width and height in px.
    pub size: [f32; 2],
    pub ships: Vec<ShipState>,
    pub rocks: Vec<RockState>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipState {
    pub id: EntityId,
    pub body: Body,
    /// The input it was steered with this tick.
    pub input: Input,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RockState {
    pub id: EntityId,
    pub body: Body,
    pub spin: Spin,
    pub asteroid: Asteroid,
}

impl Snapshot {
    /// Ship `id`, if it's in this snapshot.
    pub fn ship(&self, id: EntityId) -> Option<&ShipState> {
        let i = self.ships.binary_search_by_key(&id, |s| s.id).ok()?;
        Some(&self.ships[i])
    }

    /// A fingerprint of every number the physics uses, from their exact bits.
    /// Two worlds that differ anywhere, even in the last bit of one float,
    /// almost certainly have different checksums. Used to check determinism,
    /// and later to spot clients drifting from the server.
    pub fn checksum(&self) -> u64 {
        let mut h = Checksum::default();
        h.floats(&self.size);
        for ship in &self.ships {
            h.floats(&ship.body.pos);
            h.floats(&ship.body.vel);
            h.floats(&[ship.body.angle]);
        }
        for rock in &self.rocks {
            h.word(rock.asteroid.class as u32);
            h.floats(&rock.body.pos);
            h.floats(&rock.body.vel);
            h.floats(&rock.asteroid.shape);
            h.floats(&[rock.asteroid.radius, rock.body.angle, rock.spin.0]);
            h.word(rock.asteroid.look);
        }
        h.0
    }
}

/// 64-bit FNV-1a over 32-bit words.
struct Checksum(u64);

impl Default for Checksum {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Checksum {
    fn word(&mut self, w: u32) {
        for byte in w.to_le_bytes() {
            self.0 = (self.0 ^ byte as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn floats(&mut self, fs: &[f32]) {
        for f in fs {
            self.word(f.to_bits());
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
    a + wrap(b - a, TAU) * t
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
    /// Out of the way of the rocks in most tests.
    const FAR: [f32; 2] = [0.0, 300.0];

    /// A rock's components.
    type RockParts = (Body, Spin, Asteroid, Drift);

    /// A round rock, not spinning.
    fn rock(pos: [f32; 2], vel: [f32; 2], radius: f32) -> RockParts {
        (
            Body {
                pos,
                vel,
                angle: 0.0,
            },
            Spin(0.0),
            Asteroid::new(Class::Medium, [radius; SHAPE_POINTS]),
            Drift {
                floor: Class::Medium.params().drift_floor,
            },
        )
    }

    /// A world of `SIZE` holding `rocks` (IDs 1, 2, … in order) and then one
    /// ship at `ship_pos`.
    fn world(ship_pos: [f32; 2], rocks: Vec<RockParts>) -> World {
        let mut w = World::empty(0, SIZE);
        for parts in rocks {
            w.spawn(parts);
        }
        w.spawn((
            Body {
                pos: ship_pos,
                ..Body::default()
            },
            Ship::default(),
        ));
        w
    }

    /// Step with every ship steered by `input`.
    fn step(w: &mut World, input: Input) {
        let inputs = w.snapshot().ships.iter().map(|s| (s.id, input)).collect();
        w.step(&inputs);
    }

    fn rocks(w: &World) -> Vec<RockState> {
        w.snapshot().rocks
    }

    /// The first ship's body.
    fn ship(w: &World) -> Body {
        w.snapshot().ships[0].body
    }

    #[test]
    fn new_world_spawns_rocks_clear_of_the_spawn_point_and_each_other() {
        for seed in 0..20 {
            let w = World::new(seed, SIZE);
            let rocks = rocks(&w);
            assert_eq!(rocks.len(), 6);
            assert!(w.snapshot().ships.is_empty());
            let large = rocks.iter().filter(|r| r.asteroid.class == Class::Large);
            assert_eq!(large.count(), 1);
            for (i, a) in rocks.iter().enumerate() {
                let reach = a.asteroid.radius + SHIP_RADIUS;
                assert!(overlap(a.body.pos, SPAWN_POINT, reach, SIZE).is_none());
                for b in &rocks[i + 1..] {
                    let radii = a.asteroid.radius + b.asteroid.radius;
                    assert!(overlap(a.body.pos, b.body.pos, radii, SIZE).is_none());
                }
            }
        }
    }

    #[test]
    fn rock_count_scales_with_map_area() {
        let rocks = rocks(&World::new(7, MAP_SIZE));
        let count = |class| rocks.iter().filter(|r| r.asteroid.class == class).count();
        assert_eq!(count(Class::Large), 14);
        assert_eq!(count(Class::Big), 20);
        assert_eq!(count(Class::Medium), 58);
        assert_eq!(count(Class::Small), 6);
    }

    #[test]
    fn first_ship_spawns_at_the_spawn_point_and_the_next_somewhere_clear() {
        let mut w = World::new(5, SIZE);
        let a = w.spawn_ship();
        let b = w.spawn_ship();
        assert!(a < b);
        let snap = w.snapshot();
        assert_eq!(snap.ship(a).unwrap().body.pos, SPAWN_POINT);
        let b_pos = snap.ship(b).unwrap().body.pos;
        let clearance = 2.0 * SHIP_RADIUS + SPAWN_CLEARANCE;
        assert!(overlap(SPAWN_POINT, b_pos, clearance, SIZE).is_none());
        for r in &snap.rocks {
            let reach = r.asteroid.radius + SHIP_RADIUS;
            assert!(overlap(r.body.pos, b_pos, reach, SIZE).is_none());
        }
    }

    #[test]
    fn removed_ships_are_gone_and_their_ids_never_come_back() {
        let mut w = World::new(5, SIZE);
        let a = w.spawn_ship();
        assert!(w.remove_ship(a));
        assert!(!w.remove_ship(a));
        assert!(w.snapshot().ship(a).is_none());
        // Rocks aren't ships.
        let rock = w.snapshot().rocks[0].id;
        assert!(!w.remove_ship(rock));
        assert!(w.spawn_ship() > a);
    }

    #[test]
    fn each_ship_follows_its_own_input_and_ships_without_one_coast() {
        let mut w = World::new(5, SIZE);
        let (a, b) = (w.spawn_ship(), w.spawn_ship());
        let thrust = Input {
            thrust: true,
            ..Input::default()
        };
        let before = w.snapshot();
        // An input for a ship that doesn't exist is ignored.
        w.step(&BTreeMap::from([(a, thrust), (EntityId(999), thrust)]));
        let after = w.snapshot();
        let a_ship = after.ship(a).unwrap();
        assert!(a_ship.body.vel[0] > 0.0);
        assert_eq!(a_ship.input, thrust);
        let b_ship = after.ship(b).unwrap();
        assert_eq!(b_ship.body, before.ship(b).unwrap().body);
        assert_eq!(b_ship.input, Input::default());
    }

    #[test]
    fn ships_bounce_off_each_other_in_the_world() {
        let mut w = World::empty(0, SIZE);
        let at = |x: f32, vx: f32| Body {
            pos: [x, 0.0],
            vel: [vx, 0.0],
            angle: 0.0,
        };
        let a = w.spawn((at(-100.0, 60.0), Ship::default()));
        let b = w.spawn((at(100.0, -60.0), Ship::default()));
        let mut inputs = BTreeMap::new();
        for _ in 0..180 {
            w.step(&inputs);
            inputs.clear();
        }
        let snap = w.snapshot();
        let (a, b) = (snap.ship(a).unwrap().body, snap.ship(b).unwrap().body);
        assert!(a.vel[0] < 0.0 && b.vel[0] > 0.0, "{a:?} {b:?}");
        let gap = wrapped_offset(a.pos, b.pos, SIZE)[0];
        assert!(gap >= 2.0 * SHIP_RADIUS - 1e-3, "{gap}");
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
        let mut w = world([0.0, 0.0], vec![rock([630.0, 0.0], [30.0, 0.0], 20.0)]);
        for _ in 0..60 {
            step(&mut w, Input::default());
        }
        // 630 + 30 = 660, which wraps past the right edge (640) to -620.
        assert!((rocks(&w)[0].body.pos[0] + 620.0).abs() < 0.01);
    }

    #[test]
    fn equal_rocks_bounce_head_on() {
        let mut w = world(
            FAR,
            vec![
                rock([-100.0, 0.0], [40.0, 0.0], 30.0),
                rock([100.0, 0.0], [-40.0, 0.0], 30.0),
            ],
        );
        for _ in 0..180 {
            step(&mut w, Input::default());
        }
        let r = rocks(&w);
        // Equal masses in a perfectly elastic head-on hit swap velocities.
        assert!((r[0].body.vel[0] + 40.0).abs() < 0.01);
        assert!((r[1].body.vel[0] - 40.0).abs() < 0.01);
        let gap = r[1].body.pos[0] - r[0].body.pos[0];
        assert!(gap >= 60.0);
    }

    /// Linear momentum (m v) and angular momentum about the origin
    /// (m (x vy - y vx) + I ω).
    fn momenta(rocks: &[RockParts]) -> ([f32; 2], f32) {
        rocks
            .iter()
            .fold(([0.0; 2], 0.0), |(p, l), (body, spin, asteroid, _)| {
                let m = asteroid.mass();
                let (pos, vel) = (body.pos, body.vel);
                (
                    [p[0] + m * vel[0], p[1] + m * vel[1]],
                    l + m * (pos[0] * vel[1] - pos[1] * vel[0]) + asteroid.inertia() * spin.0,
                )
            })
    }

    #[test]
    fn rock_collisions_conserve_momentum_and_angular_momentum() {
        // Just touching, off-center and spinning, so friction comes into play.
        let mut big = rock([-30.0, -200.0], [25.0, 0.0], 40.0);
        big.1 = Spin(-1.0);
        let mut parts = vec![big, rock([29.0, -190.0], [-20.0, 5.0], 20.0)];
        let (p0, l0) = momenta(&parts);
        let mut rocks: Vec<Rock> = parts
            .iter_mut()
            .map(|(body, spin, asteroid, _)| Rock {
                body,
                spin,
                asteroid,
            })
            .collect();
        asteroid::collide_all(&mut rocks, SIZE);
        let (p1, l1) = momenta(&parts);
        assert!(parts[1].0.vel[0] > 0.0, "small rock should rebound");
        assert!(parts[1].1.0.abs() > 0.05, "friction should spin it");
        for i in 0..2 {
            assert!((p0[i] - p1[i]).abs() < 1.0, "{p0:?} vs {p1:?}");
        }
        assert!((l1 - l0).abs() < 0.005 * l0.abs(), "{l0} vs {l1}");
    }

    #[test]
    fn big_rock_cannot_fling_a_small_one_too_fast() {
        let mut w = world(
            FAR,
            vec![
                rock([-100.0, 0.0], [45.0, 0.0], 48.0),
                rock([100.0, 0.0], [-45.0, 0.0], 18.0),
            ],
        );
        for _ in 0..240 {
            step(&mut w, Input::default());
        }
        let small = rocks(&w)[1].body.vel;
        assert!(small[0] > 0.0, "small rock should rebound");
        assert!(math::hypot(small[0], small[1]) <= asteroid::SPEED_LIMIT + 1e-3);
    }

    #[test]
    fn glancing_hit_spins_both_rocks() {
        // A passes just under B (moving the other way), rubbing B's underside
        // to the right and its own top to the left: both turn counterclockwise.
        let mut w = world(
            FAR,
            vec![
                rock([-100.0, -100.0], [40.0, 0.0], 30.0),
                rock([100.0, -60.0], [-15.0, 0.0], 30.0),
            ],
        );
        for _ in 0..240 {
            step(&mut w, Input::default());
        }
        let r = rocks(&w);
        assert!(r[0].spin.0 > 0.05, "{:?}", r[0].spin);
        assert!(r[1].spin.0 > 0.05, "{:?}", r[1].spin);
    }

    #[test]
    fn spinning_rock_kicks_another_sideways_and_counter_spins_it() {
        let mut spinner = rock([-100.0, 0.0], [40.0, 0.0], 30.0);
        spinner.1 = Spin(2.0);
        let mut w = world(FAR, vec![spinner, rock([100.0, 0.0], [-15.0, 0.0], 30.0)]);
        for _ in 0..240 {
            step(&mut w, Input::default());
        }
        let r = rocks(&w);
        let (a, b) = (r[0], r[1]);
        // Like meshing gears: A's right side moves up, dragging B's left side
        // up, so B turns clockwise and heads up as well as away.
        assert!(b.spin.0 < -0.05, "{:?}", b.spin);
        assert!(b.body.vel[1] > 0.5, "{:?}", b.body.vel);
        assert!(a.spin.0 < 2.0);
    }

    #[test]
    fn spin_is_capped() {
        let mut spinner = rock([-100.0, 0.0], [45.0, 0.0], 48.0);
        spinner.1 = Spin(3.0);
        let mut w = world(FAR, vec![spinner, rock([100.0, 0.0], [-45.0, 0.0], 18.0)]);
        for _ in 0..240 {
            step(&mut w, Input::default());
        }
        for r in rocks(&w) {
            assert!(r.spin.0.abs() <= asteroid::SPIN_LIMIT + 1e-6);
        }
    }

    #[test]
    fn rocks_collide_across_the_wrapped_edge() {
        let mut w = world(
            FAR,
            vec![
                rock([600.0, 0.0], [40.0, 0.0], 20.0),
                rock([-600.0, 0.0], [-40.0, 0.0], 20.0),
            ],
        );
        for _ in 0..120 {
            step(&mut w, Input::default());
        }
        let r = rocks(&w);
        assert!(r[0].body.vel[0] < 0.0);
        assert!(r[1].body.vel[0] > 0.0);
    }

    #[test]
    fn ship_cannot_fly_through_a_rock_and_does_not_move_it() {
        let rock = || vec![rock([150.0, 0.0], [-15.0, 0.0], 30.0)];
        let mut w = world([0.0, 0.0], rock());
        let mut without_ship = world(FAR, rock());
        let thrust = Input {
            thrust: true,
            ..Input::default()
        };
        for _ in 0..300 {
            step(&mut w, thrust);
            step(&mut without_ship, Input::default());
            assert_eq!(rocks(&w), rocks(&without_ship));
            let gap = rocks(&w)[0].body.pos[0] - ship(&w).pos[0];
            assert!(gap >= 30.0 + SHIP_RADIUS - 1e-3, "{gap}");
        }
    }

    #[test]
    fn slowed_rocks_ease_back_up_to_a_drift() {
        let mut w = world(FAR, vec![rock([0.0, 0.0], [2.0, 0.0], 30.0)]);
        for _ in 0..600 {
            step(&mut w, Input::default());
        }
        let v = rocks(&w)[0].body.vel;
        assert!((v[0] - 15.0).abs() < 1e-3 && v[1] == 0.0, "{v:?}");
    }

    #[test]
    fn drifting_rock_pushes_the_ship() {
        let mut w = world([0.0, 0.0], vec![rock([-100.0, 0.0], [60.0, 0.0], 30.0)]);
        for _ in 0..120 {
            step(&mut w, Input::default());
        }
        let rock = rocks(&w)[0].body;
        assert_eq!(rock.vel, [60.0, 0.0]);
        assert!(ship(&w).pos[0] - rock.pos[0] >= 30.0 + SHIP_RADIUS - 1e-3);
    }

    /// Thrusting and turning in an irregular pattern, so the ship gets around
    /// and runs into rocks.
    fn scripted_input(tick: u32) -> Input {
        Input {
            thrust: tick % 5 < 3,
            left: tick % 13 < 4,
            right: tick % 17 < 3,
        }
    }

    /// Every tick's checksum over a minute of play on a small, crowded map.
    fn scripted_checksums() -> Vec<u64> {
        let mut w = World::new(42, SIZE);
        let ship = w.spawn_ship();
        (0..3600)
            .map(|tick| {
                w.step(&BTreeMap::from([(ship, scripted_input(tick))]));
                w.checksum()
            })
            .collect()
    }

    #[test]
    fn same_seed_and_inputs_give_identical_checksums_every_tick() {
        assert_eq!(scripted_checksums(), scripted_checksums());
    }

    /// Checksums after 1 tick, 1 second, 10 seconds and a minute of the
    /// scripted run, from a known-good build. They must come out the same on
    /// every platform (ADR 0002), and refactors must keep them. Only re-pin
    /// them for a change that's meant to change what the sim does.
    const GOLDEN: [u64; 4] = [
        0x274b_9a50_df1c_e40d,
        0xede9_f8af_6da9_80a6,
        0x7189_5eb4_bee3_dbd9,
        0x5f64_8445_8180_efe5,
    ];

    #[test]
    fn checksums_match_the_golden_record() {
        let c = scripted_checksums();
        assert_eq!(
            [c[0], c[59], c[599], c[3599]],
            GOLDEN,
            "{:#x?}",
            [c[0], c[59], c[599], c[3599]]
        );
    }
}
