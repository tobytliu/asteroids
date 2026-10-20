// Asteroids: pixel-art rocks, one instance per rock. The outline comes from
// the sim, so what you see is exactly what you collide with. Colors and
// craters come from each rock's `look` seed.

struct Rock {
    // Relative to the view's center.
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
    // The rock's `sim::Class` (see the `CLASS_` constants).
    @location(12) rock_class: u32,
};

struct VsOut {
    @builtin(position) clip: vec4f,
    // This copy's position relative to the view's center.
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
    @location(10) @interpolate(flat) rock_class: u32,
};

const OUTLINE = vec3f(0.102, 0.110, 0.173); // same as the ship's outline
// Where light comes from (upper left, toward the viewer). Fixed in world
// space, so shading stays put while the rock turns.
const LIGHT = vec3f(-0.55, 0.6, 0.58);
// Share of medium and big rocks with craters (small ones have no room for
// two, and a lone crater is meant to be rare), and of those that also get
// ridges (every rock without craters has them).
const CRATERED: f32 = 0.3;
const CRATERED_RIDGED: f32 = 0.4;
const MAX_CRATERS: i32 = 3;
// Share of cratered big rocks that ask for 3 rather than 2.
const THREE_CRATERS: f32 = 0.45;
// How many spots to try for each crater before giving up on it.
const CRATER_TRIES: i32 = 6;
// Craters sit within this fraction of the outline, at least this far apart
// (in px, rim to rim).
const CRATER_REACH: f32 = 0.85;
const CRATER_GAP: f32 = PIXEL;
const GOLDEN_ANGLE: f32 = 2.3999632;
// Smallest crater radius, and the smallest that gets a rim, in px. Anything
// smaller is a blob too few pixels across to read as a bowl.
const MIN_CRATER: f32 = 3.0 * PIXEL;
const RIM_MIN_CRATER: f32 = 4.0 * PIXEL;
// The sim's rock classes, in `sim::Class` order.
const CLASS_SMALL: u32 = 0u;
const CLASS_MEDIUM: u32 = 1u;
const CLASS_BIG: u32 = 2u;
const CLASS_LARGE: u32 = 3u;
// Ridges: at most this many per rock, crossing within this fraction of the
// rock's mean radius either side of its center. Each has a steep side this
// wide, in px, and tilts the face beyond it by this much.
const MAX_RIDGES: i32 = 3;
const RIDGE_REACH: f32 = 1.0;
const RIDGE_STEEP: f32 = 2.0 * PIXEL;
const RIDGE_LEAN: f32 = 0.3;
// How strongly a ridge curves (how far it strays from straight across the
// rock, as a fraction of the mean radius), and the share of extra ridges
// that run straight.
const RIDGE_BOW_MIN: f32 = 0.35;
const RIDGE_BOW_MAX: f32 = 0.8;
const RIDGE_STRAIGHT: f32 = 0.2;
// Mottling strength range (rocks vary from smooth to rough), and the size of
// its broad and fine patches, in px.
const MOTTLE_MIN: f32 = 0.2;
const MOTTLE_MAX: f32 = 0.4;
const MOTTLE_BROAD: f32 = 6.0 * PIXEL;
const MOTTLE_FINE: f32 = 3.0 * PIXEL;

@vertex
fn vs(@builtin(vertex_index) i: u32, rock: Rock) -> VsOut {
    var out: VsOut;
    out.center = rock.pos;
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
    out.rock_class = rock.rock_class;
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
    let rules = surface_rules(in.rock_class);

    let cratered = rules != CLASS_SMALL && hash(vec3f(look, 21.0, 0.0)) < CRATERED;
    let ridged = !cratered || hash(vec3f(look, 25.0, 0.0)) < CRATERED_RIDGED;

    // Light it like a sphere squashed to the outline, its surface tilted by
    // any ridges.
    let s = d / edge;
    var tilt = vec2f(0.0);
    if ridged {
        tilt = to_local(ridges(local, look, mean, rules), -angle);
    }
    let normal = normalize(vec3f(s + tilt, sqrt(max(0.0, 1.0 - dot(s, s)))));
    var light = dot(normal, normalize(LIGHT)) * 0.65 + 0.3;

    // Craters, on only some rocks: medium ones ask for 2, big ones 2 or 3, and
    // a big rock sometimes has one oversized one. Each shifts the shading by
    // whole shades, so it reads as solid bands; see `crater`. They stay inside
    // the outline. One that can't find a clear spot is left out, so a medium
    // rock sometimes ends up with just 1.
    let light_local = to_local(normalize(LIGHT.xy), angle);
    var craters = 0;
    if cratered {
        craters = 2;
        if rules == CLASS_BIG && hash(vec3f(look, 26.0, 0.0)) < THREE_CRATERS {
            craters = 3;
        }
    }
    let has_big = rules == CLASS_BIG && hash(vec3f(look, 22.0, 0.0)) > 0.7;
    // Headings step round by the golden angle from a random start, so craters
    // scatter around the rock instead of trailing off in a line.
    let first_heading = hash(vec3f(look, 23.0, 0.0)) * TAU;
    // Craters already placed: center (in the rock's frame) and radius.
    var placed: array<vec3f, MAX_CRATERS>;
    var count = 0;
    var shades = 0.0;
    for (var c = 0; c < MAX_CRATERS; c++) {
        if c >= craters {
            break;
        }
        for (var t = 0; t < CRATER_TRIES; t++) {
            // Consecutive integers hash to similar values, so spread them out.
            let key = f32(c) * 13.7 + f32(t) * 5.3;
            var size = max((0.15 + 0.15 * pow(hash(vec3f(look, key, 5.0)), 1.5)) * mean, MIN_CRATER);
            if c == 0 && has_big {
                size = (0.32 + 0.1 * hash(vec3f(look, key, 8.0))) * mean;
            }
            let heading = first_heading + (f32(c) + f32(t) * 0.37) * GOLDEN_ANGLE
                + (hash(vec3f(look, key, 3.0)) - 0.5) * 0.8;
            let room = CRATER_REACH * outline(shape, heading) - size;
            if room < 0.0 {
                continue;
            }
            let dist = sqrt(hash(vec3f(look, key, 4.0))) * room;
            let at = vec2f(cos(heading), sin(heading)) * dist;
            // Try again if it would overlap (or touch the rim of) an earlier one.
            var clear = true;
            for (var k = 0; k < count; k++) {
                if distance(at, placed[k].xy) < size + placed[k].z + CRATER_GAP {
                    clear = false;
                }
            }
            if !clear {
                continue;
            }
            placed[count] = vec3f(at, size);
            count++;
            shades += crater(local - at, size, light_local);
            break;
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
    // Craters shift by whole bands, so their zones keep their shape however
    // the shading beneath them falls.
    light += 0.25 * shades;
    let shade = u32(floor(clamp(light * 4.0, 0.0, 3.99)));
    return vec4f(rock_color(palette(look), shade), 1.0);
}

// Which class's rules a rock of `rock_class` has its surface drawn by. Large
// rocks borrow big ones' for now.
fn surface_rules(rock_class: u32) -> u32 {
    switch rock_class {
        case CLASS_LARGE: {
            return CLASS_BIG;
        }
        default: {
            return rock_class;
        }
    }
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

// How the surface at `p` (in the rock's own frame) is tilted by ridges, as a
// sideways push on its normal. Like the rocks in the reference art, a few
// crests cut across the rock at their own angles, splitting it into faces
// that each lean their own way. Past each crest the surface drops steeply, so
// one side of it is a thin crease. Which side looks dark follows the light, so
// nothing jumps as the rock turns.
fn ridges(p: vec2f, look: f32, mean: f32, rules: u32) -> vec2f {
    var count = 1;
    if rules == CLASS_BIG {
        count = 2 + i32(hash(vec3f(look, 27.0, 0.0)) * 2.0);
    } else if rules == CLASS_MEDIUM {
        count = 1 + i32(hash(vec3f(look, 27.0, 0.0)) * 2.0);
    }
    var tilt = vec2f(0.0);
    for (var i = 0; i < MAX_RIDGES; i++) {
        if i >= count {
            break;
        }
        // Consecutive integers hash to similar values, so spread them out.
        let key = f32(i) * 13.7;
        // Roughly across the long axis (local x), at its own angle, and
        // dropping either way.
        var a = (hash(vec3f(look, key, 28.0)) - 0.5) * 2.0;
        if hash(vec3f(look, key, 29.0)) < 0.5 {
            a += TAU * 0.5;
        }
        let n = vec2f(cos(a), sin(a));
        let along = dot(p, vec2f(-n.y, n.x));
        // Where it crosses, kept well inside the rock.
        let at = (hash(vec3f(look, key, 30.0)) - 0.5) * RIDGE_REACH * mean;
        // Most ridges curve: an arc, an S-bend, or both. A straight one is
        // only ever an extra ridge, so no rock has just one straight one.
        var bow = 0.0;
        var bend = 0.0;
        let straight = i > 0 && hash(vec3f(look, key, 33.0)) < RIDGE_STRAIGHT;
        if !straight {
            let style = hash(vec3f(look, key, 31.0));
            let sign = select(-1.0, 1.0, hash(vec3f(look, key, 34.0)) < 0.5);
            if style < 0.7 {
                bow = sign * mix(RIDGE_BOW_MIN, RIDGE_BOW_MAX, hash(vec3f(look, key, 35.0)));
            }
            if style > 0.4 {
                bend = sign * mix(RIDGE_BOW_MIN, RIDGE_BOW_MAX, hash(vec3f(look, key, 36.0)));
            }
        }
        let waver = (value_noise(p / (0.5 * mean), look + key + 0.3) - 0.5) * 0.25 * mean;
        let u = dot(p, n) - at + bow * along * along / mean
            + bend * 0.5 * mean * sin(1.5 * along / mean) + waver;
        if u < 0.0 {
            continue;
        }
        if u < RIDGE_STEEP {
            tilt += n;
        } else {
            // The face beyond leans its own way.
            let lean = hash(vec3f(look, key, 32.0)) * TAU;
            tilt += vec2f(cos(lean), sin(lean)) * RIDGE_LEAN;
        }
    }
    return tilt;
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
