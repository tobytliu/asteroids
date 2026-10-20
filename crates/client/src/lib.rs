//! Browser client: rendering, input and the frame loop. No game logic lives
//! here; input handling only produces a `sim::Input`.
//!
//! Only the browser modules are wasm-only. Anything else compiles natively, so
//! it can be unit-tested with plain `cargo test -p client`.

#[cfg(target_arch = "wasm32")]
mod gfx;
#[cfg(target_arch = "wasm32")]
mod sprite;
#[cfg(target_arch = "wasm32")]
mod web;
