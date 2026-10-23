//! Particles: lone art pixels that fly, slow and cool on their own once
//! something has flung them out. Every effect that sprays shares one pool.

use super::rng::Rng;

/// The most particles alive at once. New ones are dropped when it's full, so a
/// burst never grows the pool without limit.
pub const MAX_PARTICLES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// Where it is on the map, in px.
    pub pos: [f32; 2],
    /// In px/s.
    pub vel: [f32; 2],
    /// Seconds since it was flung out, and how long it lasts.
    pub age: f32,
    pub life: f32,
    /// How hot it is when thrown and when it burns out, 0 to 1 (see
    /// `heat_color` in heat.wgsl). It cools steadily between them.
    pub heat: (f32, f32),
    /// How fast it slows, per second.
    pub drag: f32,
    /// Picks its twinkle.
    pub seed: f32,
}

impl Particle {
    /// How hot it is now.
    pub fn heat_now(&self) -> f32 {
        let (start, end) = self.heat;
        start + (end - start) * self.age / self.life
    }

    /// Fly, slow and age by `dt` seconds on a map of `size`.
    pub fn advance(&mut self, dt: f32, size: [f32; 2]) {
        let slow = (-self.drag * dt).exp();
        self.vel = self.vel.map(|v| v * slow);
        self.pos = [0, 1].map(|i| wrap(self.pos[i] + self.vel[i] * dt, size[i]));
        self.age += dt;
    }
}

/// Every live particle, and the randomness effects use to fling them.
pub struct Particles {
    list: Vec<Particle>,
    pub rng: Rng,
}

impl Default for Particles {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            rng: Rng::new(0x5eed),
        }
    }
}

impl Particles {
    /// Add a particle, unless the pool is full.
    pub fn spawn(&mut self, particle: Particle) {
        if self.list.len() < MAX_PARTICLES {
            self.list.push(particle);
        }
    }

    /// Move every particle on by `dt` seconds and forget the ones that have
    /// burnt out.
    pub fn step(&mut self, dt: f32, size: [f32; 2]) {
        for p in &mut self.list {
            p.advance(dt, size);
        }
        self.list.retain(|p| p.age < p.life);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Particle> {
        self.list.iter()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// `x` wrapped into a map `size` wide, centered on the origin.
fn wrap(x: f32, size: f32) -> f32 {
    let half = size / 2.0;
    (x + half).rem_euclid(size) - half
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: [f32; 2] = [1000.0, 1000.0];

    fn spark() -> Particle {
        Particle {
            pos: [495.0, 0.0],
            vel: [600.0, 0.0],
            age: 0.0,
            life: 0.5,
            heat: (1.0, 0.5),
            drag: 2.0,
            seed: 0.0,
        }
    }

    #[test]
    fn particles_slow_wrap_cool_and_burn_out() {
        let mut particles = Particles::default();
        particles.spawn(spark());
        particles.step(0.1, SIZE);
        let p = *particles.iter().next().unwrap();
        assert!(p.vel[0] < 600.0, "slowed");
        assert!(p.pos[0] < 0.0, "wrapped to the left edge: {}", p.pos[0]);
        assert!((p.heat_now() - 0.9).abs() < 1e-5);
        particles.step(0.5, SIZE);
        assert!(particles.is_empty());
    }

    #[test]
    fn the_pool_never_grows_past_its_limit() {
        let mut particles = Particles::default();
        for _ in 0..MAX_PARTICLES + 10 {
            particles.spawn(spark());
        }
        assert_eq!(particles.len(), MAX_PARTICLES);
    }
}
