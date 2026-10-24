//! Visual effects: things that are only drawn, never part of the game. Each
//! effect is an `Emitter` that knows nothing about the others and only adds
//! particles to the shared pool (ADR 0012). A new effect is a new emitter.

pub mod fire;
pub mod particles;
pub mod rng;
pub mod shots;

use sim::Snapshot;

use fire::Fires;
use particles::Particles;
use shots::Shots;

/// Something that keeps its own state per entity and throws particles.
pub trait Emitter {
    /// Catch up with `world`, `dt` seconds after the last update, adding any
    /// particles to `out`.
    fn update(&mut self, world: &Snapshot, dt: f32, out: &mut Particles);
}

/// Every effect, and the particles they've thrown.
#[derive(Default)]
pub struct Effects {
    pub fires: Fires,
    pub shots: Shots,
    pub particles: Particles,
}

impl Effects {
    /// Move every effect on by `dt` seconds, to match `world`. Its `hits`
    /// must be every hit since the last update, each given once.
    pub fn update(&mut self, world: &Snapshot, dt: f32) {
        // Particles already out move first, so new ones aren't moved twice.
        self.particles.step(dt, world.size);
        self.fires.update(world, dt, &mut self.particles);
        self.shots.update(world, dt, &mut self.particles);
    }
}
