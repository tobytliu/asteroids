//! Randomness for visual effects. It's the client's own, never the sim's
//! `Rng`: nothing it picks changes the game, only how it looks.

/// A small xorshift generator.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        // Xorshift gets stuck at zero.
        Self(seed.max(1))
    }

    /// A value in [0, 1).
    pub fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// A value in [lo, hi).
    pub fn range(&mut self, (lo, hi): (f32, f32)) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_stay_in_range() {
        let mut rng = Rng::new(0);
        for _ in 0..1000 {
            let x = rng.range((2.0, 3.0));
            assert!((2.0..3.0).contains(&x), "{x}");
        }
    }
}
