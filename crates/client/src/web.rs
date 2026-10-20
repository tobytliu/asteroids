//! The browser entry point: input and the frame loop.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, HtmlElement, KeyboardEvent, UrlSearchParams, Window};

use crate::camera::{Camera, ShipCamera};
use crate::debug::DebugOverlay;
use crate::interpolate;

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
    let document = window.document().expect("no document");
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("game")
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

    // A fresh asteroid field each page load, unless the URL asks for one.
    let seed =
        seed_from_url(&window).unwrap_or_else(|| (js_sys::Math::random() * u32::MAX as f64) as u64);
    web_sys::console::log_1(
        &format!("seed {seed} (add ?seed={seed} to the URL to replay it)").into(),
    );

    let debug_element: HtmlElement = document
        .get_element_by_id("debug")
        .expect("no #debug element")
        .dyn_into()
        .expect("#debug is not an HTML element");
    let debug = Rc::new(RefCell::new(DebugOverlay::new(debug_element, seed)));
    listen_for_debug_key(&window, &debug);

    let mut world = sim::World::new(seed, sim::MAP_SIZE);
    // The world as it was one tick earlier, so frames can be drawn between.
    let mut prev = world.clone();
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
            prev.clone_from(&world);
            world.step(input.get());
            accumulator -= sim::DT;
        }
        // The camera follows the blended ship too, so it stays perfectly still
        // in the middle of the view.
        let shown = interpolate::blend(&prev, &world, accumulator / sim::DT);
        let view = camera.update(&shown, elapsed);
        gfx.borrow_mut()
            .render(&shown, view, input.get().thrust, elapsed);
        debug.borrow_mut().frame(&shown, view, elapsed);

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

/// The `seed` query parameter, if the URL has a valid one.
fn seed_from_url(window: &Window) -> Option<u64> {
    let search = window.location().search().ok()?;
    let seed = UrlSearchParams::new_with_str(&search).ok()?.get("seed")?;
    match seed.parse() {
        Ok(seed) => Some(seed),
        Err(_) => {
            web_sys::console::warn_1(&format!("ignoring ?seed={seed}: not a whole number").into());
            None
        }
    }
}

/// Backquote shows and hides the debug readout.
fn listen_for_debug_key(window: &Window, debug: &Rc<RefCell<DebugOverlay>>) {
    let debug = debug.clone();
    let on_key = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        if event.code() == "Backquote" && !event.repeat() {
            debug.borrow_mut().toggle();
        }
    });
    listen(window, "keydown", on_key);
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
