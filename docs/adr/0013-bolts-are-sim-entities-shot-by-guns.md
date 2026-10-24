# Bolts are sim entities shot by guns, and hits are reported in the snapshot

Ships shoot bolts: entities in the sim with a `Body` and a `Bolt`, flying straight until they hit something or burn out. A ship's `Gun` turns its trigger into shots. The input only says whether the trigger is held (`Input::shoot`), and the gun tells a press from a hold itself. What a weapon does is all in `WeaponKind::params`: its trigger (single shot or auto), cooldown, muzzle speed, lifetime and reach. So a new weapon is a new kind, and the client picks its look by the same kind. A bolt's path each tick is swept against every rock outline and every ship but its own, so a fast bolt can't slip past something small. The first thing it touches stops it, and that's recorded as a hit in that tick's snapshot (`Snapshot::hits`). The client draws impacts from hits and never guesses them. Bolts are the first thing spawned during a tick, so they bring the spawn/despawn queue that determinism rule 4 asks for. It's applied once at the end of the tick, and new bolts take part from the next tick.

## Considered Options

- **Bolts as a client effect, like fire (ADR 0012).** They'd be cheap, but as soon as a bolt hits anything the client would be deciding what happened in the game.
- **A one-tick "shoot now" input from controllers.** Every controller would have to detect presses itself, and a repeated or dropped network input would add or lose a shot. Holding state in the input and edges in the gun avoids both.
- **The client inferring a hit when a bolt vanishes early.** It can't tell where along the last tick's path the bolt hit, and it would break as soon as bolts can vanish for other reasons.

## Consequences

- Bolts move only by their velocity. Nothing pushes them, and they don't push what they hit. Damage and knockback, when they come, are new sim state.
- Hits only last for the tick that made them. A client stepping several ticks in one frame must gather every tick's hits itself (web.rs does).
- Gun state (cooldown, whether the trigger was held) isn't in the checksum yet, so `GOLDEN` stayed as it was. Bolts and hits are, and a second scripted run that shoots has its own pinned checksums (`GOLDEN_SHOOTING`).
