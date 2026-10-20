//! The browser entry point: input and the frame loop.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, KeyboardEvent, Window};

use crate::camera::{Camera, ShipCamera};

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

    let gfx = Rc::new(RefCell::new(crate::gfx::Gfx::new(canvas).await));
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

    // A fresh asteroid field each page load.
    let seed = (js_sys::Math::random() * u32::MAX as f64) as u64;
    let mut world = sim::World::new(seed, sim::MAP_SIZE);
    let mut camera: Box<dyn Camera> = Box::new(ShipCamera::default());
    let mut accumulator = 0.0f32;
    let mut last_ms: Option<f64> = None;

    // The closure reschedules itself, so it holds a handle to its own slot.
    let frame = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let next_frame = frame.clone();
    let raf_window = window.clone();
    *frame.borrow_mut() = Some(Closure::new(move |now_ms: f64| {
        let elapsed = last_ms.map_or(0.0, |last| ((now_ms - last) / 1000.0) as f32);
        last_ms = Some(now_ms);

        let elapsed = elapsed.clamp(0.0, MAX_FRAME_SECS);
        accumulator += elapsed;
        while accumulator >= sim::DT {
            world.step(input.get());
            accumulator -= sim::DT;
        }
        let view = camera.update(&world, elapsed);
        gfx.borrow_mut()
            .render(&world, view, input.get().thrust, elapsed);

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
