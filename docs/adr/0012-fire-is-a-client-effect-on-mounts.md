# Fire is a client effect that attaches to anything through mounts

Fire (a ship's engine flame today, missiles and manoeuvring thrusters later) is drawn by the client as a plume plus sparks and embers, which are CPU particles in one pool that every effect shares. Each effect is an independent emitter, so a new effect is a new emitter and touches no other. Fire never knows what's burning: it reads a pose (position, velocity, angle, spin) and a mount (where the nozzle sits on the burner, which way it points, its style and what lights it). The only code that knows which entities have fires is one binding function. The motion is physics, not procedural pattern. Sparks are thrown from across the nozzle's width, leaning outward, with the nozzle's real velocity, including its swing as the burner turns, so a turn sweeps the spray into an arc. The plume's tail is a spring that chases the burner's spin, so it trails into a turn and eases back when the turn stops. The spring's damping is part of the style, so a fire can also whip past centre if that suits it.

## Considered Options

- **Keep fire procedural in the ship shader** (how it was). There's no state, but sparks can't keep the direction they were thrown in, so they can't fan out through a turn. Every new burner would also copy the flame into its own shader.
- **Simulate fire in `sim`.** The sim would then carry state that's only for show, and every tweak to a look would re-pin `GOLDEN`.

## Consequences

- Fire is visual only. When fire starts to hurt things, whether something burns and its damage zone become sim state, and the client draws the plume from that, so what's seen burning is what burns. The plume's spring and the sparks stay decoration.
- Effects use the client's own random generator, never the sim's, and step by frame time. They don't have to be deterministic.
- New kinds of fire are new `FireStyle` constants and mounts. A new way of lighting one is a new `Drive` variant.
