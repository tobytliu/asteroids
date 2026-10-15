// Positions are in CSS px, origin at the canvas center, +y up.
struct Ship {
    pos: vec2f,
    angle: f32,
    // Engine flame intensity, 0 (off) to 1 (full).
    flame: f32,
    view_size: vec2f,
    fb_size: vec2f,
    // Seconds, for animation.
    time: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
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

// The flame animates at a low frame rate for a choppy 8-bit flicker.
const FLAME_FPS: f32 = 12.0;
// Longest flame and ember trail behind it, in sprite pixels.
const FLAME_LEN: f32 = 16.0;
const EMBER_LEN: f32 = 14.0;
// How fast embers stream away from the flame, in sprite pixels per second.
const EMBER_SPEED: f32 = 30.0;

// Flame colors, from the same palette as sprite.rs.
const WHITE = vec3f(0.957, 0.957, 0.957);
const YELLOW = vec3f(1.0, 0.804, 0.459);
const ORANGE = vec3f(0.937, 0.490, 0.341);
const RED = vec3f(0.694, 0.243, 0.325);
const PURPLE = vec3f(0.365, 0.153, 0.365);

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    // A square around the ship, big enough to hold the sprite at any rotation
    // plus the longest flame and ember trail.
    let size = vec2f(textureDimensions(sprite)) / UPSCALE;
    let reach = size.y * 0.5 + FLAME_LEN * 1.2 + EMBER_LEN + 2.0;
    var corners = array<vec2f, 6>(
        vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
        vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0),
    );
    let p = ship.pos + corners[i] * reach * PIXEL;
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

    // Ship-local position in sprite pixels: +forward toward the nose, +left to port.
    let step = TAU / ROTATIONS;
    let a = round(ship.angle / step) * step;
    let forward = dot(d, vec2f(cos(a), sin(a))) / PIXEL;
    let left = dot(d, vec2f(-sin(a), cos(a))) / PIXEL;

    // The sprite is drawn nose-up: image up is forward, image left is the ship's
    // left. The small bias keeps samples off exact texel edges at right angles.
    let size = vec2f(textureDimensions(sprite)) / UPSCALE;
    let texel = vec2f(size.x * 0.5 - left, size.y * 0.5 - forward) + 1e-3;
    if all(texel >= vec2f(0.0)) && all(texel < size) {
        let color = textureSampleLevel(sprite, sprite_sampler, texel / size, 0.0);
        if color.a >= 0.5 {
            return vec4f(engine_glow(color.rgb), 1.0);
        }
    }

    let fire = flame(-forward - size.y * 0.5, left);
    if fire.a < 0.5 {
        discard;
    }
    return fire;
}

// The sprite's engine pixel flashes hot while the flame is lit.
fn engine_glow(rgb: vec3f) -> vec3f {
    if ship.flame < 0.05 || distance(rgb, ORANGE) > 0.02 {
        return rgb;
    }
    let frame = floor(ship.time * FLAME_FPS);
    return select(YELLOW, WHITE, hash(vec3f(1.0, 2.0, frame)) > 0.4);
}

// The exhaust at a point `behind` sprite pixels behind the tail and `left` of
// the centerline. Returns transparent where there is none.
fn flame(behind: f32, left: f32) -> vec4f {
    if ship.flame < 0.02 {
        return vec4f(0.0);
    }
    let frame = floor(ship.time * FLAME_FPS);
    let cell = floor(vec2f(left, behind));

    // The whole flame surges and shrinks a little every frame.
    let len = ship.flame * FLAME_LEN * (0.75 + 0.45 * hash(vec3f(0.0, 0.0, frame)));
    // 0 at the nozzle, which is tucked just inside the tail, to 1 at the tip.
    let t = (behind + 1.5) / len;
    if t >= 0.0 && t < 1.0 {
        let n = hash(vec3f(cell, frame));
        // Nozzle-wide at the start, billowing out, then tapering to a point,
        // with ragged edges.
        let bloom = 1.6 + 1.6 * smoothstep(0.0, 0.25, t);
        let half_width = bloom * sqrt(1.0 - t) * min(1.0, ship.flame * 1.5) + (n - 0.5) * 0.9;
        if abs(left) < half_width {
            // Hottest along the centerline near the nozzle.
            let heat = 1.15 - t - 0.45 * abs(left) / max(half_width, 0.1) + (n - 0.5) * 0.3;
            return vec4f(heat_color(heat), 1.0);
        }
    }

    // Embers stream back from the tip, thinning out as they go.
    let e = behind + 1.5 - len * 0.6;
    if ship.flame > 0.3 && e >= 0.0 && e < EMBER_LEN * ship.flame {
        let fade = e / (EMBER_LEN * ship.flame);
        let spread = 1.5 + e * 0.35;
        let streaming = vec3f(cell.x, floor(behind - ship.time * EMBER_SPEED), 7.0);
        if abs(left) < spread && hash(streaming) > 0.8 + 0.17 * fade {
            return vec4f(select(ORANGE, YELLOW, fade < 0.35), 1.0);
        }
    }
    return vec4f(0.0);
}

fn heat_color(heat: f32) -> vec3f {
    if heat > 0.85 {
        return WHITE;
    } else if heat > 0.62 {
        return YELLOW;
    } else if heat > 0.38 {
        return ORANGE;
    } else if heat > 0.1 {
        return RED;
    }
    return PURPLE;
}

// A pseudo-random value in [0, 1) for each input (Dave Hoskins' hash13).
fn hash(p: vec3f) -> f32 {
    var q = fract(p * vec3f(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}
