struct Ship { pos: vec2f, angle: f32, aspect: f32 };
@group(0) @binding(0) var<uniform> ship: Ship;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    var pts = array<vec2f, 3>(vec2f(0.05, 0.0), vec2f(-0.03, 0.025), vec2f(-0.03, -0.025));
    let p = pts[i];
    let c = cos(ship.angle);
    let s = sin(ship.angle);
    let r = vec2f(p.x * c - p.y * s, p.x * s + p.y * c);
    // Undo the canvas stretch so the ship keeps its shape at any window size.
    let shape = vec2f(r.x / ship.aspect, r.y);
    return vec4f(shape + ship.pos, 0.0, 1.0);
}

@fragment
fn fs() -> @location(0) vec4f { return vec4f(0.3, 0.9, 1.0, 1.0); }
