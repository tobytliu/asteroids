//! Browser client: rendering, input and the frame loop. No game logic lives
//! here; input handling only produces a `sim::Input`.
#![cfg(target_arch = "wasm32")]

mod gfx;
mod sprite;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, KeyboardEvent, Window};

/// Longest frame time fed to the accumulator, to avoid a catch-up spiral
/// after the tab has been in the background.
const MAX_FRAME_SECS: f32 = 0.25;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(run());
}

async fn run() {
    let window = web_sys::window().expect("no window");
    let canvas: HtmlCanvasElement = window
        .document()
        .and_then(|document| document.get_element_by_id("game"))
        .expect("no #game element")
        .dyn_into()
        .expect("#game is not a canvas");

    let gfx = Rc::new(RefCell::new(gfx::Gfx::new(canvas).await));
    let input = Rc::new(Cell::new(sim::Input::default()));

    listen_for_input(&window, &input);
    {
        let gfx = gfx.clone();
        listen(
            &window,
            "resize",
            Closure::<dyn FnMut()>::new(move || gfx.borrow_mut().resize()),
        );
    }

    let mut ship = sim::Ship::default();
    let mut accumulator = 0.0f32;
    let mut last_ms: Option<f64> = None;

    // The closure reschedules itself, so it holds a handle to its own slot.
    let frame = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let next_frame = frame.clone();
    let raf_window = window.clone();
    *frame.borrow_mut() = Some(Closure::new(move |now_ms: f64| {
        let elapsed = last_ms.map_or(0.0, |last| ((now_ms - last) / 1000.0) as f32);
        last_ms = Some(now_ms);

        accumulator += elapsed.clamp(0.0, MAX_FRAME_SECS);
        let world_size = gfx.borrow().view_size();
        while accumulator >= sim::DT {
            ship.step(input.get(), world_size);
            accumulator -= sim::DT;
        }
        gfx.borrow_mut().render(&ship);

        request_frame(&raf_window, next_frame.borrow().as_ref().unwrap());
    }));
    request_frame(&window, frame.borrow().as_ref().unwrap());
}

fn listen_for_input(window: &Window, input: &Rc<Cell<sim::Input>>) {
    for (event, pressed) in [("keydown", true), ("keyup", false)] {
        let input = input.clone();
        let on_key = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
            let mut state = input.get();
            // `code` is the physical key, so this works on non-QWERTY layouts.
            match event.code().as_str() {
                "KeyW" => state.thrust = pressed,
                "KeyA" => state.left = pressed,
                "KeyD" => state.right = pressed,
                _ => return,
            }
            input.set(state);
        });
        listen(window, event, on_key);
    }

    // Keyups are not delivered while unfocused, so release everything on blur.
    let input = input.clone();
    listen(
        window,
        "blur",
        Closure::<dyn FnMut()>::new(move || input.set(sim::Input::default())),
    );
}

/// Attach a listener that lives for the rest of the page's lifetime.
fn listen<T: ?Sized + wasm_bindgen::closure::WasmClosure>(
    window: &Window,
    event: &str,
    callback: Closure<T>,
) {
    window
        .add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())
        .expect("failed to add event listener");
    callback.forget();
}

fn request_frame(window: &Window, callback: &Closure<dyn FnMut(f64)>) {
    window
        .request_animation_frame(callback.as_ref().unchecked_ref())
        .expect("requestAnimationFrame failed");
}
