//! Lays out the starting field: small groups of asteroids at different
//! densities, each strung along a swish (a curved band tapering to points),
//! lanes between them, and standalone asteroids (ADRs 0010, 0011).
//!
//! Every rock is placed by trying random spots and keeping the first that
//! meets all its spacing rules, or else the one that comes closest. Rules come
//! in two strengths: hard ones (never overlap another rock, stay clear of the
//! spawn point) always win over soft ones (lane width, a group's spacing), so a
//! map too small for lanes still gets every rock without overlaps.

use std::f32::consts::{PI, TAU};

use crate::asteroid::{Asteroid, Class, SPAWN_CLEARANCE, Spin, random_shape};
use crate::forces::Drift;
use crate::{Body, Rng, math, wrap, wrapped_offset};

/// How many rocks of each class the map starts with per million px². The
/// 4000×4000 map gets 14, 20, 58 and 6; a 1280×800 one gets 1, 1, 4 and none.
const ROCKS_PER_MPX: [(Class, f32); 4] = [
    (Class::Large, 0.875),
    (Class::Big, 1.25),
    (Class::Medium, 3.625),
    (Class::Small, 0.375),
];
/// How many groups the map has per million px², at least one. The 4000×4000
/// map gets 16, of about 5 rocks each.
const GROUPS_PER_MPX: f32 = 1.0;
/// How many of the groups are dense and moderate; the rest are sparse.
const DENSE_SHARE: f32 = 0.25;
const MODERATE_SHARE: f32 = 0.375;
/// The open space kept between groups at spawn, edge to edge, in px: a lane.
/// Also kept around the spawn point.
pub(crate) const LANE_WIDTH: f32 = 300.0;
/// How much of a group's swish its rocks (with half their spacing around
/// each) fill. Lower makes roomier groups that are easier to fill.
const PACKING: f32 = 0.35;
/// How many times longer than wide a swish is, at its middle.
const SWISH_ASPECT: (f32, f32) = (3.0, 6.0);
/// How far a swish's centerline turns from one end to the other, in radians.
const SWISH_BEND: (f32, f32) = (0.5, 1.5);
/// How wide a swish's ends are, as a share of its middle's width.
const SWISH_TIP: f32 = 0.3;
/// Random spots tried per group or rock before settling for the best.
const TRIES: usize = 100;
/// Rocks never start closer than this to each other, in px.
const MIN_GAP: f32 = 4.0;
/// A group's speed range, in px/s.
const GROUP_SPEED: (f32, f32) = (5.0, 12.0);
/// How far a member's starting velocity strays from its group's, in px/s.
const VELOCITY_JITTER: f32 = 2.0;

/// Names one group. Groups exist only as the IDs their members carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(pub u32);

/// Makes a rock a member of a group, for its whole life. Standalone rocks
/// don't have one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Member {
    pub group: GroupId,
    /// The group's natural gap between neighbours' edges, in px. Gravity
    /// doesn't pull members closer than this.
    pub spacing: f32,
}

/// How tightly a group is packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Density {
    Dense,
    Moderate,
    Sparse,
}

impl Density {
    /// The smallest gap between neighbouring members' edges, in px. A dense
    /// group just lets a ship squeeze through; a sparse one is easy flying.
    fn spacing(self) -> f32 {
        match self {
            Density::Dense => 30.0,
            Density::Moderate => 80.0,
            Density::Sparse => 150.0,
        }
    }

    /// How strongly medium and small rocks are dealt to this kind of group.
    fn fill_weight(self) -> f32 {
        match self {
            Density::Dense => 3.0,
            Density::Moderate => 2.0,
            Density::Sparse => 1.0,
        }
    }
}

/// One starting rock, with its group if it's in one.
pub(crate) struct NewRock {
    pub body: Body,
    pub spin: Spin,
    pub asteroid: Asteroid,
    pub drift: Drift,
    pub member: Option<Member>,
}

/// A group while the field is being laid out.
struct Group {
    density: Density,
    /// Indices of its rocks.
    members: Vec<usize>,
    center: [f32; 2],
    /// The swish's length along its centerline and width at its middle, in px.
    length: f32,
    width: f32,
    /// How far its centerline turns from end to end, in radians, either way.
    bend: f32,
    /// The direction its middle runs in, in radians.
    tilt: f32,
    vel: [f32; 2],
    floor: f32,
}

impl Group {
    /// The radius of a circle around its center that holds the whole swish.
    fn reach(&self) -> f32 {
        (self.length + self.width) / 2.0
    }
}

/// A rock that has been given a spot.
struct Placed {
    pos: [f32; 2],
    radius: f32,
    group: Option<usize>,
}

/// Lay out the starting rocks around `spawn_point`, in the order they should
/// be given IDs.
pub(crate) fn lay_out(rng: &mut Rng, size: [f32; 2], spawn_point: [f32; 2]) -> Vec<NewRock> {
    let area = size[0] * size[1] / 1e6;
    let mut asteroids: Vec<Asteroid> = ROCKS_PER_MPX
        .iter()
        .flat_map(|&(class, per_mpx)| std::iter::repeat_n(class, (per_mpx * area).round() as usize))
        .map(|class| Asteroid::new(class, random_shape(rng, class)))
        .collect();

    let standalone = pick_standalone(&asteroids);
    let mut groups = form_groups(rng, &asteroids, &standalone, area);
    place_groups(rng, &mut groups, size, spawn_point);

    let group_of: Vec<Option<usize>> = (0..asteroids.len())
        .map(|i| groups.iter().position(|g| g.members.contains(&i)))
        .collect();
    // Largest first, so the big rocks get the clearest spots. Members are
    // tried inside their group's swish, standalone rocks anywhere.
    let mut placed: Vec<Placed> = Vec::new();
    let mut pos = vec![[0.0; 2]; asteroids.len()];
    let all: Vec<usize> = (0..asteroids.len()).collect();
    for i in largest_first(&all, &asteroids) {
        let group = group_of[i];
        let radius = asteroids[i].radius;
        let spot = best_spot(rng, size, spawn_point, &placed, radius, group, &groups, |rng| match group {
            Some(g) => in_swish(rng, &groups[g], size),
            None => anywhere(rng, size),
        });
        pos[i] = spot;
        placed.push(Placed {
            pos: spot,
            radius,
            group,
        });
    }

    asteroids
        .iter_mut()
        .enumerate()
        .map(|(i, asteroid)| {
            let params = asteroid.class.params();
            let (vel, floor, member) = match group_of[i] {
                Some(g) => {
                    let group = &groups[g];
                    let jitter = in_disc(rng, VELOCITY_JITTER);
                    let member = Member {
                        group: GroupId(g as u32),
                        spacing: group.density.spacing(),
                    };
                    ([0, 1].map(|k| group.vel[k] + jitter[k]), group.floor, Some(member))
                }
                None => {
                    let speed = rng.range(params.speed.0, params.speed.1);
                    (heading(rng, speed), params.drift_floor, None)
                }
            };
            let body = Body {
                pos: pos[i],
                vel,
                angle: rng.range(0.0, TAU),
            };
            let spin = Spin(rng.range(-params.max_spin, params.max_spin));
            asteroid.look = rng.next_u32();
            NewRock {
                body,
                spin,
                asteroid: *asteroid,
                drift: Drift { floor },
                member,
            }
        })
        .collect()
}

/// Which rocks start standalone: every large rock (a lone giant would make a
/// group too wide to read as a swish) and every small one (loose debris).
fn pick_standalone(asteroids: &[Asteroid]) -> Vec<usize> {
    (0..asteroids.len())
        .filter(|&i| matches!(asteroids[i].class, Class::Large | Class::Small))
        .collect()
}

/// Make the groups and deal every rock that isn't standalone into one, then
/// size each group's swish to fit its rocks and give it a velocity.
fn form_groups(rng: &mut Rng, asteroids: &[Asteroid], standalone: &[usize], area: f32) -> Vec<Group> {
    let grouped: Vec<usize> = (0..asteroids.len()).filter(|i| !standalone.contains(i)).collect();
    let count = ((GROUPS_PER_MPX * area).round() as usize).max(1).min(grouped.len());
    let dense = (DENSE_SHARE * count as f32).round() as usize;
    let moderate = ((MODERATE_SHARE * count as f32).round() as usize).min(count - dense);
    let mut groups: Vec<Group> = (0..count)
        .map(|g| Group {
            density: if g < dense {
                Density::Dense
            } else if g < dense + moderate {
                Density::Moderate
            } else {
                Density::Sparse
            },
            members: Vec::new(),
            center: [0.0; 2],
            length: 0.0,
            width: 0.0,
            bend: 0.0,
            tilt: 0.0,
            vel: [0.0; 2],
            floor: 0.0,
        })
        .collect();
    if groups.is_empty() {
        return groups;
    }

    // Big rocks anchor the sparser groups, spread evenly; medium ones fill
    // the denser groups most.
    let anchors_allowed: Vec<usize> = match (0..count).filter(|&g| groups[g].density != Density::Dense).collect::<Vec<_>>() {
        v if v.is_empty() => (0..count).collect(),
        v => v,
    };
    // Anchors don't count towards the fill, or they'd push it all into the
    // dense groups.
    let mut anchors = vec![0usize; count];
    let mut fill = vec![0usize; count];
    for class in [Class::Large, Class::Big, Class::Medium, Class::Small] {
        for &i in grouped.iter().filter(|&&i| asteroids[i].class == class) {
            let g = if matches!(class, Class::Large | Class::Big) {
                let g = *anchors_allowed.iter().min_by_key(|&&g| anchors[g]).expect("not empty");
                anchors[g] += 1;
                g
            } else {
                let load = |g: usize| (fill[g] + 1) as f32 / groups[g].density.fill_weight();
                let g = (0..count).min_by(|&a, &b| load(a).total_cmp(&load(b))).expect("not empty");
                fill[g] += 1;
                g
            };
            groups[g].members.push(i);
        }
    }

    for group in &mut groups {
        let spacing = group.density.spacing();
        let footprint: f32 = group
            .members
            .iter()
            .map(|&i| {
                let r = asteroids[i].radius + spacing / 2.0;
                PI * r * r
            })
            .sum::<f32>()
            / PACKING;
        // Its area is about length × width × the average taper, and it's at
        // least wide enough for its biggest rock.
        let taper = (2.0 + SWISH_TIP) / 3.0;
        let aspect = rng.range(SWISH_ASPECT.0, SWISH_ASPECT.1);
        let widest = group
            .members
            .iter()
            .map(|&i| 2.0 * asteroids[i].radius + spacing)
            .fold(0.0, f32::max);
        group.width = (footprint / (aspect * taper)).sqrt().max(widest);
        group.length = footprint / (group.width * taper);
        let bend = rng.range(SWISH_BEND.0, SWISH_BEND.1);
        group.bend = if rng.range(0.0, 1.0) < 0.5 { -bend } else { bend };
        group.tilt = rng.range(0.0, TAU);
        let speed = rng.range(GROUP_SPEED.0, GROUP_SPEED.1);
        group.vel = heading(rng, speed);
        // Below the slowest a member can start, so the floor never pushes
        // members apart.
        group.floor = (speed - VELOCITY_JITTER).max(0.0);
    }
    groups
}

/// Spread the groups' centers over the map, biggest first, each a lane's width
/// from the others and from the spawn point. Swishes are treated as circles
/// that hold them here; the rocks themselves keep the exact lane widths.
fn place_groups(rng: &mut Rng, groups: &mut [Group], size: [f32; 2], spawn_point: [f32; 2]) {
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| groups[b].reach().total_cmp(&groups[a].reach()));
    for (n, &g) in order.iter().enumerate() {
        let r = groups[g].reach();
        let mut best = ([0.0; 2], f32::NEG_INFINITY);
        for _ in 0..TRIES {
            let c = anywhere(rng, size);
            let slack = order[..n]
                .iter()
                .map(|&o| distance(c, groups[o].center, size) - r - groups[o].reach() - LANE_WIDTH)
                .fold(distance(c, spawn_point, size) - r - LANE_WIDTH, f32::min);
            if slack > best.1 {
                best = (c, slack);
            }
            if slack >= 0.0 {
                break;
            }
        }
        groups[g].center = best.0;
    }
}

/// Try spots drawn by `candidate` for a rock of `radius` in group `group` (or
/// standalone), and return the first that meets every rule, or else the best.
/// If none of them meets the hard rules, try anywhere on the map as well, so a
/// group too cramped to hold a rock never makes it overlap another.
#[allow(clippy::too_many_arguments)]
fn best_spot(
    rng: &mut Rng,
    size: [f32; 2],
    spawn_point: [f32; 2],
    placed: &[Placed],
    radius: f32,
    group: Option<usize>,
    groups: &[Group],
    mut candidate: impl FnMut(&mut Rng) -> [f32; 2],
) -> [f32; 2] {
    let mut best = ([0.0; 2], (f32::NEG_INFINITY, f32::NEG_INFINITY));
    for attempt in 0..2 * TRIES {
        if attempt == TRIES && best.1.0 >= 0.0 {
            break;
        }
        let pos = if attempt < TRIES {
            candidate(rng)
        } else {
            anywhere(rng, size)
        };
        let to_spawn = distance(pos, spawn_point, size) - radius;
        let mut hard = to_spawn - SPAWN_CLEARANCE;
        let mut soft = to_spawn - LANE_WIDTH;
        for other in placed {
            let gap = distance(pos, other.pos, size) - radius - other.radius;
            hard = hard.min(gap - MIN_GAP);
            let wanted = match (group, other.group) {
                (Some(a), Some(b)) if a == b => groups[a].density.spacing(),
                (Some(_), Some(_)) | (None, None) => LANE_WIDTH,
                (Some(_), None) | (None, Some(_)) => LANE_WIDTH / 2.0,
            };
            soft = soft.min(gap - wanted);
        }
        // Hard rules first: any spot that breaks one loses to every spot that
        // doesn't.
        let score = (hard.min(0.0), soft);
        if score.0 > best.1.0 || (score.0 == best.1.0 && score.1 > best.1.1) {
            best = (pos, score);
        }
        if hard >= 0.0 && soft >= 0.0 {
            break;
        }
    }
    best.0
}

/// A random point on the map.
fn anywhere(rng: &mut Rng, size: [f32; 2]) -> [f32; 2] {
    [0, 1].map(|k| rng.range(-size[k] / 2.0, size[k] / 2.0))
}

/// A random point inside `group`'s swish, wrapped onto the map.
fn in_swish(rng: &mut Rng, group: &Group, size: [f32; 2]) -> [f32; 2] {
    // How far along the centerline, from -½ to ½, and how far across it, as a
    // share of the width there, which narrows toward the tips.
    let along = rng.range(-0.5, 0.5);
    let taper = 1.0 - (1.0 - SWISH_TIP) * 4.0 * along * along;
    let across = rng.range(-0.5, 0.5) * group.width * taper;
    // The centerline is an arc through the center, heading along +x there.
    let turn = group.bend * along;
    let (turn_sin, turn_cos) = math::sin_cos(turn);
    let r = group.length / group.bend;
    let on_line = [r * turn_sin, r * (1.0 - turn_cos)];
    let local = [on_line[0] - across * turn_sin, on_line[1] + across * turn_cos];
    let (sin, cos) = math::sin_cos(group.tilt);
    let turned = [local[0] * cos - local[1] * sin, local[0] * sin + local[1] * cos];
    [0, 1].map(|k| wrap(group.center[k] + turned[k], size[k]))
}

/// A uniformly random point in a disc of `radius` around the origin.
fn in_disc(rng: &mut Rng, radius: f32) -> [f32; 2] {
    let r = radius * rng.range(0.0, 1.0).sqrt();
    heading(rng, r)
}

/// A vector of length `len` pointing a random way.
fn heading(rng: &mut Rng, len: f32) -> [f32; 2] {
    let (sin, cos) = math::sin_cos(rng.range(0.0, TAU));
    [cos * len, sin * len]
}

/// `rocks` by outline size, largest first, so the big ones get the clearest
/// spots. Ties keep their order.
fn largest_first(rocks: &[usize], asteroids: &[Asteroid]) -> Vec<usize> {
    let mut sorted = rocks.to_vec();
    sorted.sort_by(|&a, &b| asteroids[b].radius.total_cmp(&asteroids[a].radius));
    sorted
}

/// Distance from `a` to `b` the short way across the wrapped edges.
fn distance(a: [f32; 2], b: [f32; 2], size: [f32; 2]) -> f32 {
    let d = wrapped_offset(a, b, size);
    math::hypot(d[0], d[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAP_SIZE, SPAWN_POINT};

    fn field(seed: u64) -> Vec<NewRock> {
        lay_out(&mut Rng::new(seed), MAP_SIZE, SPAWN_POINT)
    }

    #[test]
    fn large_and_small_rocks_are_standalone_and_the_rest_are_grouped() {
        for rock in field(1) {
            let standalone = matches!(rock.asteroid.class, Class::Large | Class::Small);
            assert_eq!(rock.member.is_none(), standalone, "{:?}", rock.asteroid.class);
        }
    }

    #[test]
    fn groups_are_small() {
        let rocks = field(1);
        let mut sizes = std::collections::BTreeMap::new();
        for member in rocks.iter().filter_map(|r| r.member) {
            *sizes.entry(member.group).or_insert(0) += 1;
        }
        assert_eq!(sizes.len(), 16);
        assert!(sizes.values().all(|&n| (2..=8).contains(&n)), "{sizes:?}");
    }

    #[test]
    fn swish_points_stay_within_its_reach() {
        let group = Group {
            density: Density::Moderate,
            members: Vec::new(),
            center: [0.0; 2],
            length: 1000.0,
            width: 200.0,
            bend: 1.2,
            tilt: 0.7,
            vel: [0.0; 2],
            floor: 0.0,
        };
        let mut rng = Rng::new(7);
        for _ in 0..1000 {
            let p = in_swish(&mut rng, &group, MAP_SIZE);
            assert!(math::hypot(p[0], p[1]) <= group.reach(), "{p:?}");
        }
    }
}
