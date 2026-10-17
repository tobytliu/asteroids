// Asteroids: pixel-art rocks, one instance per rock. The outline comes from
// the sim, so what you see is exactly what you collide with. Colors, craters
// and speckles come from each rock's `look` seed.

struct Rock {
    @location(0) pos: vec2f,
    // The farthest the outline reaches, in px.
    @location(1) radius: f32,
    @location(2) angle: f32,
    @location(3) look: f32,
    // Outline radii at 16 evenly spaced angles in the rock's own frame,
    // starting along `angle` (see `Asteroid::shape` in the sim).
    @location(4) shape0: vec4f,
    @location(5) shape1: vec4f,
    @location(6) shape2: vec4f,
    @location(7) shape3: vec4f,
};

struct VsOut {
    @builtin(position) clip: vec4f,
    // The rock's position in this wrapped copy.
    @location(0) @interpolate(flat) center: vec2f,
    // (angle, look)
    @location(1) @interpolate(flat) params: vec2f,
    @location(2) @interpolate(flat) shape0: vec4f,
    @location(3) @interpolate(flat) shape1: vec4f,
    @location(4) @interpolate(flat) shape2: vec4f,
    @location(5) @interpolate(flat) shape3: vec4f,
};

const OUTLINE = vec3f(0.102, 0.110, 0.173); // same as the ship's outline
// Where light comes from (upper left, toward the viewer). Fixed in world
// space, so shading stays put while the rock turns.
const LIGHT = vec3f(-0.55, 0.6, 0.58);
const MAX_CRATERS: i32 = 5;

@vertex
fn vs(@builtin(vertex_index) i: u32, rock: Rock) -> VsOut {
    var out: VsOut;
    out.center = rock.pos + wrap_offset(i);
    out.clip = world_to_clip(out.center + object_corner(i, rock.radius + PIXEL));
    out.params = vec2f(rock.angle, rock.look);
    out.shape0 = rock.shape0;
    out.shape1 = rock.shape1;
    out.shape2 = rock.shape2;
    out.shape3 = rock.shape3;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    let look = in.params.y;
    // Not snapped to rotation steps, so the drawn outline always matches the
    // one the sim collides with. The art-pixel grid keeps it blocky anyway.
    let angle = in.params.x;
    let d = snapped_offset(in.clip, in.center);
    // Position on the rock's own surface, which turns with it.
    let local = to_local(d, angle);

    var shape = array<f32, 16>(
        in.shape0.x, in.shape0.y, in.shape0.z, in.shape0.w,
        in.shape1.x, in.shape1.y, in.shape1.z, in.shape1.w,
        in.shape2.x, in.shape2.y, in.shape2.z, in.shape2.w,
        in.shape3.x, in.shape3.y, in.shape3.z, in.shape3.w,
    );
    // The outline is straight between samples, as in `Asteroid::surface`.
    let t = fract(atan2(local.y, local.x) / TAU) * 16.0;
    let k = u32(t) % 16u;
    let edge = mix(shape[k], shape[(k + 1u) % 16u], t - floor(t));
    let r = length(d);
    if r > edge {
        discard;
    }
    if r > edge - PIXEL {
        return vec4f(OUTLINE, 1.0);
    }
    var mean = 0.0;
    for (var j = 0; j < 16; j++) {
        mean += shape[j];
    }
    mean /= 16.0;

    // Light it like a sphere squashed to the outline.
    let s = d / edge;
    let normal = vec3f(s, sqrt(max(0.0, 1.0 - dot(s, s))));
    var light = dot(normal, normalize(LIGHT)) * 0.65 + 0.3;

    // Craters: 1 to 5 per rock, sometimes one big one. Each is a bowl, shadowed
    // on the side nearest the light and lit on the far wall, with a raised rim
    // that catches the light on the near side.
    let light_local = to_local(normalize(LIGHT.xy), angle);
    let craters = 1 + i32(hash(vec3f(look, 21.0, 0.0)) * f32(MAX_CRATERS));
    let has_big = hash(vec3f(look, 22.0, 0.0)) > 0.7;
    for (var c = 0; c < MAX_CRATERS; c++) {
        if c >= craters {
            break;
        }
        let cf = f32(c);
        var size = (0.1 + 0.2 * pow(hash(vec3f(look, cf, 5.0)), 1.5)) * mean;
        if c == 0 && has_big {
            size = (0.35 + 0.1 * hash(vec3f(look, cf, 8.0))) * mean;
        }
        let heading = hash(vec3f(look, cf, 3.0)) * TAU;
        let dist = sqrt(hash(vec3f(look, cf, 4.0))) * (0.75 * mean - size * 0.5);
        let q = (local - vec2f(cos(heading), sin(heading)) * dist) / size;
        let qq = dot(q, q);
        if qq < 1.0 {
            light += -0.12 - 0.3 * dot(q, light_local);
        } else if qq < 1.5 {
            light += 0.18 * dot(normalize(q), light_local);
        }
    }

    // Speckles: scattered lighter and darker art pixels, more on some rocks.
    let cell_local = floor(local / PIXEL);
    let speckles = 0.01 + 0.05 * hash(vec3f(look, 23.0, 0.0));
    let grain = hash(vec3f(cell_local, look));
    if grain < speckles {
        light += select(-0.25, 0.25, hash(vec3f(cell_local, look + 0.5)) > 0.5);
    } else {
        light += (grain - 0.5) * 0.06;
    }

    // A 2×2 ordered dither between the palette's four shades.
    let cell = floor(d / PIXEL);
    let bx = cell.x - 2.0 * floor(cell.x * 0.5);
    let by = cell.y - 2.0 * floor(cell.y * 0.5);
    let bayer = (bx * 2.0 + by * 3.0 - bx * by * 4.0 + 0.5) / 4.0 - 0.5;
    let shade = u32(floor(clamp(light * 4.0 + bayer * 0.9, 0.0, 3.99)));
    return vec4f(rock_color(palette(look), shade), 1.0);
}

// Which of the rock palettes this rock uses: brown and grey are most common.
fn palette(look: f32) -> u32 {
    let h = hash(vec3f(look, 20.0, 0.0));
    if h < 0.3 {
        return 0u;
    } else if h < 0.6 {
        return 1u;
    } else if h < 0.75 {
        return 2u;
    } else if h < 0.9 {
        return 3u;
    }
    return 4u;
}

// Shade 0 (darkest) to 3 (lightest) of a rock palette.
fn rock_color(palette: u32, shade: u32) -> vec3f {
    var colors = array<vec3f, 20>(
        // Brown
        vec3f(0.243, 0.184, 0.173), vec3f(0.373, 0.290, 0.259),
        vec3f(0.541, 0.435, 0.373), vec3f(0.722, 0.604, 0.510),
        // Grey basalt
        vec3f(0.169, 0.176, 0.208), vec3f(0.278, 0.294, 0.337),
        vec3f(0.435, 0.455, 0.502), vec3f(0.624, 0.643, 0.678),
        // Ochre
        vec3f(0.231, 0.188, 0.125), vec3f(0.400, 0.329, 0.184),
        vec3f(0.604, 0.506, 0.275), vec3f(0.788, 0.690, 0.443),
        // Rust
        vec3f(0.239, 0.122, 0.110), vec3f(0.420, 0.180, 0.145),
        vec3f(0.612, 0.290, 0.196), vec3f(0.788, 0.467, 0.310),
        // Ice
        vec3f(0.173, 0.243, 0.341), vec3f(0.310, 0.435, 0.561),
        vec3f(0.525, 0.663, 0.769), vec3f(0.812, 0.890, 0.933),
    );
    return colors[palette * 4u + shade];
}
