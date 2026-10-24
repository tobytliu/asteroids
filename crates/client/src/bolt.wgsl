// Bolts: glowing streaks with a white-hot core, one per instance, added on
// top of everything else. Unlike the pixel art they're drawn smoothly, off
// the art grid and at their exact angle, so the streak lies exactly along
// the path the sim flies the bolt on.

struct BoltInstance {
    // The bolt's head, relative to the view's center.
    @location(0) pos: vec2f,
    // (angle it points, length of the streak behind its head in px, half the
    // width of its colored body in px)
    @location(1) shape: vec3f,
    @location(2) color: vec3f,
};

struct VsOut {
    @builtin(position) clip: vec4f,
    // This fragment in the bolt's frame: +x toward its head, which is at 0.
    @location(0) local: vec2f,
    // (length, half width)
    @location(1) @interpolate(flat) size: vec2f,
    @location(2) @interpolate(flat) color: vec3f,
};

// How far the glow reaches past the body, in px.
const GLOW: f32 = 16.0;
const CORE = vec3f(1.0, 0.92, 0.9);

@vertex
fn vs(@builtin(vertex_index) i: u32, instance: BoltInstance) -> VsOut {
    let angle = instance.shape.x;
    let len = instance.shape.y;
    let half = instance.shape.z;
    // The corner as ±1, stretched over the streak and its glow.
    let c = object_corner(i, 1.0);
    let local = vec2f(mix(-len - GLOW, GLOW, c.x * 0.5 + 0.5), c.y * (half + GLOW));
    let dir = vec2f(cos(angle), sin(angle));
    let world = instance.pos + dir * local.x + vec2f(-dir.y, dir.x) * local.y;
    var out: VsOut;
    out.clip = world_to_clip(world);
    out.local = local;
    out.size = vec2f(len, half);
    out.color = instance.color;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    let len = in.size.x;
    let half = in.size.y;
    // Distance from the streak's spine, a line from its tail to its head, so
    // both ends come out round.
    let along = clamp(in.local.x, -len, 0.0);
    let d = length(in.local - vec2f(along, 0.0));
    let core = 1.0 - smoothstep(half * 0.2, half * 0.5, d);
    let body = 1.0 - smoothstep(half * 0.85, half * 1.2, d);
    let past = max(d - half, 0.0);
    let halo = exp(-past / (GLOW * 0.3)) * 0.85 * (1.0 - smoothstep(GLOW * 0.4, GLOW, past));
    var color = mix(in.color * halo, in.color, body);
    color = mix(color, CORE, core);
    // A little dimmer toward the tail.
    let toward_head = 1.0 + along / max(len, 1.0);
    // Added to what's beneath, so it glows over whatever it passes.
    return vec4f(color * mix(0.6, 1.0, toward_head), 1.0);
}
