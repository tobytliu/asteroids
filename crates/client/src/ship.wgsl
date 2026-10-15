// Positions are in CSS px, origin at the canvas center, +y up.
struct Ship { pos: vec2f, angle: f32, _pad: f32, view_size: vec2f, fb_size: vec2f };
@group(0) @binding(0) var<uniform> ship: Ship;
@group(0) @binding(1) var sprite: texture_2d<f32>;
@group(0) @binding(2) var sprite_sampler: sampler;

// CSS px per sprite pixel.
const PIXEL: f32 = 3.0;
// The texture is the sprite upscaled this many times (see sprite.rs).
const UPSCALE: f32 = 8.0;
// Rotation is drawn in discrete steps, like a pre-rendered sprite sheet.
const ROTATIONS: f32 = 36.0;
const TAU: f32 = 6.2831853;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    // A square around the ship, big enough to hold the sprite at any rotation.
    let radius = (length(vec2f(textureDimensions(sprite)) / UPSCALE) * 0.5 + 1.0) * PIXEL;
    var corners = array<vec2f, 6>(
        vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
        vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0),
    );
    let p = ship.pos + corners[i] * radius;
    return vec4f(p / (ship.view_size * 0.5), 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    // Fragment position in world px (frag.xy is in framebuffer pixels, y down).
    let css = frag.xy * ship.view_size / ship.fb_size;
    let world = vec2f(css.x - ship.view_size.x * 0.5, ship.view_size.y * 0.5 - css.y);

    // Snap to a screen-aligned grid of big pixels that moves with the ship, so
    // the sprite stays blocky at every angle and doesn't shimmer as it flies.
    let d = (floor((world - ship.pos) / PIXEL) + 0.5) * PIXEL;

    let step = TAU / ROTATIONS;
    let a = round(ship.angle / step) * step;
    let forward = dot(d, vec2f(cos(a), sin(a)));
    let left = dot(d, vec2f(-sin(a), cos(a)));

    // The sprite is drawn nose-up: image up is forward, image left is the ship's
    // left. `texel` is in sprite pixels; the small bias keeps samples off exact
    // texel edges at right angles.
    let size = vec2f(textureDimensions(sprite)) / UPSCALE;
    let texel = vec2f(size.x * 0.5 - left / PIXEL, size.y * 0.5 - forward / PIXEL) + 1e-3;
    let color = textureSampleLevel(sprite, sprite_sampler, texel / size, 0.0);
    if any(texel < vec2f(0.0)) || any(texel >= size) || color.a < 0.5 {
        discard;
    }
    return color;
}
