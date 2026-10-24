//! Browser client: rendering, input and the frame loop. No game logic lives
//! here; input handling only produces a `sim::Input`.
//!
//! Only the browser modules are wasm-only. Anything else compiles natively, so
//! it can be unit-tested with plain `cargo test -p client`.

pub mod camera;
pub mod copies;
pub mod effects;
pub mod interpolate;
pub mod sprite;

/// CSS px per art pixel. Must match `PIXEL` in common.wgsl.
pub const PIXEL: f32 = 3.0;

#[cfg(target_arch = "wasm32")]
mod debug;
#[cfg(target_arch = "wasm32")]
mod gfx;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    /// Every shader, with common.wgsl (and heat.wgsl for anything that
    /// burns) prepended as gfx.rs does.
    const SHADERS: [(&str, &str); 6] = [
        (
            "stars",
            concat!(include_str!("common.wgsl"), include_str!("stars.wgsl")),
        ),
        (
            "ship",
            concat!(
                include_str!("common.wgsl"),
                include_str!("heat.wgsl"),
                include_str!("ship.wgsl")
            ),
        ),
        (
            "fire",
            concat!(
                include_str!("common.wgsl"),
                include_str!("heat.wgsl"),
                include_str!("fire.wgsl")
            ),
        ),
        (
            "particle",
            concat!(
                include_str!("common.wgsl"),
                include_str!("heat.wgsl"),
                include_str!("particle.wgsl")
            ),
        ),
        (
            "bolt",
            concat!(include_str!("common.wgsl"), include_str!("bolt.wgsl")),
        ),
        (
            "asteroid",
            concat!(include_str!("common.wgsl"), include_str!("asteroid.wgsl")),
        ),
    ];

    #[test]
    fn shaders_compile() {
        for (name, source) in SHADERS {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|e| panic!("{name}.wgsl: {}", e.emit_to_string(source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}.wgsl: {}", e.emit_to_string(source)));
        }
    }
}
