# Roadmap

What's built, what's next, and what's still undecided. Decisions themselves live in [ADRs](adr/); this file tracks the work. Update it when something lands or the plan changes.

## Done

- **Fixed wrapped map** (ADR 0003): 4000×4000, rocks spawned by density per class.
- **Swappable camera** (ADR 0004): a `Camera` trait with a ship-centered camera. Its center is continuous and never wraps; wrapped copies are placed on the CPU; the starfield is a parallax layer.
- **Render interpolation** (ADR 0005): frames are drawn blended between the last two snapshots, by entity ID.
- **Debug tools**: `?seed=N` replays a field, and the backquote key shows a readout.
- **Bit-exact determinism** (ADR 0002): `libm` maths, checksums, and a golden record that passes natively and under wasm.
- **ECS** (ADR 0001): hecs with entity IDs, multiple ships with inputs keyed by ship, ship–ship collisions, and snapshots as the client's only view of the sim.

## Next

Roughly in order. Each item lists what it needs first and what's still open.

### 1. AI ships

Controllers that fly ships by producing inputs, in a new `ai` crate that depends on `sim` (ADR 0006).

- **Needs**: nothing more; multiple ships and snapshots are in place.
- **Scope**: start with a wanderer that flies around and steers away from rocks. Spawn a few in the browser to exercise multiple ships, their flames and ship–ship collisions.
- **Open**:
  - What does an AI see: a snapshot, or something narrower?
  - How is an AI ship marked: an `ai` component in the sim, or just a list of IDs the AI crate owns? ADR 0006 says the sim never knows which ships are human, which points to the list.
  - AI must follow the determinism rules, so it takes randomness from a seeded generator of its own and never reads the clock.

### 2. Stations

Large structures fixed in the map (see the glossary).

- **Needs**: nothing.
- **Open** (needs a grilling round and probably an ADR):
  - What are stations for: docking, respawning, trading, shelter?
  - Are they immovable bodies? That was the tentative idea: rocks and ships bounce off them and they don't move.
  - Collision shape: circle, outline, or polygon?
  - How many are there, where are they placed (seeded?), and must they stay clear of rocks at spawn?
  - How are they drawn: sprite or procedural?

### 3. Netcode

A native authoritative `server` crate, with client prediction and reconciliation and snapshot interpolation for remote entities (ADR 0007, still **proposed**: settle it before starting).

- **Needs**: probably AI first, so a server has something to run with nobody connected.
- **Open**:
  - Accept or replace ADR 0007.
  - Transport: WebSocket first? WebRTC data channels later for unreliable delivery?
  - Snapshot encoding and size: split each rock's fixed data (outline, class, look), sent once, from its moving state, sent every tick. Consider deltas against the last acknowledged snapshot.
  - Tick and send rates; the input delay and buffering on the server.
  - How the client rewinds and replays when it mispredicts. The sim needs a way to restore a world from a snapshot.
  - Joining and leaving: who calls `spawn_ship`/`remove_ship`, and how a joining client gets its ship's ID.
  - Desync detection: compare checksums between client and server.

### 4. Camera extras

- **A second camera** (for example, one that eases toward the ship or looks ahead of it). Then add the deferred runtime swap key.
- **Zoom**: the view already carries a zoom. A camera could pick it so every player sees the same area of the map whatever their window size, which is fairer in multiplayer.

## Known gaps

Small things that are wrong or missing today.

- **The world has no tick count.** The glossary's *World* includes one, but `sim::World` doesn't track it. Netcode will need one to name snapshots and inputs.
- **The checksum skips the RNG and the ID counter.** It covers every entity's state, but not the world's random generator or its next ID. Two worlds could match on checksum and still diverge at the next random draw or spawn. Add them the next time `GOLDEN` is deliberately re-pinned.
- **No in-tick spawn queue.** Determinism rule 4 asks for spawns and despawns queued and applied at one point in ID order. Nothing spawns during a tick yet, so the queue doesn't exist. Build it with the first thing that does (bullets, rock splitting).
- **Rocks only see the first 16 vertex attributes** on WebGL2, and the rock instance already uses 13 of them. A longer outline or more per-rock data needs another way in, such as a data texture.

## Later, when needed

- **Broadphase** for collisions (a spatial grid). Today's checks compare every pair, which is fine at around 144 rocks but grows with the square of the count. Do this before raising rock counts or map size much.
- **Deterministic parallelism** (ADR 0008, proposed): only if profiling shows one sim system needs more than one core after a broadphase.
