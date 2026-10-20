//! Drawing between ticks (ADR 0005). The sim moves in 1/60 s steps, which
//! looks like step-pause-step on faster screens, so each frame shows the world
//! blended between the last two ticks' snapshots by how far it is into the next
//! one. What's shown is up to one tick behind the sim; that's deliberate.

use sim::{Body, EntityId, Snapshot};

/// Anything that moves further than this in one tick, in px, jumped rather
/// than moved (nothing in the game goes this fast), so it's drawn where it is
/// now instead of sliding across the gap.
const MAX_STEP: f32 = 200.0;

/// The world `alpha` of the way from `prev` to `curr` (0 is `prev`, 1 is
/// `curr`), for drawing only. Entities are matched by ID; anything only in
/// `curr` (it just appeared) is drawn where it is now, and anything only in
/// `prev` (it's gone) isn't drawn.
pub fn blend(prev: &Snapshot, curr: &Snapshot, alpha: f32) -> Snapshot {
    let size = curr.size;
    let body = |before: Option<&Body>, now: Body| {
        let Some(before) = before else {
            return now;
        };
        let d = sim::wrapped_offset(before.pos, now.pos, size);
        let pos = if d[0].hypot(d[1]) > MAX_STEP {
            now.pos
        } else {
            sim::lerp_wrapped(before.pos, now.pos, alpha, size)
        };
        Body {
            pos,
            angle: sim::lerp_angle(before.angle, now.angle, alpha),
            ..now
        }
    };

    let mut shown = curr.clone();
    for ship in &mut shown.ships {
        ship.body = body(
            find(&prev.ships, ship.id, |s| s.id).map(|s| &s.body),
            ship.body,
        );
    }
    for rock in &mut shown.rocks {
        rock.body = body(
            find(&prev.rocks, rock.id, |r| r.id).map(|r| &r.body),
            rock.body,
        );
    }
    shown
}

/// The entity with `id` in `list`, which is sorted by ID.
fn find<T>(list: &[T], id: EntityId, id_of: impl Fn(&T) -> EntityId) -> Option<&T> {
    list.binary_search_by_key(&id, id_of).ok().map(|i| &list[i])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// Snapshots of two ticks in a row, with a ship thrusting and turning.
    fn ticks() -> (Snapshot, Snapshot) {
        let mut world = sim::World::new(3, [1000.0, 1000.0]);
        let ship = world.spawn_ship();
        let prev = world.snapshot();
        world.step(&BTreeMap::from([(
            ship,
            sim::Input {
                thrust: true,
                left: true,
                right: false,
            },
        )]));
        (prev, world.snapshot())
    }

    /// Whether every body in `shown` is where it is in `tick`, give or take
    /// float rounding.
    fn matches(shown: &Snapshot, tick: &Snapshot) -> bool {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let bodies = |s: &Snapshot| {
            let mut v: Vec<Body> = s.ships.iter().map(|s| s.body).collect();
            v.extend(s.rocks.iter().map(|r| r.body));
            v
        };
        bodies(shown).iter().zip(bodies(tick)).all(|(a, b)| {
            close(a.pos[0], b.pos[0]) && close(a.pos[1], b.pos[1]) && close(a.angle, b.angle)
        })
    }

    #[test]
    fn ends_match_the_ticks() {
        let (prev, curr) = ticks();
        assert!(matches(&blend(&prev, &curr, 0.0), &prev));
        assert!(matches(&blend(&prev, &curr, 1.0), &curr));
    }

    #[test]
    fn halfway_is_between_the_ticks() {
        let (prev, curr) = ticks();
        let shown = blend(&prev, &curr, 0.5);
        let (a, b, s) = (prev.ships[0].body, curr.ships[0].body, shown.ships[0].body);
        for i in 0..2 {
            assert!((s.pos[i] - (a.pos[i] + b.pos[i]) / 2.0).abs() < 1e-4);
        }
        assert!((s.angle - (a.angle + b.angle) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn jumps_are_not_blended() {
        let (prev, mut curr) = ticks();
        curr.ships[0].body.pos = [300.0, 300.0];
        assert_eq!(blend(&prev, &curr, 0.5).ships[0].body.pos, [300.0, 300.0]);
    }

    #[test]
    fn new_entities_show_where_they_are_and_gone_ones_vanish() {
        let (mut prev, mut curr) = ticks();
        // The last rock only exists in `curr`, the first only in `prev`.
        let new_rock = *curr.rocks.last().unwrap();
        prev.rocks.retain(|r| r.id != new_rock.id);
        let gone = prev.rocks[0];
        curr.rocks.retain(|r| r.id != gone.id);
        let shown = blend(&prev, &curr, 0.5);
        assert_eq!(shown.rocks.last().unwrap().body, new_rock.body);
        assert!(shown.rocks.iter().all(|r| r.id != gone.id));
    }
}
