# Groups are small swishes whose members push apart inside their spacing

Supersedes part of [ADR 0010](0010-groups-held-by-gravity.md): gravity between members still holds a group together, but members closer than their group's spacing now push each other apart instead of feeling nothing.

Big groups read as blobs, so the field is now many small groups (about 5 rocks each) that start strung along a swish: a curved band tapering to points, at a random angle. Every large rock is standalone, because one in a group made the swish too wide to read, and so is every small rock, as loose debris. Groups of medium and big rocks are much lighter than ones anchored by a large rock, so gravity is ten times stronger. ADR 0010's cutoff stopped gravity between two members inside their spacing, but the rest of the group still pulled each of them inward, so groups squeezed into touching clumps. A push that grows as members close in gives each group a resting spacing.

## Considered Options

- **Gravity only, tuned.** Weak gravity let groups of medium rocks drift apart; strong gravity crushed them together. No single strength did both.
- **A spring toward each member's place in the swish.** It would keep the swish shape for good, but members steer themselves back as if on rails, the look ADR 0010 rejected.

## Consequences

- A swish only lasts at the start. Gravity pulls every group toward round, so after a few minutes groups are small loose clusters.
- The push is gentle (`PUSH`, as a multiple of gravity's strength). Much stronger and a group that starts packed tighter than its spacing throws its members out.
