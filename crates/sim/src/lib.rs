pub const DT: f32 = 1.0 / 60.0;

pub const TURN: f32 = 4.0; // rad/s
pub const ACCEL: f32 = 1.2; // units/s²
pub const DRAG: f32 = 0.995; // per tick

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Input {
    pub thrust: bool,
    pub left: bool,
    pub right: bool,
}

/// Coordinates are normalized device space: x and y in [-1, 1), angle 0 points +x.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ship {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub angle: f32,
}

impl Ship {
    pub fn step(&mut self, input: Input) {
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
            self.pos[i] += self.vel[i] * DT;
            self.pos[i] = (self.pos[i] + 1.0).rem_euclid(2.0) - 1.0; // wrap to [-1, 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stationary_ship_stays_put_without_input() {
        let mut ship = Ship::default();
        for _ in 0..60 {
            ship.step(Input::default());
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
            ship.step(input);
        }
        assert!(ship.pos[0] > 0.0);
        assert!(ship.pos[1].abs() < 1e-6);
    }

    #[test]
    fn left_for_one_second_turns_by_turn_rate() {
        let mut ship = Ship::default();
        let input = Input {
            left: true,
            ..Input::default()
        };
        for _ in 0..60 {
            ship.step(input);
        }
        assert!((ship.angle - TURN).abs() < 1e-3);
    }

    #[test]
    fn ship_wraps_at_right_edge() {
        let mut ship = Ship {
            pos: [0.99, 0.0],
            vel: [1.0, 0.0],
            angle: 0.0,
        };
        ship.step(Input::default());
        assert!(ship.pos[0] < -0.9, "pos[0] = {}", ship.pos[0]);
        assert!(ship.pos[0] >= -1.0);
    }

    #[test]
    fn same_inputs_are_bit_identical() {
        let run = || {
            let mut ship = Ship::default();
            for tick in 0u32..500 {
                ship.step(Input {
                    thrust: tick % 3 != 0,
                    left: tick % 7 < 3,
                    right: tick % 11 < 4,
                });
            }
            ship
        };
        let (a, b) = (run(), run());
        assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
        assert_eq!(a.vel.map(f32::to_bits), b.vel.map(f32::to_bits));
        assert_eq!(a.angle.to_bits(), b.angle.to_bits());
    }
}
