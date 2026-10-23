// The player ship: a pixel-art sprite whose engine pixel glows while it
// thrusts. Its flame is drawn separately, in fire.wgsl.

// One ship, as a per-instance vertex.
struct ShipInstance {
    // Relative to the view's center.
    @location(0) pos: vec2f,
    // How far the sprite can reach from `pos`, in px.
    @location(1) reach: f32,
    @location(2) angle: f32,
    @location(3) flame: f32,
};

struct Ship {
    angle: f32,
    // Engine flame intensity, 0 (off) to 1 (full).
    flame: f32,
};
// The ship this fragment belongs to, set at the top of `fs`.
var<private> ship: Ship;

@group(1) @binding(1) var sprite: texture_2d<f32>;
@group(1) @binding(2) var sprite_sampler: sampler;

// The texture is the sprite upscaled this many times (see sprite.rs).
const UPSCALE: f32 = 8.0;

// The sprite's engine pixel color in sprite.rs, which engine_glow() lights up.
const ENGINE = vec3f(0.937, 0.490, 0.341);

struct VsOut {
    @builtin(position) clip: vec4f,
    // This copy's position relative to the view's center.
    @location(0) @interpolate(flat) center: vec2f,
    // (angle, flame)
    @location(1) @interpolate(flat) params: vec2f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32, instance: ShipInstance) -> VsOut {
    var out: VsOut;
    out.center = instance.pos;
    out.clip = world_to_clip(out.center + object_corner(i, instance.reach));
    out.params = vec2f(instance.angle, instance.flame);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    ship = Ship(in.params.x, in.params.y);
    // Ship-local position in sprite pixels: +x toward the nose, +y to port.
    let local = to_local(snapped_offset(in.clip, in.center), snap_angle(ship.angle)) / PIXEL;

    // The sprite is drawn nose-up: image up is forward, image left is the ship's
    // left. The small bias keeps samples off exact texel edges at right angles.
    let size = vec2f(textureDimensions(sprite)) / UPSCALE;
    let texel = vec2f(size.x * 0.5 - local.y, size.y * 0.5 - local.x) + 1e-3;
    if all(texel >= vec2f(0.0)) && all(texel < size) {
        let color = textureSampleLevel(sprite, sprite_sampler, texel / size, 0.0);
        if color.a >= 0.5 {
            return vec4f(engine_glow(color.rgb), 1.0);
        }
    }

    discard;
    // Never reached, but WGSL needs a function to end in a return (naga lets
    // this slide; browsers' WebGPU doesn't).
    return vec4f(0.0);
}

// The sprite's engine pixel flashes hot while the flame is lit.
fn engine_glow(rgb: vec3f) -> vec3f {
    if ship.flame < 0.05 || distance(rgb, ENGINE) > 0.02 {
        return rgb;
    }
    let frame = floor(view.time * FLAME_FPS);
    return select(FIRE_YELLOW, FIRE_WHITE, hash(vec3f(1.0, 2.0, frame)) > 0.4);
}
