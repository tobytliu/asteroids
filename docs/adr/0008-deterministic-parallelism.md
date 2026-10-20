---
status: proposed
---

# A sim system may go parallel only if its result can't depend on thread timing

[ADR 0002](0002-bit-exact-determinism.md) keeps the sim on one thread so that every operation happens in the same order on every run. At this game's scale one core is plenty, and the first limit we'll hit is the O(n²) collision check, which a broadphase fixes without threads. This ADR only matters if profiling later shows that one sim system, even after better algorithms, needs more than one core.

If that happens, that one system may split its work across threads, as long as no result depends on which thread finishes first:

- **Parallel compute, ordered apply.** Each entity's result is computed on its own and written to that entity's own slot. Nothing is changed until all the threads finish. Then the results are applied in entity ID order.
- **Parallel search, sequential resolve.** Order-independent searching, such as finding candidate collision pairs, can run in parallel. Its output is sorted by entity ID and resolved on one thread.
- **Independent groups.** Work can be split into groups where no two items touch the same entity, as in graph colouring for constraint solving. Each group runs in parallel, and the groups run one after another in a fixed order.

Systems still run one after another in the order written in the step. Only the inside of a system goes parallel.

This is proposed, not decided. Nothing needs it today. It records the planned way out so the single-thread rule isn't dropped wholesale when performance pressure comes.

## Considered Options

- **A parallel ECS scheduler** (e.g. bevy's). Systems with no conflicting data access run in an undefined order. Keeping that safe means removing every ambiguity and keeping them removed as systems are added. [ADR 0001](0001-ecs-with-hecs.md) rejected this.
- **Dropping determinism and relying on the server as the source of truth.** This is common in shooters. We'd lose exact replays, checksum drift detection, cheap reconciliation and the lockstep option.
- **Scaling by running one world per core instead.** This is how the server scales across matches anyway. It does nothing for a single world that is too slow.

## Consequences

- Adopting this supersedes rule 1 in [ADR 0002](0002-bit-exact-determinism.md) and in CLAUDE.md. Rule 1 would be reworded to "systems run in written order; a system may parallelise only in the ways ADR 0008 lists."
- No floating-point sums or other reductions across threads unless they are combined in a fixed order. Float addition isn't associative.
- No random draws inside the parallel part. If a parallel step needs randomness, draw the numbers beforehand in entity ID order and hand them in.
- No spawns or despawns inside the parallel part. They are queued as usual (rule 4).
- The browser may not have threads (wasm threads need `SharedArrayBuffer` and cross-origin isolation headers). Every parallel system must give bit-identical results when it runs on one thread, and the determinism test must compare the two.
