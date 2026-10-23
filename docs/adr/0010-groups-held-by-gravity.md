# Asteroid groups hold together by gravity between their own members

The field is laid out as groups of asteroids at different densities, with open lanes between them that you can fly at speed, plus a few standalone rocks. Rocks drift forever and bounce off each other, so a layout that only existed at spawn would turn back into uniform scatter within a minute or two. Each group therefore starts with one shared velocity, and its members attract each other with a weak gravity. A group then holds together and moves as a loose body for the whole session, and a rock knocked out hard enough really escapes. Membership is fixed at spawn as a component. Standalone rocks don't have it, so gravity never touches them.

Gravity acts only between members of the same group. It's softened, does nothing inside about a group's natural spacing, and fades to zero by around 1000 px. Gravity between groups or toward standalone rocks would be more realistic, but it would slowly merge everything and close the lanes. Without the inner cutoff, energy lost in collisions would let every group settle into a touching rubble pile. Without the outer fade, gravity would reach across the map's wrapped edges ([ADR 0003](0003-wrapped-fixed-size-map.md)). Ships don't feel it.

## Considered Options

- **Structure only at spawn.** This costs nothing, but it's gone within a minute.
- **Easing each member toward the group's velocity.** This is predictable, but a bounced rock visibly steers itself back as if it had thrusters.
- **One drift for the whole field.** Lanes would never close, but the whole map slides past the stars like a river.
- **Pulling members toward the group's center.** Needs per-group state and an average position on a wrapped map, and it looks like rocks on rails.

## Consequences

- Each rock carries its own speed floor. A group member uses its group's spawn speed rather than its class's, otherwise members of different classes would be pushed to different speeds and the group would tear apart.
- Lanes open and close as groups with different velocities pass each other. That's intended.
- Membership never changes, so nothing in `step` has to recalculate groups.

## Status

Partly superseded by [ADR 0011](0011-small-swish-groups-that-push-apart.md): members closer than their spacing now push apart.
