use crate::asteroid::{Contact, contact};
use crate::{Asteroid, DT, Input, wrap};

pub const TURN: f32 = 4.0; // rad/s
pub const ACCEL: f32 = 500.0; // px/s²
pub const DRAG: f32 = 0.995; // per tick
/// The ship collides as a circle this big, in px.
pub const SHIP_RADIUS: f32 = 20.0;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ship {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub angle: f32,
}

impl Ship {
    /// Advance one tick in a world of `size` (width, height) px.
    pub fn step(&mut self, input: Input, size: [f32; 2]) {
        if input.left {
            self.angle += TURN * DT;
        }
        if input.right {
            self.angle -= TURN * DT;
        }
        if input.thrust {
            self.vel[0] += self.angle.cos() * ACCEL * DT;
            self.vel[1] += self.angle.sin() * ACCEL * DT;
        }
        for i in 0..2 {
            self.vel[i] *= DRAG;
            self.pos[i] = wrap(self.pos[i] + self.vel[i] * DT, size[i]);
        }
    }

    /// If touching `rock`'s outline, push the ship out of it and cancel the
    /// part of its velocity (relative to the rock) heading into it, so it slides
    /// along the surface. The rock is unaffected.
    pub fn collide(&mut self, rock: &Asteroid, size: [f32; 2]) {
        let Some(Contact { normal, depth, .. }) = contact(
            rock.pos,
            rock.radius,
            |dir| rock.surface(dir),
            self.pos,
            SHIP_RADIUS,
            |_| SHIP_RADIUS,
            size,
        ) else {
            return;
        };
        for i in 0..2 {
            self.pos[i] = wrap(self.pos[i] + normal[i] * depth, size[i]);
        }
        let into =
            (self.vel[0] - rock.vel[0]) * normal[0] + (self.vel[1] - rock.vel[1]) * normal[1];
        if into < 0.0 {
            for i in 0..2 {
                self.vel[i] -= into * normal[i];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Class, SHAPE_POINTS};

    const SIZE: [f32; 2] = [800.0, 600.0];

    #[test]
    fn stationary_ship_stays_put_without_input() {
        let mut ship = Ship::default();
        for _ in 0..60 {
            ship.step(Input::default(), SIZE);
        }
        assert_eq!(ship, Ship::default());
    }

    #[test]
    fn thrust_at_angle_zero_moves_along_x() {
        let mut ship = Ship::default();
        let input = Input {
            thrust: true,
            ..Input::default()
        };
        for _ in 0..60 {
            ship.step(input, SIZE);
        }
        assert!(ship.pos[0] > 0.0);
        assert!(ship.pos[1].abs() < 1e-3);
    }

    #[test]
    fn left_for_one_second_turns_by_turn_rate() {
        let mut ship = Ship::default();
        let input = Input {
            left: true,
            ..Input::default()
        };
        for _ in 0..60 {
            ship.step(input, SIZE);
        }
        assert!((ship.angle - TURN).abs() < 1e-3);
    }

    #[test]
    fn ship_wraps_at_right_edge() {
        let mut ship = Ship {
            pos: [399.0, 0.0],
            vel: [600.0, 0.0],
            angle: 0.0,
        };
        ship.step(Input::default(), SIZE);
        assert!(ship.pos[0] < -390.0, "pos[0] = {}", ship.pos[0]);
        assert!(ship.pos[0] >= -400.0);
    }

    #[test]
    fn same_inputs_are_bit_identical() {
        let run = || {
            let mut ship = Ship::default();
            for tick in 0u32..500 {
                let input = Input {
                    thrust: tick % 3 != 0,
                    left: tick % 7 < 3,
                    right: tick % 11 < 4,
                };
                ship.step(input, SIZE);
            }
            ship
        };
        let (a, b) = (run(), run());
        assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
        assert_eq!(a.vel.map(f32::to_bits), b.vel.map(f32::to_bits));
        assert_eq!(a.angle.to_bits(), b.angle.to_bits());
    }

    #[test]
    fn ship_slides_along_a_rock() {
        let rock = Asteroid::new(Class::Medium, [0.0, 0.0], [0.0, 0.0], [30.0; SHAPE_POINTS]);
        // Just touching the top of the rock, moving right and down into it.
        let mut ship = Ship {
            pos: [0.0, 30.0 + SHIP_RADIUS - 1.0],
            vel: [40.0, -40.0],
            angle: 0.0,
        };
        ship.collide(&rock, SIZE);
        assert!((ship.pos[1] - (30.0 + SHIP_RADIUS)).abs() < 1e-3);
        assert!((ship.vel[0] - 40.0).abs() < 1e-3, "keeps sliding sideways");
        assert!(ship.vel[1].abs() < 1e-3, "stops moving into the rock");
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
    fn long_rock(angle: f32) -> Asteroid {
        let shape = std::array::from_fn(|i| {
            let theta = i as f32 / SHAPE_POINTS as f32 * TAU;
            1.0 / ((theta.cos() / 60.0).powi(2) + (theta.sin() / 25.0).powi(2)).sqrt()
        });
        Asteroid {
            angle,
            ..Asteroid::new(Class::Medium, [0.0, 0.0], [0.0, 0.0], shape)
        }
    }

    fn ship_at(pos: [f32; 2]) -> Ship {
        Ship {
            pos,
            ..Ship::default()
        }
    }

    #[test]
    fn ship_touches_the_outline_not_a_bounding_circle() {
        let rock = long_rock(0.0);
        // Beside the narrow middle: inside the bounding circle, outside the rock.
        let mut beside = ship_at([0.0, 50.0]);
        beside.collide(&rock, SIZE);
        assert_eq!(beside.pos, [0.0, 50.0]);
        // Off the long end: pushed out to the tip.
        let mut end = ship_at([70.0, 0.0]);
        end.collide(&rock, SIZE);
        assert!((end.pos[0] - (60.0 + SHIP_RADIUS)).abs() < 1e-3);
    }

    #[test]
    fn outline_turns_with_the_rock() {
        let rock = long_rock(FRAC_PI_2);
        let mut above = ship_at([0.0, 50.0]);
        above.collide(&rock, SIZE);
        assert!((above.pos[1] - (60.0 + SHIP_RADIUS)).abs() < 1e-3);
    }
}
