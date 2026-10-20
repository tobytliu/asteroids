# Asteroids

A multiplayer Asteroids-style game written in Rust and compiled to wasm for the browser, rendered with `wgpu` (WebGPU, falling back to WebGL2). The simulation is deterministic and fixed-tick so that an authoritative native server can be added later; today it is a single controllable ship.

## Crates

- `crates/sim` — pure game logic. Compiles and tests natively. `World` holds the ship and asteroids and steps them in a fixed order. Every asteroid has a `Class` (Small, Medium, Big, Large; Large is 120–250 px mean radius, 2–4× a big rock's); `Class::params` sets each class's size range, speed, spin, drift floor and whether it gets bites, and `random_shape` dispatches on class so each class can make its outline its own way (today all make the same bitten potato). Each asteroid's outline is 32 radii (`Asteroid::shape`), generated as a potato that varies in length, taper, lopsidedness and lumps (no pinched waists), with bites scooped out of bigger rocks' edges and every outline smoothed so no corner is sharp; collisions compare outlines along the line between centers (the ship is a circle), measured across the wrapped edges. Rock-rock hits are elastic with friction, which transfers spin; mass and moment of inertia come from the outline's actual polygon. Randomness comes only from the seeded `Rng`.
- `crates/client` — browser client: rendering, input, and the `requestAnimationFrame` loop. The browser modules (`web`, `gfx`, `sprite`) and their dependencies are wasm-only; any other module (pure maths such as the camera) must compile natively so `cargo test -p client` can test it.
- `crates/server` — planned native authoritative server. Does not exist yet.

## Architecture rules

- `sim` has no `wgpu`, `web-sys`, `wasm-bindgen` or I/O. It must build and test with plain `cargo test`. Never put browser types in it; the server will depend on it.
- `client` depends on `sim` and contains **no game logic**. Input handling only produces a `sim::Input`.
- Simulation advances in fixed ticks (`sim::DT` = 1/60 s) via an accumulator, never by variable frame time. Required for future netcode.
- The client is browser-only: no `winit`. Canvas and events come straight from `web-sys`.
- World units are CSS pixels: origin at the center, +y up, angle 0 points +x. The map is a fixed size (`sim::MAP_SIZE`) whose opposite edges join (ADR 0003); a bigger window only shows more of it.
- `client` never decides anything about the game. Cameras, interpolation and visual effects only change what is shown.
- Ships are steered only by `sim::Input`, one per ship per tick. Keyboard, network and AI are all controllers that produce inputs. AI belongs in its own crate, never in `client` (ADR 0006).

## Determinism rules

The sim must be bit-exact across native and wasm given the same seed and inputs (ADR 0002). Every sim change must keep these:

1. Single-threaded. The step calls its systems in an order written out in code; no scheduler.
2. Only rely on ECS query order where order can't change the result. Anything order-sensitive (sequential collision resolution, random draws, spawning) sorts by entity ID first.
3. Entity IDs come from the world's counter and are never reused. `hecs` entity handles never leave `sim`.
4. Spawns and despawns are queued and applied at one fixed point in the tick, in entity ID order.
5. Randomness only from the world's own generator. No `HashMap`/`HashSet` iteration anywhere in `sim`.
6. No clock, frame time, environment or I/O in `sim`.
7. Sine, cosine and other transcendental functions come from the `libm` crate, never `std`'s float methods.
8. The determinism test (same seed and scripted inputs run twice give identical world checksums every tick) must pass.

## Knowledge repository

- `GLOSSARY.md` is the game's vocabulary. Use its terms in code, comments and docs (e.g. map, not canvas, for the playfield; view, not screen).
- `docs/adr/` records decisions that are hard to reverse. Read the relevant ADR before changing something it covers, and add a new ADR (next number) when making a decision like that. Don't edit an accepted ADR's decision; supersede it with a new one.

## Prerequisites

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

## Commands

```sh
cargo test -p sim                                        # sim unit tests
cargo test -p client                                     # client's native-testable modules
cargo build -p client --target wasm32-unknown-unknown    # type-check/build the client
trunk serve                                              # dev server at http://127.0.0.1:8090, rebuilds on save
trunk build --release                                    # production build into dist/
```

## Notes

- Debugging: the seed is logged to the browser console on load, and `?seed=N` in the URL replays that field. The backquote key toggles a readout (`debug.rs`, the `#debug` element in `index.html`) with the seed, ship position, camera center, map size and FPS.
- `Trunk.toml` at the repo root points trunk at `crates/client/index.html` and watches all of `crates/`, so run trunk from the root.
- The ship is pixel art defined as text in `crates/client/src/sprite.rs`. The shader draws it on a 3 px grid with the angle snapped to 36 steps (Escape Velocity style), and it's upscaled with Scale2x before rotating so diagonals stay clean.
- Shaders: `common.wgsl` (view uniform, chunky-pixel grid, angle snapping, hash) is prepended to each other shader with `concat!`. Shaders never see map coordinates: each frame the camera (`camera.rs`, a swappable `Camera` trait; today `ShipCamera` keeps the ship centered) gives a view with a continuous center that never wraps (ADR 0004), and `copies.rs` works out which wrapped copies of each object overlap the view and their positions relative to its center. Ships and rocks are both drawn as instances of those copies. Each frame draws the world blended between the last two ticks (`interpolate.rs`, ADR 0005), so motion is smooth on screens faster than 60 Hz; the camera follows the blended ship, and what's shown is up to one tick behind the sim on purpose. `cargo test -p client` also compiles every shader with naga, so shader errors show up without a browser.
- The starfield (`stars.wgsl`) is a full-screen pass drawn first; stars come from hashing art-grid cells, so there's no star data anywhere. It's a separate layer that scrolls at `PARALLAX` (0.5) of the camera's continuous center, so it never shows the map's seam; its pattern repeats every `STAR_PERIOD` art pixels so the scroll offset sent to the GPU stays small.
- Asteroids are drawn in `asteroid.wgsl` from the sim's outline (passed as instance data, so the drawn edge is the collision edge), their `look` seed (palette, craters) and their class (`surface_rules()` picks which class's ridge/crater rules apply; Large borrows Big's for now). The instance already uses 13 of WebGL2's 16 vertex attributes, so a longer outline needs another way in (e.g. a data texture). Unlike the ship they rotate smoothly, not in 36 steps, so the drawn outline never drifts from the collision outline. Because of that, shading uses solid bands with no dither or per-pixel texture, and no detail smaller than a couple of art pixels: anything finer flickers as the rock turns. Most rocks get ridges (`ridges()`): a few crests at their own angles, mostly curved (arc and/or S-bend; only an extra ridge may run straight), that tilt the surface normal, each with a steep side that reads as a crease, splitting the rock into faces. About a third of medium and big rocks get craters (small ones none), usually 2 or 3; a lone crater only happens when a second won't fit.
- The engine flame is drawn procedurally in `ship.wgsl` (`flame()`), behind the sprite. It's purely visual: `Gfx::render` eases a flame intensity toward the thrust input and measures a smoothed turn rate from the ship's angle, which bends the plume into a trailing arc (`EXHAUST_SPEED` sets how much). The sim knows nothing about it.
- `wgpu` changes its instance/adapter/device/surface API between releases. Check the installed version's source or docs before editing `crates/client/src/gfx.rs`.
