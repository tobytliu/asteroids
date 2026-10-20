# The client draws between the last two ticks

The client keeps the previous tick's world as well as the current one, and draws every entity blended between them by how far it is into the next tick. Without this, screens faster than 60 Hz show motion as step-pause-step. With a camera locked to the ship, the whole view stutters, not just the ship. The cost is that what's shown is up to one tick (about 16 ms) behind the sim. That delay is deliberate; don't "fix" it.

## Considered Options

- **Stepping the sim by frame time.** Breaks determinism ([ADR 0002](0002-bit-exact-determinism.md)).
- **A higher tick rate.** Only hides the stutter (144 Hz doesn't divide evenly into 120 or 240), multiplies sim and server cost, and raises network send rates.
- **Smoothing only the camera.** The background goes smooth while ships and asteroids still step, which makes their stepping stand out more.
- **Extrapolation** (drawing ahead along each entity's velocity). No delay, but it's wrong whenever something turns or collides, and the next tick visibly snaps it back.

## Consequences

- Blends match entities by entity ID, take the shortest way round for angles and across the seam, and skip entities that appeared or jumped this tick.
- The wrap-aware blend helpers are pure maths, so they live in `sim` with unit tests. Deciding what to blend stays in the client.
- This is the same mechanism netcode will use to draw remote entities between server snapshots ([ADR 0007](0007-server-authoritative-netcode.md)).
