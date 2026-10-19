// Asteroids: pixel-art rocks, one instance per rock. The outline comes from
// the sim, so what you see is exactly what you collide with. Colors and
// craters come from each rock's `look` seed.

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
// Smallest crater radius, and the smallest that gets a rim, in px.
const MIN_CRATER: f32 = 2.0 * PIXEL;
const RIM_MIN_CRATER: f32 = 3.0 * PIXEL;
// Mottling strength range (rocks vary from smooth to rough), and the size of
// its broad and fine patches, in px.
const MOTTLE_MIN: f32 = 0.2;
const MOTTLE_MAX: f32 = 0.4;
const MOTTLE_BROAD: f32 = 6.0 * PIXEL;
const MOTTLE_FINE: f32 = 3.0 * PIXEL;

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
    // on the side nearest the light and lit on the far wall; bigger ones get a
    // raised rim that catches the light on the near side. Nothing is smaller
    // than a few art pixels, since anything finer flickers as the rock turns.
    let light_local = to_local(normalize(LIGHT.xy), angle);
    let craters = 1 + i32(hash(vec3f(look, 21.0, 0.0)) * f32(MAX_CRATERS));
    let has_big = hash(vec3f(look, 22.0, 0.0)) > 0.7;
    for (var c = 0; c < MAX_CRATERS; c++) {
        if c >= craters {
            break;
        }
        let cf = f32(c);
        var size = max((0.12 + 0.2 * pow(hash(vec3f(look, cf, 5.0)), 1.5)) * mean, MIN_CRATER);
        if c == 0 && has_big {
            size = (0.35 + 0.1 * hash(vec3f(look, cf, 8.0))) * mean;
        }
        // Small rocks only have room for the first crater or two.
        if size > 0.5 * mean && c > 0 {
            break;
        }
        let heading = hash(vec3f(look, cf, 3.0)) * TAU;
        let dist = sqrt(hash(vec3f(look, cf, 4.0))) * max(0.0, 0.75 * mean - size);
        let p = local - vec2f(cos(heading), sin(heading)) * dist;
        let r_crater = length(p);
        if r_crater < size {
            light += -0.12 - 0.3 * dot(p / size, light_local);
        } else if size >= RIM_MIN_CRATER && r_crater < size + PIXEL * 1.5 {
            light += 0.18 * dot(p / r_crater, light_local);
        }
    }

    // Mottling: smooth noise stuck to the surface, in broad patches plus finer
    // ones, so flat areas get texture and band borders wander. Every patch is
    // several art pixels across, so it moves with the rock instead of
    // flickering.
    let rough = mix(MOTTLE_MIN, MOTTLE_MAX, hash(vec3f(look, 24.0, 0.0)));
    let broad = value_noise(local / max(0.45 * mean, MOTTLE_BROAD), look) - 0.5;
    let fine = value_noise(local / MOTTLE_FINE, look + 0.5) - 0.5;
    light += rough * (broad + 0.6 * fine);

    // Solid bands of the palette's four shades, with no dither or per-pixel
    // texture, so shading stays calm while the rock turns.
    let shade = u32(floor(clamp(light * 4.0, 0.0, 3.99)));
    return vec4f(rock_color(palette(look), shade), 1.0);
}

// Smooth noise, 0 to 1: random values on a unit grid, blended between.
fn value_noise(p: vec2f, seed: f32) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(vec3f(i, seed));
    let b = hash(vec3f(i + vec2f(1.0, 0.0), seed));
    let c = hash(vec3f(i + vec2f(0.0, 1.0), seed));
    let d = hash(vec3f(i + vec2f(1.0, 1.0), seed));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Which of the rock palettes this rock uses, each equally likely.
fn palette(look: f32) -> u32 {
    return min(u32(hash(vec3f(look, 20.0, 0.0)) * 5.0), 4u);
}

// Shade 0 (darkest) to 3 (lightest) of a rock palette.
fn rock_color(palette: u32, shade: u32) -> vec3f {
    var colors = array<vec3f, 20>(
        // Charcoal grey
        vec3f(0.141, 0.137, 0.137), vec3f(0.235, 0.227, 0.224),
        vec3f(0.361, 0.349, 0.341), vec3f(0.514, 0.502, 0.486),
        // Ash grey
        vec3f(0.227, 0.220, 0.212), vec3f(0.369, 0.361, 0.349),
        vec3f(0.549, 0.537, 0.518), vec3f(0.737, 0.725, 0.702),
        // Warm brown
        vec3f(0.243, 0.184, 0.173), vec3f(0.373, 0.290, 0.259),
        vec3f(0.541, 0.435, 0.373), vec3f(0.722, 0.604, 0.510),
        // Dark umber
        vec3f(0.169, 0.133, 0.110), vec3f(0.278, 0.220, 0.176),
        vec3f(0.420, 0.341, 0.271), vec3f(0.580, 0.482, 0.392),
        // Reddish brown
        vec3f(0.200, 0.137, 0.122), vec3f(0.333, 0.220, 0.180),
        vec3f(0.482, 0.329, 0.259), vec3f(0.631, 0.463, 0.373),
    );
    return colors[palette * 4u + shade];
}
