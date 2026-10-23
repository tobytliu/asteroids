//! Lays out the starting field: groups of asteroids at different densities,
//! lanes between them, and a few standalone asteroids (ADR 0010).
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
/// map gets 8.
const GROUPS_PER_MPX: f32 = 0.5;
/// How many of the groups are dense and moderate; the rest are sparse.
const DENSE_SHARE: f32 = 0.25;
const MODERATE_SHARE: f32 = 0.375;
/// The share of rocks that start standalone rather than in a group.
const STANDALONE_SHARE: f32 = 0.1;
/// The open space kept between groups at spawn, edge to edge, in px: a lane.
/// Also kept around the spawn point.
pub(crate) const LANE_WIDTH: f32 = 300.0;
/// How much of a group's ellipse its rocks (with half their spacing around
/// each) fill. Lower makes roomier groups that are easier to fill.
const PACKING: f32 = 0.5;
/// Random spots tried per group or rock before settling for the best.
const TRIES: usize = 100;
/// Rocks never start closer than this to each other, in px.
const MIN_GAP: f32 = 4.0;
/// A group's speed range, in px/s, and a slower one for groups with a large
/// rock in them.
const GROUP_SPEED: (f32, f32) = (5.0, 12.0);
const GROUP_SPEED_WITH_LARGE: (f32, f32) = (3.0, 8.0);
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
    /// The ellipse's semi-axes, in px, before tilting.
    semi: [f32; 2],
    tilt: f32,
    vel: [f32; 2],
    floor: f32,
}

impl Group {
    /// The radius of a circle with the ellipse's area.
    fn mean_radius(&self) -> f32 {
        (self.semi[0] * self.semi[1]).sqrt()
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
    // tried inside their group's ellipse, standalone rocks anywhere.
    let mut placed: Vec<Placed> = Vec::new();
    let mut pos = vec![[0.0; 2]; asteroids.len()];
    let all: Vec<usize> = (0..asteroids.len()).collect();
    for i in largest_first(&all, &asteroids) {
        let group = group_of[i];
        let radius = asteroids[i].radius;
        let spot = best_spot(rng, size, spawn_point, &placed, radius, group, &groups, |rng| match group {
            Some(g) => in_ellipse(rng, &groups[g], size),
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

/// Which rocks start standalone: `STANDALONE_SHARE` of them, taking large and
/// small rocks in turn (lone giants and loose debris), then big, then medium.
fn pick_standalone(asteroids: &[Asteroid]) -> Vec<usize> {
    let count = (STANDALONE_SHARE * asteroids.len() as f32).round() as usize;
    let mut pools = [Class::Large, Class::Small, Class::Big, Class::Medium].map(|class| {
        (0..asteroids.len())
            .filter(|&i| asteroids[i].class == class)
            .rev()
            .collect::<Vec<usize>>()
    });
    let mut picked = Vec::new();
    let mut turn = 0;
    while picked.len() < count {
        let pool = if pools[0].is_empty() && pools[1].is_empty() {
            if pools[2].is_empty() { 3 } else { 2 }
        } else {
            let k = turn % 2;
            turn += 1;
            if pools[k].is_empty() { 1 - k } else { k }
        };
        picked.extend(pools[pool].pop());
    }
    picked
}

/// Make the groups and deal every rock that isn't standalone into one, then
/// size each group's ellipse to fit its rocks and give it a velocity.
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
            semi: [0.0; 2],
            tilt: 0.0,
            vel: [0.0; 2],
            floor: 0.0,
        })
        .collect();
    if groups.is_empty() {
        return groups;
    }

    // Large and big rocks anchor the sparser groups, spread evenly; medium and
    // small ones fill the denser groups most.
    let anchors_allowed: Vec<usize> = match (0..count).filter(|&g| groups[g].density != Density::Dense).collect::<Vec<_>>() {
        v if v.is_empty() => (0..count).collect(),
        v => v,
    };
    let mut anchors = vec![0usize; count];
    for class in [Class::Large, Class::Big, Class::Medium, Class::Small] {
        for &i in grouped.iter().filter(|&&i| asteroids[i].class == class) {
            let g = if matches!(class, Class::Large | Class::Big) {
                let g = *anchors_allowed.iter().min_by_key(|&&g| anchors[g]).expect("not empty");
                anchors[g] += 1;
                g
            } else {
                let load = |g: usize| (groups[g].members.len() + 1) as f32 / groups[g].density.fill_weight();
                (0..count).min_by(|&a, &b| load(a).total_cmp(&load(b))).expect("not empty")
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
        let aspect = rng.range(1.0, 2.5);
        group.semi = [(footprint * aspect / PI).sqrt(), (footprint / (aspect * PI)).sqrt()];
        group.tilt = rng.range(0.0, PI);
        let has_large = group.members.iter().any(|&i| asteroids[i].class == Class::Large);
        let range = if has_large { GROUP_SPEED_WITH_LARGE } else { GROUP_SPEED };
        let speed = rng.range(range.0, range.1);
        group.vel = heading(rng, speed);
        // Below the slowest a member can start, so the floor never pushes
        // members apart.
        group.floor = (speed - VELOCITY_JITTER).max(0.0);
    }
    groups
}

/// Spread the groups' centers over the map, biggest first, each a lane's width
/// from the others and from the spawn point. Ellipses are treated as circles of
/// the same area here; the rocks themselves keep the exact lane widths.
fn place_groups(rng: &mut Rng, groups: &mut [Group], size: [f32; 2], spawn_point: [f32; 2]) {
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| groups[b].mean_radius().total_cmp(&groups[a].mean_radius()));
    for (n, &g) in order.iter().enumerate() {
        let r = groups[g].mean_radius();
        let mut best = ([0.0; 2], f32::NEG_INFINITY);
        for _ in 0..TRIES {
            let c = anywhere(rng, size);
            let slack = order[..n]
                .iter()
                .map(|&o| distance(c, groups[o].center, size) - r - groups[o].mean_radius() - LANE_WIDTH)
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

/// A random point inside `group`'s ellipse, wrapped onto the map.
fn in_ellipse(rng: &mut Rng, group: &Group, size: [f32; 2]) -> [f32; 2] {
    let p = in_disc(rng, 1.0);
    let local = [p[0] * group.semi[0], p[1] * group.semi[1]];
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
