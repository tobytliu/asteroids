# Asteroids

A multiplayer Asteroids-style game written in Rust and compiled to wasm for the browser, rendered with `wgpu` (WebGPU, falling back to WebGL2). The simulation is deterministic and fixed-tick so that an authoritative native server can be added later; today it is a single controllable ship.

## Crates

- `crates/sim` — pure game logic. Compiles and tests natively.
- `crates/client` — browser client: rendering, input, and the `requestAnimationFrame` loop. Wasm-only (the crate is empty on other targets).
- `crates/server` — planned native authoritative server. Does not exist yet.

## Architecture rules

- `sim` has no `wgpu`, `web-sys`, `wasm-bindgen` or I/O. It must build and test with plain `cargo test`. Never put browser types in it; the server will depend on it.
- `client` depends on `sim` and contains **no game logic**. Input handling only produces a `sim::Input`.
- Simulation advances in fixed ticks (`sim::DT` = 1/60 s) via an accumulator, never by variable frame time. Required for future netcode.
- The client is browser-only: no `winit`. Canvas and events come straight from `web-sys`.
- Coordinates are normalized device space: x and y in [-1, 1), angle 0 points +x.

## Prerequisites

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

## Commands

```sh
cargo test -p sim                                        # sim unit tests
cargo build -p client --target wasm32-unknown-unknown    # type-check/build the client
cd crates/client && trunk serve                          # dev server at http://127.0.0.1:8080
```

## Notes

- `wgpu` changes its instance/adapter/device/surface API between releases. Check the installed version's source or docs before editing `crates/client/src/gfx.rs`.
- Known limitation: the ship stretches on non-square canvases (no aspect correction yet).
