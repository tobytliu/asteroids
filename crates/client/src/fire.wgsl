// Fire plumes, one per instance, drawn behind whatever is burning. Where each
// one sits, how bright it is and how far its tail trails come from
// effects/fire.rs.

struct FireInstance {
    // The burner's center, relative to the view's center.
    @location(0) pos: vec2f,
    // The nozzle, in px in the burner's frame (+x along its heading).
    @location(1) offset: vec2f,
    // (burner's angle, which way the fire points from its heading,
    // intensity 0 to 1, how far the tail bends per art pixel in radians)
    @location(2) motion: vec4f,
    // (longest flame in art pixels, how wide it billows either side, the
    // most its tail can swing in radians, flicker seed)
    @location(3) style: vec4f,
};

struct Fire {
    intensity: f32,
    bend: f32,
    len: f32,
    width: f32,
    max_swing: f32,
    seed: f32,
};
// The fire this fragment belongs to, set at the top of `fs`.
var<private> fire: Fire;

struct VsOut {
    @builtin(position) clip: vec4f,
    @location(0) @interpolate(flat) center: vec2f,
    @location(1) @interpolate(flat) offset: vec2f,
    @location(2) @interpolate(flat) motion: vec4f,
    @location(3) @interpolate(flat) style: vec4f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32, instance: FireInstance) -> VsOut {
    var out: VsOut;
    out.center = instance.pos;
    // Room for the nozzle's offset and the longest flame at its biggest surge.
    let reach = length(instance.offset) + (instance.style.x * 1.25 + instance.style.y + 2.0) * PIXEL;
    out.clip = world_to_clip(out.center + object_corner(i, reach));
    out.offset = instance.offset;
    out.motion = instance.motion;
    out.style = instance.style;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    fire = Fire(in.motion.z, in.motion.w, in.style.x, in.style.y, in.style.z, in.style.w);
    // On the same art grid and in the same snapped frame as the burner's
    // sprite, so the flame meets the nozzle cleanly.
    let local = to_local(snapped_offset(in.clip, in.center), snap_angle(in.motion.x)) / PIXEL;
    // (art pixels out from the nozzle, art pixels to the left of the fire's
    // direction)
    let out = to_local(local - in.offset / PIXEL, in.motion.y);
    let color = plume(out.x, out.y);
    if color.a < 0.5 {
        discard;
    }
    return color;
}

// The flame `s` art pixels out from the nozzle and `side` to the left of
// where it points. Transparent where there is none.
fn plume(s: f32, side: f32) -> vec4f {
    if fire.intensity < 0.02 || s < 0.0 {
        return vec4f(0.0);
    }
    // Gas `s` out left the nozzle a moment ago, when the burner was turned
    // back by `bend * s`, so the plume's centerline there has swung round
    // that far: the flame trails off in an arc through a turn.
    let swing = clamp(fire.bend * s, -fire.max_swing, fire.max_swing);
    let across = side + s * sin(swing);

    let frame = floor(view.time * FLAME_FPS);
    // The whole flame surges and shrinks a little every frame.
    let len = fire.intensity * fire.len * (0.75 + 0.45 * hash(vec3f(fire.seed, 0.0, frame)));
    // 0 at the nozzle to 1 at the tip.
    let t = s / len;
    if t >= 1.0 {
        return vec4f(0.0);
    }
    let n = hash(vec3f(floor(vec2f(across, s)) + fire.seed, frame));
    // Half-width at the nozzle, billowing out, then tapering to a point,
    // with ragged edges.
    let bloom = fire.width * (0.5 + 0.5 * smoothstep(0.0, 0.25, t));
    let half_width = bloom * sqrt(1.0 - t) * min(1.0, fire.intensity * 1.5) + (n - 0.5) * 0.9;
    if abs(across) >= half_width {
        return vec4f(0.0);
    }
    // Hottest along the centerline near the nozzle.
    let heat = 1.15 - t - 0.45 * abs(across) / max(half_width, 0.1) + (n - 0.5) * 0.3;
    return vec4f(heat_color(heat), 1.0);
}
