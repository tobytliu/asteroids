// The player ship: a pixel-art sprite plus a procedural engine flame.

// One ship, as a per-instance vertex.
struct ShipInstance {
    // Relative to the view's center.
    @location(0) pos: vec2f,
    // How far the sprite and flame can reach from `pos`, in px.
    @location(1) reach: f32,
    @location(2) angle: f32,
    @location(3) flame: f32,
    @location(4) turn: f32,
};

struct Ship {
    angle: f32,
    // Engine flame intensity, 0 (off) to 1 (full).
    flame: f32,
    // How fast the ship is turning, rad/s (+ is counterclockwise), smoothed.
    turn: f32,
};
// The ship this fragment belongs to, set at the top of `fs`.
var<private> ship: Ship;

@group(1) @binding(1) var sprite: texture_2d<f32>;
@group(1) @binding(2) var sprite_sampler: sampler;

// The texture is the sprite upscaled this many times (see sprite.rs).
const UPSCALE: f32 = 8.0;

// The flame animates at a low frame rate for a choppy 8-bit flicker.
const FLAME_FPS: f32 = 12.0;
// Longest flame and ember trail behind it, in sprite pixels. `SHIP_REACH` in
// gfx.rs must cover them, or the flame gets clipped.
const FLAME_LEN: f32 = 16.0;
const EMBER_LEN: f32 = 14.0;
// How fast embers stream away from the flame, in sprite pixels per second.
const EMBER_SPEED: f32 = 30.0;
// How fast exhaust leaves the nozzle, in sprite pixels per second. Lower makes
// the plume bend more when turning.
const EXHAUST_SPEED: f32 = 170.0;
// The most the plume's tail can lag behind the ship's heading, in radians.
const MAX_SWING: f32 = 1.0;

// Sparks fly out of the flame faster than embers, fanning outward as they go.
const SPARK_LEN: f32 = 14.0;
const SPARK_SPEED: f32 = 55.0;
// How far a spark drifts sideways per sprite pixel it travels back.
const SPARK_FAN: f32 = 0.5;

// The sprite's engine pixel color in sprite.rs, which engine_glow() lights up.
const ENGINE = vec3f(0.937, 0.490, 0.341);

// Flame colors: saturated and brighter than the sprite palette so the exhaust
// reads as glowing.
const FIRE_WHITE = vec3f(1.0, 1.0, 0.88);
const FIRE_YELLOW = vec3f(1.0, 0.9, 0.2);
const FIRE_ORANGE = vec3f(1.0, 0.55, 0.05);
const FIRE_RED = vec3f(1.0, 0.24, 0.1);
const FIRE_EDGE = vec3f(0.85, 0.08, 0.38);

struct VsOut {
    @builtin(position) clip: vec4f,
    // This copy's position relative to the view's center.
    @location(0) @interpolate(flat) center: vec2f,
    // (angle, flame, turn)
    @location(1) @interpolate(flat) params: vec3f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32, instance: ShipInstance) -> VsOut {
    var out: VsOut;
    out.center = instance.pos;
    out.clip = world_to_clip(out.center + object_corner(i, instance.reach));
    out.params = vec3f(instance.angle, instance.flame, instance.turn);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    ship = Ship(in.params.x, in.params.y, in.params.z);
    // Ship-local position in sprite pixels: +x toward the nose, +y to port.
    let local = to_local(snapped_offset(in.clip, in.center), snap_angle(ship.angle)) / PIXEL;
    let forward = local.x;
    let left = local.y;

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
    if ship.flame < 0.05 || distance(rgb, ENGINE) > 0.02 {
        return rgb;
    }
    let frame = floor(view.time * FLAME_FPS);
    return select(FIRE_YELLOW, FIRE_WHITE, hash(vec3f(1.0, 2.0, frame)) > 0.4);
}

// The exhaust at a point `behind` sprite pixels behind the tail and `left` of
// the centerline. Returns transparent where there is none.
fn flame(behind: f32, ship_left: f32) -> vec4f {
    if ship.flame < 0.02 {
        return vec4f(0.0);
    }
    // Exhaust keeps going the way the ship pointed when it left the nozzle, so
    // while turning the plume trails off in an arc. Gas `s` px back left
    // `s / EXHAUST_SPEED` seconds ago, when the ship was turned back by
    // `turn * s / EXHAUST_SPEED`; shift each row sideways by that much.
    let s = max(behind + 1.5, 0.0);
    let swing = clamp(ship.turn * s / EXHAUST_SPEED, -MAX_SWING, MAX_SWING);
    let left = ship_left - s * sin(swing);

    let frame = floor(view.time * FLAME_FPS);
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
        let streaming = vec3f(cell.x, floor(behind - view.time * EMBER_SPEED), 7.0);
        if abs(left) < spread && hash(streaming) > 0.8 + 0.17 * fade {
            return vec4f(select(FIRE_ORANGE, FIRE_YELLOW, fade < 0.35), 1.0);
        }
    }

    // Sparks: lone pixels flung back from mid-flame. Each one keeps its lane,
    // measured from a centerline that widens with distance, so it fans out as
    // it streams back, and it twinkles from frame to frame.
    let k = behind + 1.5 - len * 0.3;
    if ship.flame > 0.3 && k >= 0.0 && k < SPARK_LEN * ship.flame {
        let fade = k / (SPARK_LEN * ship.flame);
        let lane = floor(abs(left) - k * SPARK_FAN);
        let side = sign(left);
        let streaming = vec3f(lane, floor(behind - view.time * SPARK_SPEED), 11.0 + side);
        let twinkle = hash(vec3f(lane + side * 50.0, floor(behind), frame));
        if abs(left) < 1.5 + k * 0.8 && hash(streaming) > 0.9 + 0.08 * fade && twinkle > 0.3 {
            if fade < 0.4 {
                return vec4f(FIRE_WHITE, 1.0);
            }
            return vec4f(select(FIRE_ORANGE, FIRE_YELLOW, fade < 0.75), 1.0);
        }
    }
    return vec4f(0.0);
}

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
