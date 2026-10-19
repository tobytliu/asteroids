// Asteroids: pixel-art rocks, one instance per rock. The outline comes from
// the sim, so what you see is exactly what you collide with. Colors and
// craters come from each rock's `look` seed.

struct Rock {
    @location(0) pos: vec2f,
    // The farthest the outline reaches, in px.
    @location(1) radius: f32,
    @location(2) angle: f32,
    @location(3) look: f32,
    // Outline radii at 32 evenly spaced angles in the rock's own frame,
    // starting along `angle` (see `Asteroid::shape` in the sim).
    @location(4) shape0: vec4f,
    @location(5) shape1: vec4f,
    @location(6) shape2: vec4f,
    @location(7) shape3: vec4f,
    @location(8) shape4: vec4f,
    @location(9) shape5: vec4f,
    @location(10) shape6: vec4f,
    @location(11) shape7: vec4f,
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
    @location(6) @interpolate(flat) shape4: vec4f,
    @location(7) @interpolate(flat) shape5: vec4f,
    @location(8) @interpolate(flat) shape6: vec4f,
    @location(9) @interpolate(flat) shape7: vec4f,
};

const OUTLINE = vec3f(0.102, 0.110, 0.173); // same as the ship's outline
// Where light comes from (upper left, toward the viewer). Fixed in world
// space, so shading stays put while the rock turns.
const LIGHT = vec3f(-0.55, 0.6, 0.58);
const MAX_CRATERS: i32 = 4;
// Smallest crater radius, and the smallest that gets a rim, in px. Anything
// smaller is a blob too few pixels across to read as a bowl.
const MIN_CRATER: f32 = 3.0 * PIXEL;
const RIM_MIN_CRATER: f32 = 4.0 * PIXEL;
// Mean radii, in px, from which a rock counts as medium and as big (the sim's
// three size classes, allowing for their jitter).
const MEDIUM_ROCK: f32 = 23.0;
const BIG_ROCK: f32 = 39.0;
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
    out.shape4 = rock.shape4;
    out.shape5 = rock.shape5;
    out.shape6 = rock.shape6;
    out.shape7 = rock.shape7;
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

    let shape = array<f32, 32>(
        in.shape0.x, in.shape0.y, in.shape0.z, in.shape0.w,
        in.shape1.x, in.shape1.y, in.shape1.z, in.shape1.w,
        in.shape2.x, in.shape2.y, in.shape2.z, in.shape2.w,
        in.shape3.x, in.shape3.y, in.shape3.z, in.shape3.w,
        in.shape4.x, in.shape4.y, in.shape4.z, in.shape4.w,
        in.shape5.x, in.shape5.y, in.shape5.z, in.shape5.w,
        in.shape6.x, in.shape6.y, in.shape6.z, in.shape6.w,
        in.shape7.x, in.shape7.y, in.shape7.z, in.shape7.w,
    );
    let edge = outline(shape, atan2(local.y, local.x));
    let r = length(d);
    if r > edge {
        discard;
    }
    if r > edge - PIXEL {
        return vec4f(OUTLINE, 1.0);
    }
    var mean = 0.0;
    for (var j = 0; j < 32; j++) {
        mean += shape[j];
    }
    mean /= 32.0;

    // Light it like a sphere squashed to the outline.
    let s = d / edge;
    let normal = vec3f(s, sqrt(max(0.0, 1.0 - dot(s, s))));
    var light = dot(normal, normalize(LIGHT)) * 0.65 + 0.3;

    // Craters: fewer on smaller rocks (big 2-4, medium 1-3, small 0-1), and
    // sometimes one big one. Each shifts the shading by whole shades, so it
    // reads as solid bands; see `crater`. They stay well inside the outline.
    let light_local = to_local(normalize(LIGHT.xy), angle);
    let roll = hash(vec3f(look, 21.0, 0.0));
    var craters = i32(roll * 2.0);
    if mean >= BIG_ROCK {
        craters = 2 + i32(roll * 3.0);
    } else if mean >= MEDIUM_ROCK {
        craters = 1 + i32(roll * 3.0);
    }
    let has_big = mean >= MEDIUM_ROCK && hash(vec3f(look, 22.0, 0.0)) > 0.7;
    var shades = 0.0;
    for (var c = 0; c < MAX_CRATERS; c++) {
        if c >= craters {
            break;
        }
        let cf = f32(c);
        var size = max((0.15 + 0.15 * pow(hash(vec3f(look, cf, 5.0)), 1.5)) * mean, MIN_CRATER);
        if c == 0 && has_big {
            size = (0.32 + 0.1 * hash(vec3f(look, cf, 8.0))) * mean;
        }
        let heading = hash(vec3f(look, cf, 3.0)) * TAU;
        let room = 0.7 * outline(shape, heading) - size;
        if room < 0.0 {
            continue;
        }
        let dist = sqrt(hash(vec3f(look, cf, 4.0))) * room;
        shades += crater(local - vec2f(cos(heading), sin(heading)) * dist, size, light_local);
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
    // Craters shift by whole bands, so their zones keep their shape however
    // the shading beneath them falls.
    light += 0.25 * shades;
    let shade = u32(floor(clamp(light * 4.0, 0.0, 3.99)));
    return vec4f(rock_color(palette(look), shade), 1.0);
}

// Distance from the center to the outline in direction `a` (radians, in the
// rock's own frame), in px. Straight between samples, as in
// `Asteroid::surface`.
fn outline(shape: array<f32, 32>, a: f32) -> f32 {
    // A var, so it can be indexed at run time.
    var s = shape;
    let t = fract(a / TAU) * 32.0;
    let k = u32(t) % 32u;
    return mix(s[k], s[(k + 1u) % 32u], t - floor(t));
}

// How many shades a crater of radius `size` shifts the surface at offset `p`
// from its center, with `light` the unit direction toward the light. A bowl:
// the wall nearest the light is in shadow (a crescent, -2), the far wall
// faces the light (a crescent, +1), and the floor between is a little dark
// (-1). Bigger craters get a raised rim that catches the light on the near
// side (+1).
fn crater(p: vec2f, size: f32, light: vec2f) -> f32 {
    let r = length(p);
    if r < size {
        let toward = dot(p, light);
        if toward > 0.0 && length(p + light * 0.35 * size) > 0.75 * size {
            return -2.0;
        }
        if toward < 0.0 && length(p - light * 0.35 * size) > 0.8 * size {
            return 1.0;
        }
        return -1.0;
    }
    if size >= RIM_MIN_CRATER && r < size + PIXEL * 1.5 && dot(p / r, light) > 0.3 {
        return 1.0;
    }
    return 0.0;
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
