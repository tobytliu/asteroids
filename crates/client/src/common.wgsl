// Shared by every shader; prepended to each one in gfx.rs.
//
// World positions are CSS px, origin at the canvas center, +y up.

struct View {
    // Canvas size in CSS px, which is also the world size.
    size: vec2f,
    // Canvas backing-store size in physical px.
    fb_size: vec2f,
    // Seconds, for animation.
    time: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(0) @binding(0) var<uniform> view: View;

// CSS px per art pixel. Everything is drawn on this chunky grid.
const PIXEL: f32 = 3.0;
// Rotation is drawn in discrete steps, like a pre-rendered sprite sheet.
const ROTATIONS: f32 = 36.0;
const TAU: f32 = 6.2831853;
// Each object is drawn as a square, once per wrapped copy (3×3 copies), so
// anything straddling an edge also shows up on the opposite side.
const VERTS_PER_OBJECT: u32 = 54u;

// The corner of the square for `vertex_index`, offset into the wrapped copy it
// belongs to. `half` is half the square's width, in px.
fn object_corner(vertex_index: u32, half: f32) -> vec2f {
    var corners = array<vec2f, 6>(
        vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
        vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0),
    );
    return corners[vertex_index % 6u] * half;
}

fn wrap_offset(vertex_index: u32) -> vec2f {
    let copy = vertex_index / 6u;
    return vec2f(f32(copy % 3u) - 1.0, f32(copy / 3u) - 1.0) * view.size;
}

fn world_to_clip(p: vec2f) -> vec4f {
    return vec4f(p / (view.size * 0.5), 0.0, 1.0);
}

// Offset from `center` to the middle of the art pixel containing this fragment,
// in px. The grid is screen-aligned and moves with `center`, so art stays
// blocky at every angle and doesn't shimmer as it moves.
fn snapped_offset(frag: vec4f, center: vec2f) -> vec2f {
    // frag.xy is in framebuffer pixels, y down.
    let css = frag.xy * view.size / view.fb_size;
    let world = vec2f(css.x - view.size.x * 0.5, view.size.y * 0.5 - css.y);
    return (floor((world - center) / PIXEL) + 0.5) * PIXEL;
}

fn snap_angle(a: f32) -> f32 {
    let step = TAU / ROTATIONS;
    return round(a / step) * step;
}

// `v` in a frame rotated by `a`: (along the heading, to its left).
fn to_local(v: vec2f, a: f32) -> vec2f {
    return vec2f(dot(v, vec2f(cos(a), sin(a))), dot(v, vec2f(-sin(a), cos(a))));
}

// A pseudo-random value in [0, 1) for each input (Dave Hoskins' hash13).
fn hash(p: vec3f) -> f32 {
    var q = fract(p * vec3f(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

