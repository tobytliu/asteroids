# The camera lives in the client, and its position never wraps

Cameras are client-only and swappable behind one interface: each frame a camera watches the world and its focus ship, and gives back a view (a center and a zoom). The sim and server know nothing about cameras.

Unlike every entity, the camera's center is a continuous position that is never wrapped into the map. Each frame it moves by the focus ship's *wrapped* movement, so it can drift far outside the map's coordinates. The starfield, which isn't tied to the map and doesn't repeat, is drawn from that continuous position, so it never jumps when the ship crosses the seam. Future cameras can ease toward their focus without handling the seam themselves.

## Consequences

- Entities are drawn at their wrapped offset from the camera. The client works out on the CPU which wrapped copies of each entity overlap the view (usually one, none for anything off-screen) and sends camera-relative positions to the GPU. The shaders never see map coordinates or draw copies themselves.
- If the focus ship doesn't exist, the camera holds its last position.
- The view has a zoom from the start, though the first camera keeps it at 1. A later camera can use it to give every player the same visible area regardless of window size.
- The camera's center is rounded to whole device pixels before drawing, so the starfield's art pixels stay crisp while scrolling.
