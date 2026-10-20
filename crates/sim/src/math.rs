//! Maths the sim can't take from `std` (determinism rule 7, ADR 0002).
//!
//! `std`'s sine, cosine and the like call each platform's own maths library,
//! which can differ in the last bit, and over many ticks that grows into a
//! different game. These come from the pure-Rust `libm` crate instead, so they
//! give the same bits everywhere. `hypot` is here too: it isn't required to be
//! exactly rounded, so platforms disagree on it as well.
//!
//! Plain arithmetic, `sqrt`, `rem_euclid`, `round`, `min`/`max` and the like
//! are exactly specified by IEEE 754, so `std`'s are fine. `powi` isn't: write
//! the multiplication out instead. clippy.toml forbids the rest.

pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}

pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}

/// `(sin(x), cos(x))`.
pub fn sin_cos(x: f32) -> (f32, f32) {
    libm::sincosf(x)
}

/// The angle of the point (`x`, `y`) from the +x axis, in (-π, π].
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

/// The length of (`x`, `y`).
pub fn hypot(x: f32, y: f32) -> f32 {
    libm::hypotf(x, y)
}
