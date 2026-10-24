use crate::asteroid::{Contact, contact};
use crate::{Asteroid, Body, DT, Input, math, overlap, wrap};

/// How fast a held turn spins a ship up, in rad/s², and the fastest it
/// spins, in rad/s.
pub const TURN_ACCEL: f32 = 30.0;
pub const TURN_MAX: f32 = 7.0;
/// Turning against the way a ship is spinning brakes this many times harder,
/// so it can whip around and switch direction sharply.
pub const REVERSE_BOOST: f32 = 2.5;
/// How hard a ship's spin is braked when no turn is held, in rad/s²: from full
/// spin to a stop in a tenth of a second.
pub const TURN_BRAKE: f32 = 70.0;
pub const ACCEL: f32 = 500.0; // px/s²
pub const DRAG: f32 = 0.995; // per tick
/// The fastest a ship can fly under its own thrust, in px/s.
pub const MAX_SPEED: f32 = 600.0;
/// A ship collides as a circle this big, in px.
pub const SHIP_RADIUS: f32 = 20.0;

/// What makes an entity a ship. Where it is and how it moves are in its
/// `Body`.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ship {
    /// The input it was steered with this tick. Kept because it's part of how
    /// the ship looks (its engine flame) to everyone, not just its pilot.
    pub input: Input,
    /// How fast it's turning, in rad/s, counterclockwise.
    pub spin: f32,
}

/// Turn, thrust and slow a ship for one tick. Moving it is left to the
/// world's movement system.
pub(crate) fn steer(body: &mut Body, ship: &mut Ship) {
    let input = ship.input;
    ship.spin = spin_up(ship.spin, input);
    body.angle += ship.spin * DT;
    if input.thrust {
        body.vel[0] += math::cos(body.angle) * ACCEL * DT;
        body.vel[1] += math::sin(body.angle) * ACCEL * DT;
    }
    for i in 0..2 {
        body.vel[i] *= DRAG;
    }
    let speed = math::hypot(body.vel[0], body.vel[1]);
    if speed > MAX_SPEED {
        body.vel = body.vel.map(|v| v * MAX_SPEED / speed);
    }
}

/// A ship's spin after one tick of `input`. A held turn spins it up, harder
/// if it's spinning the other way; with no turn held (or both) it brakes to a
/// stop.
fn spin_up(spin: f32, input: Input) -> f32 {
    let want = match (input.left, input.right) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    };
    if want == 0.0 {
        let brake = TURN_BRAKE * DT;
        return if spin.abs() <= brake {
            0.0
        } else {
            spin - brake * spin.signum()
        };
    }
    let accel = if spin * want < 0.0 {
        TURN_ACCEL * REVERSE_BOOST
    } else {
        TURN_ACCEL
    };
    (spin + want * accel * DT).clamp(-TURN_MAX, TURN_MAX)
}

/// If a ship is touching a rock's outline, push the ship out of it and cancel
/// the part of its velocity (relative to the rock) heading into it, so it
/// slides along the surface. The rock is unaffected.
pub(crate) fn push_out_of_rock(ship: &mut Body, rock: &Body, asteroid: &Asteroid, size: [f32; 2]) {
    let Some(Contact { normal, depth, .. }) = contact(
        rock.pos,
        asteroid.radius,
        |dir| asteroid.surface(rock.angle, dir),
        ship.pos,
        SHIP_RADIUS,
        |_| SHIP_RADIUS,
        size,
    ) else {
        return;
    };
    for i in 0..2 {
        ship.pos[i] = wrap(ship.pos[i] + normal[i] * depth, size[i]);
    }
    let into = (ship.vel[0] - rock.vel[0]) * normal[0] + (ship.vel[1] - rock.vel[1]) * normal[1];
    if into < 0.0 {
        for i in 0..2 {
            ship.vel[i] -= into * normal[i];
        }
    }
}

/// If two ships overlap, push them apart equally and bounce them off each
/// other: equal masses in a perfectly elastic hit swap the parts of their
/// velocities along the line between them.
pub(crate) fn collide_ships(a: &mut Body, b: &mut Body, size: [f32; 2]) {
    let Some((n, depth)) = overlap(a.pos, b.pos, 2.0 * SHIP_RADIUS, size) else {
        return;
    };
    for i in 0..2 {
        a.pos[i] = wrap(a.pos[i] - n[i] * depth / 2.0, size[i]);
        b.pos[i] = wrap(b.pos[i] + n[i] * depth / 2.0, size[i]);
    }
    let closing = (b.vel[0] - a.vel[0]) * n[0] + (b.vel[1] - a.vel[1]) * n[1];
    if closing < 0.0 {
        for i in 0..2 {
            a.vel[i] += closing * n[i];
            b.vel[i] -= closing * n[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Class, SHAPE_POINTS, move_body};

    const SIZE: [f32; 2] = [800.0, 600.0];

    /// Steer and move a ship for one tick.
    fn fly(body: &mut Body, ship: &mut Ship, input: Input) {
        ship.input = input;
        steer(body, ship);
        move_body(body, SIZE);
    }

    const LEFT: Input = Input {
        thrust: false,
        left: true,
        right: false,
        shoot: false,
    };
    const RIGHT: Input = Input {
        thrust: false,
        left: false,
        right: true,
        shoot: false,
    };

    #[test]
    fn stationary_ship_stays_put_without_input() {
        let (mut body, mut ship) = (Body::default(), Ship::default());
        for _ in 0..60 {
            fly(&mut body, &mut ship, Input::default());
        }
        assert_eq!(body, Body::default());
    }

    #[test]
    fn thrust_at_angle_zero_moves_along_x() {
        let (mut body, mut ship) = (Body::default(), Ship::default());
        let input = Input {
            thrust: true,
            ..Input::default()
        };
        for _ in 0..60 {
            fly(&mut body, &mut ship, input);
        }
        assert!(body.pos[0] > 0.0);
        assert!(body.pos[1].abs() < 1e-3);
    }

    #[test]
    fn thrust_tops_out_at_max_speed() {
        let (mut body, mut ship) = (Body::default(), Ship::default());
        let input = Input {
            thrust: true,
            ..Input::default()
        };
        for _ in 0..600 {
            fly(&mut body, &mut ship, input);
        }
        let speed = math::hypot(body.vel[0], body.vel[1]);
        assert!((speed - MAX_SPEED).abs() < 1e-2, "{speed}");
    }

    #[test]
    fn a_held_turn_spins_up_to_the_cap() {
        let mut ship = Ship::default();
        ship.spin = spin_up(ship.spin, LEFT);
        assert!((ship.spin - TURN_ACCEL * DT).abs() < 1e-6, "builds gradually");
        for _ in 0..60 {
            ship.spin = spin_up(ship.spin, LEFT);
        }
        assert_eq!(ship.spin, TURN_MAX);
    }

    #[test]
    fn letting_go_brakes_to_a_stop_within_a_tenth_of_a_second() {
        let mut spin = -TURN_MAX;
        for _ in 0..6 {
            spin = spin_up(spin, Input::default());
        }
        assert_eq!(spin, 0.0);
        let both = Input {
            left: true,
            right: true,
            ..Input::default()
        };
        assert!(spin_up(TURN_MAX, both) < TURN_MAX, "both held brakes too");
    }

    #[test]
    fn reversing_gets_through_zero_faster_than_spinning_up() {
        let ticks_to = |mut spin: f32, input: Input, done: fn(f32) -> bool| {
            let mut n = 0;
            while !done(spin) {
                spin = spin_up(spin, input);
                n += 1;
            }
            n
        };
        let reverse = ticks_to(TURN_MAX, RIGHT, |s| s <= 0.0);
        let spin_up_from_rest = ticks_to(0.0, LEFT, |s| s >= TURN_MAX);
        assert!(reverse * 2 < spin_up_from_rest, "{reverse} vs {spin_up_from_rest}");
    }

    #[test]
    fn a_held_turn_whips_a_ship_around_in_well_under_a_second() {
        let (mut body, mut ship) = (Body::default(), Ship::default());
        let mut ticks = 0;
        while body.angle < std::f32::consts::PI {
            fly(&mut body, &mut ship, LEFT);
            ticks += 1;
        }
        assert!(ticks < 40, "{ticks} ticks for a half turn");
    }

    #[test]
    fn ship_wraps_at_right_edge() {
        let mut body = Body {
            pos: [399.0, 0.0],
            vel: [600.0, 0.0],
            angle: 0.0,
        };
        fly(&mut body, &mut Ship::default(), Input::default());
        assert!(body.pos[0] < -390.0, "pos[0] = {}", body.pos[0]);
        assert!(body.pos[0] >= -400.0);
    }

    #[test]
    fn ship_slides_along_a_rock() {
        let rock = Asteroid::new(Class::Medium, [30.0; SHAPE_POINTS]);
        // Just touching the top of the rock, moving right and down into it.
        let mut ship = Body {
            pos: [0.0, 30.0 + SHIP_RADIUS - 1.0],
            vel: [40.0, -40.0],
            angle: 0.0,
        };
        push_out_of_rock(&mut ship, &Body::default(), &rock, SIZE);
        assert!((ship.pos[1] - (30.0 + SHIP_RADIUS)).abs() < 1e-3);
        assert!((ship.vel[0] - 40.0).abs() < 1e-3, "keeps sliding sideways");
        assert!(ship.vel[1].abs() < 1e-3, "stops moving into the rock");
    }

    #[test]
    fn ships_bounce_off_each_other() {
        // Overlapping by 4 px, closing head on, B also sliding sideways.
        let mut a = Body {
            pos: [-18.0, 0.0],
            vel: [50.0, 0.0],
            angle: 0.0,
        };
        let mut b = Body {
            pos: [18.0, 0.0],
            vel: [-10.0, 7.0],
            angle: 0.0,
        };
        collide_ships(&mut a, &mut b, SIZE);
        assert!((a.pos[0] + 20.0).abs() < 1e-4 && (b.pos[0] - 20.0).abs() < 1e-4);
        // Swapped along the line between them, unchanged across it.
        assert_eq!(a.vel, [-10.0, 0.0]);
        assert_eq!(b.vel, [50.0, 7.0]);
    }

    #[test]
    fn ships_moving_apart_are_only_separated() {
        let mut a = Body {
            pos: [-18.0, 0.0],
            vel: [-5.0, 0.0],
            angle: 0.0,
        };
        let mut b = Body {
            pos: [18.0, 0.0],
            vel: [5.0, 0.0],
            angle: 0.0,
        };
        collide_ships(&mut a, &mut b, SIZE);
        assert_eq!((a.vel, b.vel), ([-5.0, 0.0], [5.0, 0.0]));
    }
}

#[cfg(test)]
mod outline_tests {
    use std::f32::consts::{FRAC_PI_2, TAU};

    use super::*;
    use crate::{Class, SHAPE_POINTS};

    const SIZE: [f32; 2] = [800.0, 600.0];

    /// 60 px from the center along the rock's heading (and opposite), 25 px
    /// across it.
    fn long_rock() -> Asteroid {
        let shape = std::array::from_fn(|i| {
            let theta = i as f32 / SHAPE_POINTS as f32 * TAU;
            let (x, y) = (math::cos(theta) / 60.0, math::sin(theta) / 25.0);
            1.0 / (x * x + y * y).sqrt()
        });
        Asteroid::new(Class::Medium, shape)
    }

    fn turned(angle: f32) -> Body {
        Body {
            angle,
            ..Body::default()
        }
    }

    fn ship_at(pos: [f32; 2]) -> Body {
        Body {
            pos,
            ..Body::default()
        }
    }

    #[test]
    fn ship_touches_the_outline_not_a_bounding_circle() {
        let rock = long_rock();
        // Beside the narrow middle: inside the bounding circle, outside the rock.
        let mut beside = ship_at([0.0, 50.0]);
        push_out_of_rock(&mut beside, &turned(0.0), &rock, SIZE);
        assert_eq!(beside.pos, [0.0, 50.0]);
        // Off the long end: pushed out to the tip.
        let mut end = ship_at([70.0, 0.0]);
        push_out_of_rock(&mut end, &turned(0.0), &rock, SIZE);
        assert!((end.pos[0] - (60.0 + SHIP_RADIUS)).abs() < 1e-3);
    }

    #[test]
    fn outline_turns_with_the_rock() {
        let mut above = ship_at([0.0, 50.0]);
        push_out_of_rock(&mut above, &turned(FRAC_PI_2), &long_rock(), SIZE);
        assert!((above.pos[1] - (60.0 + SHIP_RADIUS)).abs() < 1e-3);
    }
}
