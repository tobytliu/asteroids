pub const DT: f32 = 1.0 / 60.0;

// World units are CSS pixels, so the ship handles the same on any screen.
pub const TURN: f32 = 4.0; // rad/s
pub const ACCEL: f32 = 500.0; // px/s²
pub const DRAG: f32 = 0.995; // per tick

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Input {
    pub thrust: bool,
    pub left: bool,
    pub right: bool,
}

/// Position is in px with the origin at the world's center and +y up; angle 0 points +x.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ship {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub angle: f32,
}

impl Ship {
    /// Advance one tick in a world of `size` (width, height) px, wrapping
    /// position to [-size/2, size/2) on each axis.
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
            self.pos[i] += self.vel[i] * DT;
            let half = size[i] / 2.0;
            self.pos[i] = (self.pos[i] + half).rem_euclid(size[i]) - half;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
