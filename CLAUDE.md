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
- World units are CSS pixels: origin at the center, +y up, angle 0 points +x. The world is the canvas's CSS size, so a bigger window means a bigger play area while the ship's size and handling stay constant. The shader converts px to clip space.

## Prerequisites

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

## Commands

```sh
cargo test -p sim                                        # sim unit tests
cargo build -p client --target wasm32-unknown-unknown    # type-check/build the client
trunk serve                                              # dev server at http://127.0.0.1:8080, rebuilds on save
trunk build --release                                    # production build into dist/
```

## Notes

- `Trunk.toml` at the repo root points trunk at `crates/client/index.html` and watches all of `crates/`, so run trunk from the root.
- The ship is pixel art defined as text in `crates/client/src/sprite.rs`. The shader draws it on a 3 px grid with the angle snapped to 36 steps (Escape Velocity style), and it's upscaled with Scale2x before rotating so diagonals stay clean.
- The engine flame is drawn procedurally in `ship.wgsl` (`flame()`), behind the sprite. It's purely visual: the client eases a flame intensity toward the thrust input in `Gfx::render`, and the sim knows nothing about it.
- `wgpu` changes its instance/adapter/device/surface API between releases. Check the installed version's source or docs before editing `crates/client/src/gfx.rs`.
