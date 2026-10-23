# Asteroids

A multiplayer space game: ships fly a wrapped map of drifting, colliding asteroids, simulated in fixed ticks so every machine agrees on what happened.

## Space and time

**World**:
The complete simulation state at one tick: every entity, the map's size, the tick count and the world's random generator.
_Avoid_: Game, level, state (on its own)

**Map**:
The playfield the world takes place in: a fixed-size rectangle whose opposite edges are joined, so leaving one side enters the other.
_Avoid_: Canvas, screen, playfield, arena

**Seam**:
Where opposite edges of the map meet. Nothing stops at a seam or treats it as special; distances and collisions are measured across it.
_Avoid_: Border, wall, edge (when meaning the wrap)

**Tick**:
One fixed step of the simulation, 1/60 of a second. All game time is counted in ticks.
_Avoid_: Frame (a frame is a render, not a tick), update

**Snapshot**:
The world at one tick written out as plain data: every entity's state, in entity ID order. It's what clients draw from, and what a server will send.
_Avoid_: State dump, frame

**Spawn point**:
Where a new ship joins the map, if nothing is near it; otherwise the ship goes somewhere clear. The starting asteroids leave room around it.
_Avoid_: Start, origin (the origin is just the map's center)

**Seed**:
The number a world's map and starting entities are generated from. The same seed always produces the same starting world.

## Entities

**Entity**:
Anything that exists in the world and can be told apart from everything else: a ship, an asteroid, a station.
_Avoid_: Object, actor, game object

**Entity ID**:
An entity's permanent identity, assigned by the world when it is created and never reused. Every machine, message and saved state refers to entities by their entity ID.
_Avoid_: Handle, index, key

**Ship**:
A controllable craft that moves only through the inputs it is given. A ship does not know whether a person or an AI is flying it.
_Avoid_: Player (a player flies a ship but is not one), craft

**Asteroid**:
A drifting body of rock with its own outline, spin and class. "Rock" is fine as a casual short form.
_Avoid_: Meteor, boulder

**Class**:
An asteroid's size category (Small, Medium, Big, Large), which sets its size range, motion and surface features.
_Avoid_: Size, tier, type

**Group**:
A set of asteroids that start together and stay together, drifting as one loose body because its members are drawn toward each other. An asteroid belongs to at most one group, for its whole life.
_Avoid_: Cluster, belt, swarm, flock

**Swish**:
The shape a group starts in: a curved band that tapers to points at both ends, lying at any angle.
_Avoid_: Arc, streak, crescent, line

**Density**:
How tightly a group is packed: dense, moderate or sparse.
_Avoid_: Tier, level, high/medium/low (medium is a class)

**Standalone asteroid**:
An asteroid that belongs to no group.
_Avoid_: Loner, stray, free rock

**Lane**:
Open space between groups that a ship can fly through at speed. Lanes are the gaps groups leave, not paths in their own right, so they move, open and close as groups drift.
_Avoid_: Corridor, channel, path

**Outline**:
The exact edge of an asteroid. What is drawn is the outline, and what collides is the outline; there is no separate hitbox.
_Avoid_: Hitbox, shape (when meaning the edge), mesh

**Station**:
A large structure fixed in the map that other entities collide with. Not yet in the game.
_Avoid_: Base, outpost

## Control

**Input**:
What a ship is told to do for one tick: thrust, turn left, turn right. Inputs are the only way anything steers a ship.
_Avoid_: Command, action, controls

**Controller**:
Whatever produces a ship's inputs: the local keyboard, a remote player or an AI. Controllers live outside the simulation and all speak in inputs.
_Avoid_: Brain, driver, agent

## Seeing the world

**View**:
The part of the map shown on screen this frame, set by a camera as a center and a zoom.
_Avoid_: Viewport, screen, canvas

**Camera**:
Decides the view each frame by watching the world. A camera only affects what is shown, never what happens.
_Avoid_: Viewport

**Focus**:
The ship a camera is told to watch, usually the local player's own ship.
_Avoid_: Target (reserved for aiming), player ship

**Art pixel**:
One cell of the chunky grid everything is drawn on, several screen pixels wide.
_Avoid_: Pixel (on its own, which is ambiguous with screen pixels)
