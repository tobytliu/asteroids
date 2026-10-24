# Asteroids

A multiplayer Asteroids-style game written in Rust and compiled to wasm for the browser, rendered with `wgpu` (WebGPU, falling back to WebGL2). The simulation is deterministic and fixed-tick so that an authoritative native server can be added later; today it is a single controllable ship.

## Crates

- `crates/sim` — pure game logic. Compiles and tests natively. `World` keeps its entities in a `hecs` ECS (ADR 0001): each has an `EntityId` and components (`Body` for position, velocity and angle; `Spin`; `Asteroid`, a rock's fixed data; `Ship`, holding its last input and its spin; `Accel`, what this tick's forces add up to; `Drift`, a rock's drift floor; `Gun`, a ship's weapon; `Bolt`, one shot from it). `step` takes inputs keyed by ship ID and runs its systems in the order written in it. Ships turn with momentum: a held turn spins them up to `TURN_MAX`, turning against their spin brakes harder, and letting go brakes them to a stop in about a tenth of a second. Thrust tops out at `MAX_SPEED`. Apart from ships steering and collisions, motion changes only through force systems (`forces.rs`, ADR 0009) that add to `Accel`, applied together by one integrate step. `spawn_ship`/`remove_ship` add and remove ships between ticks; the first ship goes at the spawn point. `snapshot()` gives the world as plain data in ID order, which is all the client sees; hecs never leaves the sim. Every asteroid has a `Class` (Small, Medium, Big, Large; Medium is 32–60 px mean radius, Big about 100 px, Large 120–250 px); `Class::params` sets each class's size range, speed, spin, drift floor and whether it gets bites, and `random_shape` dispatches on class so each class can make its outline its own way (today all make the same bitten potato). Each asteroid's outline is 32 radii (`Asteroid::shape`), generated as a potato that varies in length, taper, lopsidedness and lumps (no pinched waists), with bites scooped out of bigger rocks' edges and every outline smoothed so no corner is sharp; collisions compare outlines along the line between centers (a ship is a circle), measured across the wrapped edges. Rocks push ships out without being moved by them; ships bounce off each other as equal masses. Rock-rock hits are elastic with friction, which transfers spin; mass and moment of inertia come from the outline's actual polygon. Randomness comes only from the seeded `Rng`. Ships shoot bolts (`weapon.rs`, ADR 0013): `Input::shoot` says the trigger is held, the ship's `Gun` decides when that shoots (Single trigger: once per press, then a cooldown), and everything about a weapon is in `WeaponKind::params`. A bolt flies straight with the ship's velocity added, its path swept each tick against rock outlines and other ships; the first touch despawns it and is recorded in `Snapshot::hits` for that tick. Spawns and despawns during a tick go through `World`'s queue, applied at the end of `step` (rule 4).
- `crates/client` — browser client: rendering, input, and the `requestAnimationFrame` loop. The browser modules (`web`, `gfx`, `debug`) and their dependencies are wasm-only; any other module (pure maths such as the camera) must compile natively so `cargo test -p client` can test it.
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
7. Sine, cosine and other transcendental functions (and `hypot`) come from `libm` via `sim/src/math.rs`, never `std`'s float methods; write `powi` out as multiplication. `crates/sim/clippy.toml` forbids the `std` ones, so run clippy.
8. The determinism tests must pass: the scripted run gives identical `World::checksum`s every tick when repeated, and matches the pinned `GOLDEN` checksums. Refactors must keep `GOLDEN` as it is; re-pin it only for a change meant to alter what the sim does, and say so in the commit.

## Knowledge repository

- `GLOSSARY.md` is the game's vocabulary. Use its terms in code, comments and docs (e.g. map, not canvas, for the playfield; view, not screen).
- `docs/roadmap.md` tracks what's built, what's next and what's still open. Update it when work lands or plans change.
- `docs/adr/` records decisions that are hard to reverse. Read the relevant ADR before changing something it covers, and add a new ADR (next number) when making a decision like that. Don't edit an accepted ADR's decision; supersede it with a new one.

## Prerequisites

```sh
rustup target add wasm32-unknown-unknown wasm32-wasip1
brew install wasmtime   # runs the sim's tests under wasm
cargo install trunk
```

## Commands

```sh
cargo test -p sim                                        # sim unit tests
cargo test -p client                                     # client's native-testable modules
cargo test -p sim --target wasm32-wasip1                # sim tests under wasm (needs wasmtime); proves GOLDEN matches native
cargo build -p client --target wasm32-unknown-unknown    # type-check/build the client
trunk serve                                              # dev server at http://127.0.0.1:8090, rebuilds on save
trunk build --release                                    # production build into dist/
```

## Notes

- Debugging: the seed is logged to the browser console on load, and `?seed=N` in the URL replays that field. The backquote key toggles a readout (`debug.rs`, the `#debug` element in `index.html`) with the seed, ship position, camera center, map size and FPS.
- `Trunk.toml` at the repo root points trunk at `crates/client/index.html` and watches all of `crates/`, so run trunk from the root.
- The ship is pixel art defined as text in `crates/client/src/sprite.rs`. The shader draws it on a 3 px grid with the angle snapped to 36 steps (Escape Velocity style), and it's upscaled with Scale2x before rotating so diagonals stay clean.
- Shaders: `common.wgsl` (view uniform, chunky-pixel grid, angle snapping, hash) is prepended to each other shader with `concat!`, and `heat.wgsl` (fire colors, `heat_color`) after it for anything that burns or glows. Shaders never see map coordinates: each frame the camera (`camera.rs`, a swappable `Camera` trait; today `ShipCamera` keeps the ship centered) gives a view with a continuous center that never wraps (ADR 0004), and `copies.rs` works out which wrapped copies of each object overlap the view and their positions relative to its center. Ships and rocks are both drawn as instances of those copies. Each frame draws the world blended between the last two ticks (`interpolate.rs`, ADR 0005), so motion is smooth on screens faster than 60 Hz; the camera follows the blended ship, and what's shown is up to one tick behind the sim on purpose. `cargo test -p client` also compiles every shader with naga, so shader errors show up without a browser.
- The starfield (`stars.wgsl`) is a full-screen pass drawn first; stars come from hashing art-grid cells, so there's no star data anywhere. It's a separate layer that scrolls at `PARALLAX` (0.5) of the camera's continuous center, so it never shows the map's seam; its pattern repeats every `STAR_PERIOD` art pixels so the scroll offset sent to the GPU stays small.
- Asteroids are drawn in `asteroid.wgsl` from the sim's outline (passed as instance data, so the drawn edge is the collision edge), their `look` seed (palette, craters) and their class (`surface_rules()` picks which class's ridge/crater rules apply; Large borrows Big's for now). The instance already uses 13 of WebGL2's 16 vertex attributes, so a longer outline needs another way in (e.g. a data texture). Unlike the ship they rotate smoothly, not in 36 steps, so the drawn outline never drifts from the collision outline. Because of that, shading uses solid bands with no dither or per-pixel texture, and no detail smaller than a couple of art pixels: anything finer flickers as the rock turns. Most rocks get ridges (`ridges()`): a few crests at their own angles, mostly curved (arc and/or S-bend; only an extra ridge may run straight), that tilt the surface normal, each with a steep side that reads as a crease, splitting the rock into faces. About a third of medium and big rocks get craters (small ones none), usually 2 or 3; a lone crater only happens when a second won't fit.
- Bolts are drawn by `bolt.wgsl` as smooth glowing streaks added on top of everything (additive blending, off the art grid), with each weapon's look in `BoltLook` in `gfx.rs`. `effects/shots.rs` throws a muzzle flash for each new bolt and a burst of sparks at each hit, styled per weapon by `ShotStyle`. The J key shoots; `web.rs` latches a press until a tick has seen it, and gathers every stepped tick's hits for the effects, since a frame can run several ticks or none.
- Fire (ADR 0012) is purely visual and lives in `effects/`. Each effect is an `Emitter` throwing CPU particles into one shared pool (`particles.rs`, drawn by `particle.wgsl`). `fire.rs` holds one `Fire` per burner and mount: a `Pose` (position, velocity, angle, spin) plus a `Mount` (nozzle offset, direction, `FireStyle`, `Drive`), so it never knows what's burning; `burners()` is the only place that says ships have an engine. Intensity eases toward the drive. The plume's tail (`lag`) is a critically damped spring chasing the burner's spin, so it trails into a turn and eases back without overshooting when the turn stops; `fire.wgsl` bends the plume by it. Sparks and embers are thrown from across the nozzle's width, leaning outward from where they left it like the flame fans, with the nozzle's real velocity (the burner's, plus its swing as it turns), strewn along the nozzle's path through the frame, so a turn sweeps the spray into an arc. The faster the burner goes (`Controls::pace`, its speed over `sim::MAX_SPEED`), the longer they last and the hotter they burn (`life_at_speed`, `heat_at_speed`). New kinds of fire are new `FireStyle` constants and mounts. The sim knows nothing about any of it.
- `wgpu` changes its instance/adapter/device/surface API between releases. Check the installed version's source or docs before editing `crates/client/src/gfx.rs`.
