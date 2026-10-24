//! Guns and the bolts they shoot (ADR 0013). A ship's `Gun` turns its
//! trigger into shots; each shot is a `Bolt` entity that flies straight until
//! it hits something or burns out. What a weapon does is all in its
//! `WeaponKind::params`, so a new weapon is a new kind and its params.

use crate::{Body, DT, EntityId, math, wrap};

/// Which weapon a gun is, and so what its bolts are like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeaponKind {
    /// Red bolts, one per press.
    Laser,
}

/// How holding the trigger shoots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// One shot per press: let go to shoot again.
    Single,
    /// Shoots again every cooldown while the trigger is held.
    Auto,
}

/// What a weapon is like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub trigger: Trigger,
    /// Ticks between shots, at the fastest.
    pub cooldown: u32,
    /// How fast a bolt leaves the muzzle, in px/s, on top of the ship's own
    /// velocity.
    pub muzzle_speed: f32,
    /// How many ticks a bolt lasts before it burns out.
    pub lifetime: u32,
    /// How far a bolt reaches from its center when it hits things, in px.
    pub radius: f32,
}

impl WeaponKind {
    pub const fn params(self) -> Params {
        match self {
            WeaponKind::Laser => Params {
                trigger: Trigger::Single,
                cooldown: 8,
                muzzle_speed: 1200.0,
                lifetime: 60,
                radius: 2.0,
            },
        }
    }
}

/// A ship's gun.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gun {
    pub kind: WeaponKind,
    /// Ticks until it can shoot again; 0 when it's ready.
    pub cooldown: u32,
    /// Whether the trigger was held last tick, so a press can be told from a
    /// hold.
    held: bool,
}

impl Gun {
    pub fn new(kind: WeaponKind) -> Self {
        Self {
            kind,
            cooldown: 0,
            held: false,
        }
    }

    /// Whether the gun shoots this tick, with its trigger `held` or not. Call
    /// once a tick. A press while it's cooling down is lost.
    pub(crate) fn pull(&mut self, held: bool) -> bool {
        let params = self.kind.params();
        let pressed = held && !self.held;
        self.held = held;
        self.cooldown = self.cooldown.saturating_sub(1);
        let wants = match params.trigger {
            Trigger::Single => pressed,
            Trigger::Auto => held,
        };
        if wants && self.cooldown == 0 {
            self.cooldown = params.cooldown;
            true
        } else {
            false
        }
    }
}

/// What makes an entity a bolt: one shot from a gun. Where it is and how it
/// moves are in its `Body`; it flies straight and nothing pushes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bolt {
    pub kind: WeaponKind,
    /// The ship that shot it, which it never hits.
    pub owner: EntityId,
    /// Ticks until it burns out.
    pub ticks_left: u32,
}

/// Where a bolt hit something this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Where on the map it touched, in px.
    pub pos: [f32; 2],
    /// The bolt's velocity as it hit, in px/s.
    pub vel: [f32; 2],
    pub kind: WeaponKind,
}

/// A new bolt shot from a ship with body `ship`, starting `muzzle` px ahead
/// of its center on a map of `size`. It flies the way it's moving, which is
/// the ship's velocity plus the muzzle speed along its heading.
pub(crate) fn shoot(
    ship: &Body,
    owner: EntityId,
    kind: WeaponKind,
    muzzle: f32,
    size: [f32; 2],
) -> (Body, Bolt) {
    let params = kind.params();
    let (sin, cos) = math::sin_cos(ship.angle);
    let dir = [cos, sin];
    let vel = [0, 1].map(|i| ship.vel[i] + dir[i] * params.muzzle_speed);
    let body = Body {
        pos: [0, 1].map(|i| wrap(ship.pos[i] + dir[i] * muzzle, size[i])),
        vel,
        angle: math::atan2(vel[1], vel[0]),
    };
    let bolt = Bolt {
        kind,
        owner,
        ticks_left: params.lifetime,
    };
    (body, bolt)
}

/// Points this far apart, at most, are checked along a bolt's path, in px.
/// Finer than anything a bolt could slip between.
const SAMPLE: f32 = 2.0;

/// How far along its path this tick (0 at the start, 1 at the end) a bolt
/// first touches something, if it does. `start` is where the bolt started
/// relative to the thing's center, `path` how far it moved, `reach` the
/// farthest from the center it could touch, and `touches` whether it touches
/// at an offset from the center.
pub(crate) fn first_touch(
    start: [f32; 2],
    path: [f32; 2],
    reach: f32,
    touches: impl Fn([f32; 2]) -> bool,
) -> Option<f32> {
    // Skip anything the path never comes within reach of.
    let len_sq = path[0] * path[0] + path[1] * path[1];
    let closest = if len_sq > 0.0 {
        (-(start[0] * path[0] + start[1] * path[1]) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let near = [0, 1].map(|i| start[i] + path[i] * closest);
    if near[0] * near[0] + near[1] * near[1] > reach * reach {
        return None;
    }
    let samples = ((len_sq.sqrt() / SAMPLE).ceil() as usize).max(1);
    (0..=samples)
        .map(|i| i as f32 / samples as f32)
        .find(|&t| touches([0, 1].map(|i| start[i] + path[i] * t)))
}

/// How far a bolt moves in one tick.
pub(crate) fn path(body: &Body) -> [f32; 2] {
    body.vel.map(|v| v * DT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_trigger_shoots_once_per_press() {
        let mut gun = Gun::new(WeaponKind::Laser);
        assert!(gun.pull(true));
        // Holding it down doesn't shoot again, however long.
        assert!((0..100).all(|_| !gun.pull(true)));
        assert!(!gun.pull(false));
        assert!(gun.pull(true));
    }

    #[test]
    fn a_press_while_cooling_down_is_lost() {
        let cooldown = WeaponKind::Laser.params().cooldown;
        let mut gun = Gun::new(WeaponKind::Laser);
        assert!(gun.pull(true));
        // Released and pressed again too soon.
        assert!(!gun.pull(false));
        assert!(!gun.pull(true));
        // Still held when it's ready: no shot, since it's not a new press.
        assert!((0..cooldown).all(|_| !gun.pull(true)));
        // Released and pressed once it's ready.
        assert!(!gun.pull(false));
        assert!(gun.pull(true));
    }

    #[test]
    fn presses_shoot_at_most_once_a_cooldown() {
        let cooldown = WeaponKind::Laser.params().cooldown;
        let mut gun = Gun::new(WeaponKind::Laser);
        // Pressed every other tick for a long time.
        let shots: Vec<u32> = (0..200).filter(|&t| gun.pull(t % 2 == 0)).collect();
        assert!(
            shots.windows(2).all(|w| w[1] - w[0] >= cooldown),
            "{shots:?}"
        );
        assert!(shots.len() > 200 / cooldown as usize / 2);
    }

    #[test]
    fn bolts_leave_the_nose_with_the_ship_velocity_added() {
        let ship = Body {
            pos: [0.0, 0.0],
            vel: [0.0, 300.0],
            angle: 0.0,
        };
        let owner = EntityId(1);
        let (body, bolt) = shoot(&ship, owner, WeaponKind::Laser, 30.0, [1000.0, 1000.0]);
        let speed = WeaponKind::Laser.params().muzzle_speed;
        assert_eq!(body.pos, [30.0, 0.0]);
        assert_eq!(body.vel, [speed, 300.0]);
        assert!(body.angle > 0.0, "points the way it flies");
        assert_eq!(bolt.owner, owner);
    }

    fn in_circle(r: f32) -> impl Fn([f32; 2]) -> bool {
        move |p| p[0] * p[0] + p[1] * p[1] <= r * r
    }

    #[test]
    fn a_path_through_a_circle_touches_where_it_enters() {
        let t = first_touch([-50.0, 0.0], [100.0, 0.0], 10.0, in_circle(10.0)).unwrap();
        assert!((t - 0.4).abs() <= SAMPLE / 100.0, "{t}");
    }

    #[test]
    fn a_path_that_misses_touches_nothing() {
        assert!(first_touch([-50.0, 11.0], [100.0, 0.0], 10.0, in_circle(10.0)).is_none());
        // Stopping short.
        assert!(first_touch([-50.0, 0.0], [30.0, 0.0], 10.0, in_circle(10.0)).is_none());
    }

    #[test]
    fn a_path_starting_inside_touches_at_once() {
        assert_eq!(
            first_touch([0.0, 0.0], [5.0, 0.0], 10.0, in_circle(10.0)),
            Some(0.0)
        );
    }
}
