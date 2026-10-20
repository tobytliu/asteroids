//! Each ship's engine flame, which is purely visual: it eases toward the
//! ship's thrust input and bends with how fast the ship is turning. The sim
//! knows nothing about it.

use std::collections::BTreeMap;

use sim::{EntityId, Snapshot};

/// How fast a flame flares up when thrust starts and dies down when it stops,
/// per second.
const IGNITE_RATE: f32 = 14.0;
const FADE_RATE: f32 = 4.0;
/// How fast a flame's bend follows the ship's turn rate, per second: about how
/// quickly old exhaust flushes out of the plume.
const BEND_RATE: f32 = 8.0;

/// One ship's flame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flame {
    /// 0 (off) to 1 (full), eased toward the thrust input.
    pub intensity: f32,
    /// Turn rate in rad/s, eased so the flame bends and straightens smoothly.
    pub turn: f32,
    /// The ship's angle last frame, to measure how fast it's turning.
    last_angle: f32,
}

/// Every ship's flame, by ship.
#[derive(Default)]
pub struct Flames(BTreeMap<EntityId, Flame>);

impl Flames {
    /// Ease every flame toward its ship's state in `world`, `dt` seconds after
    /// the last update. Ships that have gone are forgotten.
    pub fn update(&mut self, world: &Snapshot, dt: f32) {
        self.0.retain(|id, _| world.ship(*id).is_some());
        for ship in &world.ships {
            let angle = ship.body.angle;
            let flame = self.0.entry(ship.id).or_insert(Flame {
                last_angle: angle,
                ..Flame::default()
            });
            let (target, rate) = if ship.input.thrust {
                (1.0, IGNITE_RATE)
            } else {
                (0.0, FADE_RATE)
            };
            flame.intensity += (target - flame.intensity) * ease(rate, dt);
            if dt > 0.0 {
                let turn_rate = (angle - flame.last_angle) / dt;
                flame.turn += (turn_rate - flame.turn) * ease(BEND_RATE, dt);
            }
            flame.last_angle = angle;
        }
    }

    /// Ship `id`'s flame; unlit if it has none yet.
    pub fn get(&self, id: EntityId) -> Flame {
        self.0.get(&id).copied().unwrap_or_default()
    }
}

/// How much of the way to its target something easing at `rate` per second
/// gets in `dt` seconds.
fn ease(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_two_ships() -> (Snapshot, EntityId, EntityId) {
        let mut world = sim::World::new(1, [1000.0, 1000.0]);
        let (a, b) = (world.spawn_ship(), world.spawn_ship());
        (world.snapshot(), a, b)
    }

    #[test]
    fn each_ship_has_its_own_flame_from_its_own_input() {
        let (mut world, a, b) = world_with_two_ships();
        world.ships[0].input.thrust = true;
        let mut flames = Flames::default();
        for _ in 0..30 {
            flames.update(&world, 1.0 / 60.0);
        }
        assert!(flames.get(a).intensity > 0.9);
        assert_eq!(flames.get(b).intensity, 0.0);
    }

    #[test]
    fn turning_bends_the_flame_and_gone_ships_are_forgotten() {
        let (mut world, a, b) = world_with_two_ships();
        let mut flames = Flames::default();
        flames.update(&world, 1.0 / 60.0);
        world.ships[1].body.angle += 0.1;
        flames.update(&world, 1.0 / 60.0);
        assert!(flames.get(b).turn > 0.0);
        assert_eq!(flames.get(a).turn, 0.0);
        world.ships.remove(1);
        flames.update(&world, 1.0 / 60.0);
        assert_eq!(flames.get(b), Flame::default());
    }
}
