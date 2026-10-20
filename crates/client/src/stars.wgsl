// Background starfield: sparse stars on the art grid, on a layer that scrolls
// slower than the map (parallax) and isn't tied to it, so it never shows the
// map's seam. Each star twinkles at its own pace, and a few bright ones now and
// then flash a cross-shaped glint.

// The pattern repeats every this many art pixels on each axis, far too wide to
// notice. That lets the client keep `view.star_offset` small (it wraps it by
// STAR_PERIOD * PIXEL, which must match `STAR_PERIOD_PX` in gfx.rs), where
// the hash still works well, without the stars ever jumping.
const STAR_PERIOD: f32 = 4096.0;

// Fraction of art pixels holding a star.
const STAR_CHANCE: f32 = 0.0025;
// Fraction of stars bright enough to glint.
const GLINT_CHANCE: f32 = 0.04;
// Stars are drawn this faint so they stay in the background.
const OPACITY: f32 = 0.5;
// Fraction of stars drawn at full brightness instead.
const BRIGHT_CHANCE: f32 = 0.06;
// Fraction of stars drawn 2×2 art pixels instead of one. Glinting stars stay
// one pixel, so their cross stays centered on a single point.
const LARGE_CHANCE: f32 = 0.05;
// Twinkle speed range, in rad/s.
const TWINKLE_MIN: f32 = 0.3;
const TWINKLE_MAX: f32 = 1.2;
// Glint speed range, in rad/s: one flash per 2π / speed seconds.
const GLINT_MIN: f32 = 0.25;
const GLINT_MAX: f32 = 0.5;

// From the same palette as the ship.
const DIM = vec3f(0.2, 0.235, 0.341);
const FAINT = vec3f(0.337, 0.424, 0.525);
const BLUE = vec3f(0.580, 0.690, 0.761);
const YELLOW = vec3f(1.0, 0.804, 0.459);
const WHITE = vec3f(0.957, 0.957, 0.957);

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    // One triangle that covers the whole screen.
    let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    // Which art pixel of the star layer this is.
    let cell = floor(snapped_offset(frag, -view.star_offset) / PIXEL);
    var rgb = star(cell) * opacity(cell);

    // A large star at `cell` also covers the art pixels to its right and below,
    // so check the stars that would reach this pixel from the left and above.
    var owners = array<vec2f, 3>(vec2f(-1.0, 0.0), vec2f(0.0, 1.0), vec2f(-1.0, 1.0));
    for (var k = 0; k < 3; k++) {
        let owner = cell + owners[k];
        if is_large(owner) {
            rgb = max(rgb, star(owner) * opacity(owner));
        }
    }

    // A glinting star grows a one-pixel arm on each side.
    var dirs = array<vec2f, 4>(vec2f(1.0, 0.0), vec2f(-1.0, 0.0), vec2f(0.0, 1.0), vec2f(0.0, -1.0));
    for (var k = 0; k < 4; k++) {
        let owner = cell - dirs[k];
        if glint(owner) > 0.35 {
            rgb = max(rgb, BLUE * opacity(owner));
        }
    }
    return vec4f(rgb, 1.0);
}

fn is_star(cell: vec2f) -> bool {
    return star_hash(cell, 1.0) < STAR_CHANCE;
}

fn can_glint(cell: vec2f) -> bool {
    return star_hash(cell, 2.0) >= 1.0 - GLINT_CHANCE;
}

fn is_bright(cell: vec2f) -> bool {
    return star_hash(cell, 8.0) < BRIGHT_CHANCE;
}

fn is_large(cell: vec2f) -> bool {
    return is_star(cell) && !can_glint(cell) && star_hash(cell, 9.0) < LARGE_CHANCE;
}

// Over a black background, opacity is just brightness.
fn opacity(cell: vec2f) -> f32 {
    return select(OPACITY, 1.0, is_bright(cell));
}

fn star(cell: vec2f) -> vec3f {
    if !is_star(cell) {
        return vec3f(0.0);
    }
    if glint(cell) > 0.35 {
        return WHITE;
    }
    // Average brightness, plus a twinkle at the star's own speed and phase,
    // stepped into palette colors rather than faded smoothly. Bright stars stay
    // in the upper range so they never dim to the faintest colors, and large
    // ones stay brighter still, since a dim 2×2 star reads as a smudge.
    var base = star_hash(cell, 2.0);
    if is_large(cell) {
        base = 0.85 + 0.15 * base;
    } else if is_bright(cell) {
        base = 0.6 + 0.4 * base;
    }
    let speed = mix(TWINKLE_MIN, TWINKLE_MAX, star_hash(cell, 3.0));
    let phase = star_hash(cell, 4.0) * TAU;
    let twinkle = 0.5 + 0.5 * sin(view.time * speed + phase);
    let level = floor(base * 2.4 + twinkle * 1.5);
    if level >= 3.0 {
        return WHITE;
    } else if level >= 2.0 {
        return select(BLUE, YELLOW, star_hash(cell, 5.0) > 0.75);
    } else if level >= 1.0 {
        return FAINT;
    }
    return DIM;
}

// How strongly the star at `cell` is glinting right now, 0 to 1. Only the
// brightest stars glint: slow flashes, many seconds apart.
fn glint(cell: vec2f) -> f32 {
    if !is_star(cell) || !can_glint(cell) {
        return 0.0;
    }
    let speed = mix(GLINT_MIN, GLINT_MAX, star_hash(cell, 6.0));
    let phase = star_hash(cell, 7.0) * TAU;
    return pow(max(0.0, sin(view.time * speed + phase)), 24.0);
}

// A pseudo-random value in [0, 1) for star layer cell `cell`, repeating every
// STAR_PERIOD cells. `k` picks which of the cell's values.
fn star_hash(cell: vec2f, k: f32) -> f32 {
    return hash(vec3f(cell - STAR_PERIOD * floor(cell / STAR_PERIOD), k));
}
