# The world is an ECS, built on hecs

The world stores entities in an entity-component system (`hecs`) rather than one typed list per kind of entity. We expect many entity types that share traits in mixed ways (ships, AI ships, asteroids, stations, then projectiles, debris, pickups), and an ECS lets a new type be mostly a new mix of existing components rather than new code for every pairing.

## Considered Options

- **Typed lists** (one list each of ships, asteroids, stations, each its own struct). Easiest to read, and determinism is obvious. Rejected because shared behavior gets rewritten for every pair of types as the type count grows, and migrating to an ECS later would rewrite all the physics anyway.
- **`bevy_ecs`**. Rejected because its scheduler runs systems in parallel and orders them itself. We would spend effort switching that off to keep the sim deterministic, and it brings far more than we need.

## Consequences

- `hecs` only stores entities and runs queries. The world's step calls each system itself, in an order written out in code (see [ADR 0002](0002-bit-exact-determinism.md)).
- `hecs`'s own entity handles never leave the sim. Everything outside it refers to entities by entity ID.
- The current physics (asteroid collisions measured along their outlines, spin transfer, repeated ship-collision passes) is reshaped into systems.
