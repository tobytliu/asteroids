---
status: proposed
---

# Multiplayer is server-authoritative with client prediction

A native server runs the one authoritative world. Each client predicts its own ship by running the sim locally on its own inputs straight away, so controls feel instant. When the server's state arrives and disagrees, the client rewinds to it, replays its unconfirmed inputs, and eases out the visual difference. Other ships and asteroids are drawn slightly in the past, blended between server snapshots.

This is proposed, not decided: it was assumed during the camera design and is to be revisited when networking work starts. [ADR 0002](0002-bit-exact-determinism.md), [0005](0005-render-interpolation.md) and [0006](0006-controllers-speak-inputs.md) were written with it in mind.

## Considered Options

- **Deterministic lockstep** (every client simulates everything and only inputs are sent). Bit-exact determinism ([ADR 0002](0002-bit-exact-determinism.md)) keeps it possible, but the game waits on the slowest connection and late joiners need the full state.
