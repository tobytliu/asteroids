//! Cameras decide what part of the map is in view (ADR 0004). They only change
//! what is shown, never the game.

use sim::{EntityId, Snapshot};

/// What a camera shows: the point at the middle of the view, and how much it's
/// magnified.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// A continuous position in px that is never wrapped into the map, so it
    /// can be far outside the map's coordinates. f64 because it keeps growing
    /// as the ship flies, and f32 would start to jitter.
    pub center: [f64; 2],
    /// Screen px per world px.
    pub zoom: f32,
}

impl View {
    /// This view with its center rounded to whole device pixels, given how
    /// many device pixels there are per CSS px, so art pixels stay crisp while
    /// scrolling.
    pub fn snapped(self, device_px_per_css: f64) -> Self {
        let scale = device_px_per_css * self.zoom as f64;
        Self {
            center: self.center.map(|c| (c * scale).round() / scale),
            ..self
        }
    }

    /// The center wrapped into a map of `size`, in [-size/2, size/2).
    pub fn center_in_map(&self, size: [f32; 2]) -> [f32; 2] {
        [0, 1].map(|i| {
            let size = size[i] as f64;
            ((self.center[i] + size / 2.0).rem_euclid(size) - size / 2.0) as f32
        })
    }
}

/// Every frame, a camera watches the world and its focus ship and gives back
/// what to show.
pub trait Camera {
    /// `world` is what's being drawn this frame, `focus` the ship the camera
    /// is about, and `dt` seconds since the last frame.
    fn update(&mut self, world: &Snapshot, focus: EntityId, dt: f32) -> View;
}

/// Keeps the focus ship in the middle of the view.
#[derive(Default)]
pub struct ShipCamera {
    /// The view's center, if it's been placed yet.
    center: Option<[f64; 2]>,
    /// Where the focus ship was at the last update, if it was there.
    last_ship: Option<[f32; 2]>,
}

impl Camera for ShipCamera {
    fn update(&mut self, world: &Snapshot, focus: EntityId, _dt: f32) -> View {
        let ship = world.ship(focus).map(|s| s.body.pos);
        let center = match (self.center, self.last_ship, ship) {
            // Move by however far the ship moved, the short way across the
            // seam, so the center never jumps when the ship wraps.
            (Some(center), Some(last), Some(ship)) => {
                let moved = sim::wrapped_offset(last, ship, world.size);
                [0, 1].map(|i| center[i] + moved[i] as f64)
            }
            // Nothing to follow: stay put.
            (Some(center), _, None) => center,
            // The ship is back: go to it, the short way across the seam, so the
            // center stays continuous.
            (Some(center), None, Some(ship)) => {
                let view = View { center, zoom: 1.0 };
                let to_ship = sim::wrapped_offset(view.center_in_map(world.size), ship, world.size);
                [0, 1].map(|i| center[i] + to_ship[i] as f64)
            }
            (None, _, Some(ship)) => ship.map(f64::from),
            (None, _, None) => [0.0; 2],
        };
        self.center = Some(center);
        self.last_ship = ship;
        View { center, zoom: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: [f32; 2] = [1000.0, 1000.0];

    /// A world with one ship, and its ID.
    fn world() -> (Snapshot, EntityId) {
        let mut world = sim::World::new(1, MAP);
        let ship = world.spawn_ship();
        (world.snapshot(), ship)
    }

    fn with_ship_at(world: &Snapshot, pos: [f32; 2]) -> Snapshot {
        let mut world = world.clone();
        world.ships[0].body.pos = pos;
        world
    }

    #[test]
    fn starts_on_the_ship() {
        let (world, ship) = world();
        let view = ShipCamera::default().update(&with_ship_at(&world, [10.0, -20.0]), ship, 0.0);
        assert_eq!(view.center, [10.0, -20.0]);
        assert_eq!(view.zoom, 1.0);
    }

    #[test]
    fn follows_the_ship_across_the_seam_without_wrapping() {
        let (world, ship) = world();
        let mut camera = ShipCamera::default();
        camera.update(&with_ship_at(&world, [490.0, 0.0]), ship, 0.0);
        // The ship wraps from the right edge to the left; the camera carries on
        // to the right, past the map's edge.
        let view = camera.update(&with_ship_at(&world, [-490.0, 5.0]), ship, 0.0);
        assert_eq!(view.center, [510.0, 5.0]);
        let view = camera.update(&with_ship_at(&world, [-480.0, 5.0]), ship, 0.0);
        assert_eq!(view.center, [520.0, 5.0]);
        assert_eq!(view.center_in_map(MAP), [-480.0, 5.0]);
    }

    #[test]
    fn holds_still_while_the_ship_is_missing_then_returns_to_it() {
        let (world, ship) = world();
        let mut camera = ShipCamera::default();
        camera.update(&with_ship_at(&world, [100.0, 0.0]), ship, 0.0);
        let mut gone = world.clone();
        gone.ships.clear();
        assert_eq!(camera.update(&gone, ship, 0.0).center, [100.0, 0.0]);
        // It comes back across the seam: 150 px to the right, not 850 left.
        camera.update(&with_ship_at(&world, [400.0, 0.0]), ship, 0.0);
        camera.update(&gone, ship, 0.0);
        let back = camera.update(&with_ship_at(&world, [-450.0, 50.0]), ship, 0.0);
        assert_eq!(back.center, [550.0, 50.0]);
    }

    #[test]
    fn snapping_rounds_to_device_pixels() {
        let view = View {
            center: [10.3, -4.6],
            zoom: 1.0,
        };
        assert_eq!(view.snapped(1.0).center, [10.0, -5.0]);
        assert_eq!(view.snapped(2.0).center, [10.5, -4.5]);
    }
}
