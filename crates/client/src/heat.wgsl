// Fire's colors, shared by everything that burns or glows: the ship's engine
// pixel, plumes and sparks. Prepended after common.wgsl in gfx.rs.

// Fire animates at a low frame rate for a choppy 8-bit flicker.
const FLAME_FPS: f32 = 12.0;

// Saturated and brighter than the sprite palette so fire reads as glowing.
const FIRE_WHITE = vec3f(1.0, 1.0, 0.88);
const FIRE_YELLOW = vec3f(1.0, 0.9, 0.2);
const FIRE_ORANGE = vec3f(1.0, 0.55, 0.05);
const FIRE_RED = vec3f(1.0, 0.24, 0.1);
const FIRE_EDGE = vec3f(0.85, 0.08, 0.38);

// The color of something `heat` hot: 1 is white-hot, 0 barely glowing.
fn heat_color(heat: f32) -> vec3f {
    if heat > 0.8 {
        return FIRE_WHITE;
    } else if heat > 0.56 {
        return FIRE_YELLOW;
    } else if heat > 0.32 {
        return FIRE_ORANGE;
    } else if heat > 0.1 {
        return FIRE_RED;
    }
    return FIRE_EDGE;
}
