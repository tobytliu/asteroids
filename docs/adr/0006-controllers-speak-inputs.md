# Every controller speaks inputs; AI is its own crate

Ships are steered only by inputs, handed to the world's step one set per ship per tick. The local keyboard, a remote player and an AI are all controllers that produce the same inputs, so AI ships obey exactly the same physics as players and can't cheat, and the sim never knows which ships are human. A ship with no input for a tick coasts as if everything were released.

AI runs wherever the authoritative sim runs: the server in multiplayer, or the browser when playing offline. It therefore can't live in the client. It goes in its own `ai` crate that depends on `sim`, so the core physics stays small and AI can grow and be tuned separately. It must follow the sim's determinism rules ([ADR 0002](0002-bit-exact-determinism.md)).

## Considered Options

- **An AI module inside `sim`.** Simpler at first, but it mixes decision-making into the physics crate, and the server and every test would carry it.
