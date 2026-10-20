# The map is a fixed size and wraps

The map is a fixed-size rectangle whose opposite edges join, independent of the canvas size. The window only decides how much of the map is in view. Wrapping keeps the collision, distance and spawning code we already have (which measures across the seam), needs no boundary physics, and keeps netcode simple. Once the map is several views wide, players rarely notice the seam.

## Considered Options

- **Walls at the edges.** Needs new boundary collisions, and drifting asteroids pile up against the walls.
- **Unbounded or infinite map.** Needs entities generated in chunks and loaded or unloaded by distance, which is a much bigger project and harder to keep deterministic.

## Consequences

- The whole map is simulated every tick, including what nobody can see.
- Asteroids are spawned to a density per class, so how full the map looks doesn't depend on its size.
- Anything comparing two positions must use the wrapped difference. That includes the client's interpolation and camera.
