//! What changes how bodies move, apart from ships steering and collisions
//! (ADR 0009). Each force is its own system that only adds to a body's
//! `Accel`; `integrate` then applies the total. Which bodies a force acts on
//! is picked by the components it queries.

use crate::field::Member;
use crate::{Body, DT, math, wrapped_offset};

/// How fast a rock below its drift floor is eased back up to it, in px/s².
const DRIFT_RECOVERY: f32 = 4.0;
/// How strongly members of a group pull on each other: the acceleration, in
/// px/s², from 1 px² of the other rock's area at 1 px.
const GRAVITY: f32 = 3.0;
/// Two members closer than their group's spacing (edge to edge) don't pull at
/// all, so groups never settle into a heap; the pull fades in over this many
/// px beyond that.
const GRAVITY_RAMP: f32 = 100.0;
/// Members farther apart than this, center to center in px, don't pull at
/// all, so a rock knocked out of its group can leave and nothing pulls across
/// the map's wrapped edges. The pull fades out over the last `GRAVITY_FADE`.
const GRAVITY_REACH: f32 = 1800.0;
const GRAVITY_FADE: f32 = 400.0;

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

/// A group member, as gravity sees it.
pub(crate) struct Attractor {
    pub pos: [f32; 2],
    pub mass: f32,
    /// The farthest its outline reaches, in px.
    pub radius: f32,
    pub member: Member,
}

/// Each rock's pull from the other members of its group, in px/s². `rocks` must
/// be in entity ID order (determinism rule 2), and the result is in the same
/// order.
pub(crate) fn group_gravity(rocks: &[Attractor], size: [f32; 2]) -> Vec<[f32; 2]> {
    let mut accel = vec![[0.0; 2]; rocks.len()];
    for i in 0..rocks.len() {
        for j in i + 1..rocks.len() {
            let (a, b) = (&rocks[i], &rocks[j]);
            if a.member.group != b.member.group {
                continue;
            }
            let d = wrapped_offset(a.pos, b.pos, size);
            let dist = math::hypot(d[0], d[1]);
            let near = a.radius + b.radius + a.member.spacing;
            let fade_in = ((dist - near) / GRAVITY_RAMP).clamp(0.0, 1.0);
            let fade_out = ((GRAVITY_REACH - dist) / GRAVITY_FADE).clamp(0.0, 1.0);
            let strength = GRAVITY * fade_in * fade_out;
            if strength == 0.0 {
                continue;
            }
            // Toward each other, by the inverse square of the distance.
            let pull = d.map(|v| v * strength / (dist * dist * dist));
            for k in 0..2 {
                accel[i][k] += pull[k] * b.mass;
                accel[j][k] -= pull[k] * a.mass;
            }
        }
    }
    accel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::GroupId;

    const SIZE: [f32; 2] = [4000.0, 4000.0];

    fn member(pos: [f32; 2], group: u32) -> Attractor {
        Attractor {
            pos,
            mass: 5000.0,
            radius: 40.0,
            member: Member {
                group: GroupId(group),
                spacing: 50.0,
            },
        }
    }

    #[test]
    fn members_of_a_group_pull_each_other_together_equally() {
        let a = group_gravity(&[member([0.0, 0.0], 0), member([400.0, 0.0], 0)], SIZE);
        assert!(a[0][0] > 0.0 && a[1][0] < 0.0, "{a:?}");
        assert_eq!(a[0][0], -a[1][0]);
        assert_eq!([a[0][1], a[1][1]], [0.0, 0.0]);
    }

    #[test]
    fn heavier_members_pull_harder() {
        let mut heavy = member([400.0, 0.0], 0);
        heavy.mass *= 4.0;
        let a = group_gravity(&[member([0.0, 0.0], 0), heavy], SIZE);
        assert_eq!(a[0][0], -4.0 * a[1][0]);
    }

    #[test]
    fn different_groups_do_not_pull() {
        let a = group_gravity(&[member([0.0, 0.0], 0), member([400.0, 0.0], 1)], SIZE);
        assert_eq!(a, vec![[0.0; 2]; 2]);
    }

    #[test]
    fn members_within_their_spacing_do_not_pull() {
        // 40 + 40 + 50 apart, edge to edge exactly the group's spacing.
        let a = group_gravity(&[member([0.0, 0.0], 0), member([130.0, 0.0], 0)], SIZE);
        assert_eq!(a, vec![[0.0; 2]; 2]);
    }

    #[test]
    fn gravity_fades_out_with_distance_and_stops_at_its_reach() {
        let at = |x| group_gravity(&[member([0.0, 0.0], 0), member([x, 0.0], 0)], SIZE)[0][0];
        assert!(at(900.0) > 0.0 && at(900.0) < at(700.0));
        assert_eq!(at(GRAVITY_REACH), 0.0);
        assert_eq!(at(2000.0), 0.0);
    }

    #[test]
    fn gravity_pulls_the_short_way_across_the_wrapped_edge() {
        let a = group_gravity(&[member([1900.0, 0.0], 0), member([-1900.0, 0.0], 0)], SIZE);
        assert!(a[0][0] > 0.0, "{a:?}");
    }

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
