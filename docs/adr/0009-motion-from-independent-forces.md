# Motion comes from independent force systems and one integrator

Apart from collisions, nothing changes a body's velocity directly. Each effect on motion, such as group gravity ([ADR 0010](0010-groups-held-by-gravity.md)) or later a gravity well, shockwave or tractor beam, is its own system in `step`. It knows nothing about the others and only adds to the body's acceleration for this tick. One integrate step then applies the total and clears it, and collisions follow as impulses, the same as today. A new effect is one new system, and it never edits drift, collisions or the other effects. Which bodies feel an effect is chosen by ECS components, not by flags or branches: a system queries only the bodies that have the component it acts on.

## Considered Options

- **Each effect edits velocity in place** (how `drift` worked). This is simpler at first, but every new effect has to know where it sits relative to the others, and effects that should add up end up overwriting each other.
- **A general physics engine.** This brings far more than an asteroid field needs, and it's hard to keep bit-exact ([ADR 0002](0002-bit-exact-determinism.md)).

## Consequences

- Determinism: each body's forces are summed in the order the systems are written in `step`. A force that depends on pairs of bodies goes through them in entity ID order (determinism rule 2), because float addition depends on order.
- The speed floor that keeps rocks moving is a constraint, not a force: it tops up speed without changing heading. It stays its own system and may set velocity directly, at a fixed point in the step.
- The move to this structure is a pure refactor and must leave `GOLDEN` unchanged.
