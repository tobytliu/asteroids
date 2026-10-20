//! Drawing between ticks (ADR 0005). The sim moves in 1/60 s steps, which
//! looks like step-pause-step on faster screens, so each frame shows the world
//! blended between the last two ticks by how far it is into the next one. What's
//! shown is up to one tick behind the sim; that's deliberate.

use sim::World;

/// Anything that moves further than this in one tick, in px, jumped rather
/// than moved (nothing in the game goes this fast), so it's drawn where it is
/// now instead of sliding across the gap.
const MAX_STEP: f32 = 200.0;

/// The world `alpha` of the way from `prev` to `curr` (0 is `prev`, 1 is
/// `curr`), for drawing only.
pub fn blend(prev: &World, curr: &World, alpha: f32) -> World {
    let mut shown = curr.clone();
    let size = curr.size;
    let pos = |a: [f32; 2], b: [f32; 2]| {
        let d = sim::wrapped_offset(a, b, size);
        if d[0].hypot(d[1]) > MAX_STEP {
            b
        } else {
            sim::lerp_wrapped(a, b, alpha, size)
        }
    };

    shown.ship.pos = pos(prev.ship.pos, curr.ship.pos);
    shown.ship.angle = sim::lerp_angle(prev.ship.angle, curr.ship.angle, alpha);
    // Rocks are matched by index, which holds while none appear or go away.
    // When that changes this needs entity IDs; until then a changed count just
    // shows the current tick.
    if prev.asteroids.len() == curr.asteroids.len() {
        for (rock, before) in shown.asteroids.iter_mut().zip(&prev.asteroids) {
            rock.pos = pos(before.pos, rock.pos);
            rock.angle = sim::lerp_angle(before.angle, rock.angle, alpha);
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worlds() -> (World, World) {
        let prev = World::new(3, [1000.0, 1000.0]);
        let mut curr = prev.clone();
        curr.step(sim::Input {
            thrust: true,
            left: true,
            right: false,
        });
        (prev, curr)
    }

    /// Whether everything blended in `shown` is where it is in `tick`, give or
    /// take float rounding.
    fn matches(shown: &World, tick: &World) -> bool {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let things = |w: &World| {
            let mut v = vec![(w.ship.pos, w.ship.angle)];
            v.extend(w.asteroids.iter().map(|r| (r.pos, r.angle)));
            v
        };
        things(shown)
            .iter()
            .zip(things(tick))
            .all(|((p, a), (q, b))| close(p[0], q[0]) && close(p[1], q[1]) && close(*a, b))
    }

    #[test]
    fn ends_match_the_ticks() {
        let (prev, curr) = worlds();
        assert!(matches(&blend(&prev, &curr, 0.0), &prev));
        assert!(matches(&blend(&prev, &curr, 1.0), &curr));
    }

    #[test]
    fn halfway_is_between_the_ticks() {
        let (prev, curr) = worlds();
        let shown = blend(&prev, &curr, 0.5);
        for i in 0..2 {
            let mid = (prev.ship.pos[i] + curr.ship.pos[i]) / 2.0;
            assert!((shown.ship.pos[i] - mid).abs() < 1e-4);
        }
        assert!((shown.ship.angle - (prev.ship.angle + curr.ship.angle) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn jumps_are_not_blended() {
        let (prev, mut curr) = worlds();
        curr.ship.pos = [300.0, 300.0];
        assert_eq!(blend(&prev, &curr, 0.5).ship.pos, [300.0, 300.0]);
    }
}
