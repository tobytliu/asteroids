# The sim is bit-exact across native and wasm

Given the same seed and the same inputs, the sim produces bit-identical worlds on every machine, whether it runs natively (server) or in wasm (browser). This is close to free when designed in, and painful to retrofit. It makes replays exact, lets client and server compare checksums to detect drift, and keeps lockstep open as an option.

Plain float arithmetic and square roots are already identical everywhere, but sine, cosine and other transcendental functions come from each platform's own maths library and can differ in the last bit. Over many ticks that difference grows. So the sim gets all of those from the pure-Rust `libm` crate, through one module, never from `std`.

## Consequences

The ECS ([ADR 0001](0001-ecs-with-hecs.md)) chooses its own storage order, so determinism has to be kept on purpose. These rules are listed in CLAUDE.md and apply to every sim change:

1. One thread, and the step runs its systems in an order written out in code.
2. Query order is only relied on where order can't affect the result. Anything order-sensitive (sequential collision resolution, random draws, spawning) first sorts by entity ID.
3. Entity IDs come from a world counter and are never reused.
4. Spawns and despawns are queued and applied at one fixed point in the tick, in entity ID order.
5. Randomness comes only from the world's own generator. Nothing in the sim iterates a `HashMap` or `HashSet`.
6. The sim reads no clock, frame time or environment.
7. A test runs the same seed and scripted inputs twice and asserts identical world checksums at every tick.
