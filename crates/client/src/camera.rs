//! Cameras decide what part of the map is in view (ADR 0004). They only change
//! what is shown, never the game.

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

/// Every frame, a camera watches the world and gives back what to show.
pub trait Camera {
    /// `dt` is seconds since the last frame.
    fn update(&mut self, world: &sim::World, dt: f32) -> View;
}

/// Keeps the ship in the middle of the view.
#[derive(Default)]
pub struct ShipCamera {
    /// The view's center, and where the ship was when it was last updated.
    last: Option<([f64; 2], [f32; 2])>,
}

impl Camera for ShipCamera {
    fn update(&mut self, world: &sim::World, _dt: f32) -> View {
        let ship = world.ship.pos;
        // Move by however far the ship moved, the short way across the seam,
        // so the center never jumps when the ship wraps.
        let center = match self.last {
            None => ship.map(f64::from),
            Some((center, last_ship)) => {
                let moved = sim::wrapped_offset(last_ship, ship, world.size);
                [0, 1].map(|i| center[i] + moved[i] as f64)
            }
        };
        self.last = Some((center, ship));
        View { center, zoom: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_ship_at(pos: [f32; 2]) -> sim::World {
        let mut world = sim::World::new(1, [1000.0, 1000.0]);
        world.ship.pos = pos;
        world
    }

    #[test]
    fn starts_on_the_ship() {
        let view = ShipCamera::default().update(&world_with_ship_at([10.0, -20.0]), 0.0);
        assert_eq!(view.center, [10.0, -20.0]);
        assert_eq!(view.zoom, 1.0);
    }

    #[test]
    fn follows_the_ship_across_the_seam_without_wrapping() {
        let mut camera = ShipCamera::default();
        camera.update(&world_with_ship_at([490.0, 0.0]), 0.0);
        // The ship wraps from the right edge to the left; the camera carries on
        // to the right, past the map's edge.
        let view = camera.update(&world_with_ship_at([-490.0, 5.0]), 0.0);
        assert_eq!(view.center, [510.0, 5.0]);
        let view = camera.update(&world_with_ship_at([-480.0, 5.0]), 0.0);
        assert_eq!(view.center, [520.0, 5.0]);
        assert_eq!(view.center_in_map([1000.0, 1000.0]), [-480.0, 5.0]);
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
