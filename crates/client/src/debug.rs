//! A debug readout over the game (the `#debug` element in index.html), shown
//! and hidden with the backquote key.

use web_sys::HtmlElement;

use crate::camera::View;

/// How often the readout is refreshed, in seconds. Every frame would be
/// unreadable and needlessly touch the DOM.
const REFRESH_SECS: f32 = 0.25;

pub struct DebugOverlay {
    element: HtmlElement,
    seed: u64,
    /// Frames and seconds since the readout was last refreshed, for FPS.
    frames: u32,
    secs: f32,
}

impl DebugOverlay {
    pub fn new(element: HtmlElement, seed: u64) -> Self {
        Self {
            element,
            seed,
            frames: 0,
            secs: 0.0,
        }
    }

    pub fn toggle(&mut self) {
        self.element.set_hidden(!self.element.hidden());
        self.frames = 0;
        self.secs = 0.0;
    }

    /// Call once per frame with the world and view being shown, the ship the
    /// view follows, and `dt`, the seconds since the last frame.
    pub fn frame(&mut self, world: &sim::Snapshot, focus: sim::EntityId, view: View, dt: f32) {
        if self.element.hidden() {
            return;
        }
        self.frames += 1;
        self.secs += dt;
        if self.secs < REFRESH_SECS {
            return;
        }
        let fps = self.frames as f32 / self.secs;
        self.frames = 0;
        self.secs = 0.0;

        let ship = match world.ship(focus) {
            Some(ship) => format!("{:.0}, {:.0}", ship.body.pos[0], ship.body.pos[1]),
            None => "gone".to_string(),
        };
        let text = format!(
            "seed   {}\n\
             ship   {focus} at {ship}\n\
             ships  {}, rocks {}\n\
             camera {:.0}, {:.0}\n\
             map    {:.0} × {:.0}\n\
             fps    {fps:.0}",
            self.seed,
            world.ships.len(),
            world.rocks.len(),
            view.center[0],
            view.center[1],
            world.size[0],
            world.size[1],
        );
        self.element.set_text_content(Some(&text));
    }
}
