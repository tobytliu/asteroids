// Particles: lone art pixels thrown by effects (effects/particles.rs),
// colored by how hot they still are.

struct ParticleInstance {
    // Relative to the view's center.
    @location(0) pos: vec2f,
    // (heat 0 to 1, twinkle seed)
    @location(1) heat_seed: vec2f,
};

struct VsOut {
    @builtin(position) clip: vec4f,
    @location(0) @interpolate(flat) heat_seed: vec2f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32, instance: ParticleInstance) -> VsOut {
    var out: VsOut;
    out.clip = world_to_clip(instance.pos + object_corner(i, PIXEL * 0.5));
    out.heat_seed = instance.heat_seed;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    // Each one twinkles out now and then.
    let frame = floor(view.time * FLAME_FPS);
    if hash(vec3f(in.heat_seed.y, frame, 3.0)) < 0.25 {
        discard;
    }
    return vec4f(heat_color(in.heat_seed.x), 1.0);
}
