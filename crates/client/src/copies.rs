//! Where to draw things on a wrapped map. Anything near the seam can show up
//! more than once, and anything out of view not at all, so the client works
//! out each copy's position relative to the camera and the shaders never see
//! map coordinates (ADR 0004).

use crate::camera::View;

/// What the camera can see this frame.
pub struct Visible {
    /// The view's center wrapped into the map.
    center: [f32; 2],
    /// Half the visible area's width and height, in world px.
    half: [f32; 2],
    map: [f32; 2],
}

impl Visible {
    /// `view_size` is the view's size in CSS px; `map` is the map's size.
    pub fn new(view: &View, view_size: [f32; 2], map: [f32; 2]) -> Self {
        Self {
            center: view.center_in_map(map),
            half: view_size.map(|s| s / 2.0 / view.zoom),
            map,
        }
    }

    /// The position, relative to the view's center, of every copy of
    /// something at `pos` reaching `reach` px from its center that overlaps
    /// the view. Usually one; none when it's out of view.
    pub fn copies(&self, pos: [f32; 2], reach: f32) -> impl Iterator<Item = [f32; 2]> + use<> {
        let (half, map) = (self.half, self.map);
        let d = sim::wrapped_offset(self.center, pos, map);
        // Which whole-map shifts on each axis keep the copy within reach of
        // the view.
        let shifts = move |i: usize| {
            let edge = half[i] + reach;
            let lo = ((-edge - d[i]) / map[i]).ceil() as i32;
            let hi = ((edge - d[i]) / map[i]).floor() as i32;
            (lo..=hi).map(move |k| d[i] + k as f32 * map[i])
        };
        let ys: Vec<f32> = shifts(1).collect();
        shifts(0).flat_map(move |x| ys.clone().into_iter().map(move |y| [x, y]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: [f32; 2] = [1000.0, 1000.0];

    fn visible(center: [f64; 2], view_size: [f32; 2]) -> Visible {
        Visible::new(&View { center, zoom: 1.0 }, view_size, MAP)
    }

    fn copies(v: &Visible, pos: [f32; 2], reach: f32) -> Vec<[f32; 2]> {
        v.copies(pos, reach).collect()
    }

    #[test]
    fn something_in_view_is_drawn_once_relative_to_the_camera() {
        let v = visible([100.0, 0.0], [400.0, 300.0]);
        assert_eq!(copies(&v, [150.0, 20.0], 10.0), [[50.0, 20.0]]);
    }

    #[test]
    fn something_out_of_view_is_not_drawn() {
        let v = visible([0.0, 0.0], [400.0, 300.0]);
        assert!(copies(&v, [300.0, 0.0], 10.0).is_empty());
        // Just reaching into view counts.
        assert_eq!(copies(&v, [205.0, 0.0], 10.0), [[205.0, 0.0]]);
    }

    #[test]
    fn things_are_seen_across_the_seam() {
        // The camera sits beyond the right edge; a rock just inside the left
        // edge is right next to it.
        let v = visible([1490.0, 0.0], [400.0, 300.0]);
        assert_eq!(copies(&v, [-480.0, 0.0], 10.0), [[30.0, 0.0]]);
    }

    #[test]
    fn a_view_wider_than_the_map_shows_several_copies() {
        let v = visible([0.0, 0.0], [2500.0, 300.0]);
        assert_eq!(
            copies(&v, [0.0, 0.0], 10.0),
            [[-1000.0, 0.0], [0.0, 0.0], [1000.0, 0.0]]
        );
    }
}
