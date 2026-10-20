//! What changes how bodies move, apart from ships steering and collisions
//! (ADR 0009). Each force is its own system that only adds to a body's
//! `Accel`; `integrate` then applies the total. Which bodies a force acts on
//! is picked by the components it queries.

use crate::{Body, DT, math};

/// How fast a rock below its drift floor is eased back up to it, in px/s².
const DRIFT_RECOVERY: f32 = 4.0;

/// The acceleration this tick's forces add up to, in px/s². Every body has
/// one. Force systems add to it in the order `step` runs them, and
/// `integrate` applies and clears it.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Accel(pub [f32; 2]);

/// A rock's drift floor: slower than this, in px/s, it's eased back up to it
/// (friction in collisions slowly drains motion), so the field never grinds to
/// a halt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Drift {
    pub floor: f32,
}

/// Apply this tick's forces to a body's velocity and clear them.
pub(crate) fn integrate(body: &mut Body, accel: &mut Accel) {
    // A body nothing pushed keeps its velocity bit for bit: adding a zero
    // would turn a -0.0 into 0.0.
    if accel.0 != [0.0, 0.0] {
        for i in 0..2 {
            body.vel[i] += accel.0[i] * DT;
        }
    }
    *accel = Accel::default();
}

/// Ease a rock slower than its drift floor back up toward it, keeping its
/// heading. This holds speed rather than pushing, so it sets the velocity
/// itself instead of adding to `Accel`.
pub(crate) fn hold_drift_floor(body: &mut Body, drift: Drift) {
    let speed = math::hypot(body.vel[0], body.vel[1]);
    if speed < drift.floor {
        let target = (speed + DRIFT_RECOVERY * DT).min(drift.floor);
        // Keep its direction, or head the way it's facing if it's stopped.
        let dir = if speed > 1e-3 {
            body.vel.map(|v| v / speed)
        } else {
            [math::cos(body.angle), math::sin(body.angle)]
        };
        body.vel = dir.map(|d| d * target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrate_applies_the_forces_once() {
        let mut body = Body {
            vel: [10.0, 0.0],
            ..Body::default()
        };
        let mut accel = Accel([60.0, -120.0]);
        let expected = [10.0 + 60.0 * DT, -120.0 * DT];
        integrate(&mut body, &mut accel);
        assert_eq!(body.vel, expected);
        assert_eq!(accel, Accel::default());
        integrate(&mut body, &mut accel);
        assert_eq!(body.vel, expected);
    }

    #[test]
    fn integrate_leaves_an_unpushed_body_exactly_as_it_was() {
        let mut body = Body {
            vel: [-0.0, 3.0],
            ..Body::default()
        };
        integrate(&mut body, &mut Accel::default());
        assert_eq!(body.vel[0].to_bits(), (-0.0f32).to_bits());
    }
}
