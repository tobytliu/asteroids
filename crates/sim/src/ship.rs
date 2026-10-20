use crate::asteroid::{Contact, contact};
use crate::{Asteroid, Body, DT, Input, math, overlap, wrap};

pub const TURN: f32 = 4.0; // rad/s
pub const ACCEL: f32 = 500.0; // px/s²
pub const DRAG: f32 = 0.995; // per tick
/// A ship collides as a circle this big, in px.
pub const SHIP_RADIUS: f32 = 20.0;

/// What makes an entity a ship. Where it is and how it moves are in its
/// `Body`.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ship {
    /// The input it was steered with this tick. Kept because it's part of how
    /// the ship looks (its engine flame) to everyone, not just its pilot.
    pub input: Input,
}

/// Turn, thrust and slow a ship for one tick. Moving it is left to the
/// world's movement system.
pub(crate) fn steer(body: &mut Body, input: Input) {
    if input.left {
        body.angle += TURN * DT;
    }
    if input.right {
        body.angle -= TURN * DT;
    }
    if input.thrust {
        body.vel[0] += math::cos(body.angle) * ACCEL * DT;
        body.vel[1] += math::sin(body.angle) * ACCEL * DT;
    }
    for i in 0..2 {
        body.vel[i] *= DRAG;
    }
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
    fn fly(body: &mut Body, input: Input) {
        steer(body, input);
        move_body(body, SIZE);
    }

    #[test]
    fn stationary_ship_stays_put_without_input() {
        let mut ship = Body::default();
        for _ in 0..60 {
            fly(&mut ship, Input::default());
        }
        assert_eq!(ship, Body::default());
    }

    #[test]
    fn thrust_at_angle_zero_moves_along_x() {
        let mut ship = Body::default();
        let input = Input {
            thrust: true,
            ..Input::default()
        };
        for _ in 0..60 {
            fly(&mut ship, input);
        }
        assert!(ship.pos[0] > 0.0);
        assert!(ship.pos[1].abs() < 1e-3);
    }

    #[test]
    fn left_for_one_second_turns_by_turn_rate() {
        let mut ship = Body::default();
        let input = Input {
            left: true,
            ..Input::default()
        };
        for _ in 0..60 {
            fly(&mut ship, input);
        }
        assert!((ship.angle - TURN).abs() < 1e-3);
    }

    #[test]
    fn ship_wraps_at_right_edge() {
        let mut ship = Body {
            pos: [399.0, 0.0],
            vel: [600.0, 0.0],
            angle: 0.0,
        };
        fly(&mut ship, Input::default());
        assert!(ship.pos[0] < -390.0, "pos[0] = {}", ship.pos[0]);
        assert!(ship.pos[0] >= -400.0);
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
