//! Draws a seed's whole map as an SVG, for tuning how the field is laid out:
//! at the start, and again after some seconds of drifting with no ships, so
//! you can see whether groups hold together and lanes stay open. Rocks are
//! colored by group; standalone ones are grey.
//!
//!     cargo run -p sim --example field_svg -- [seed] [seconds]
//!
//! Writes `field-<seed>.svg` and prints each group's spacing at both times.

// Only draws the map, outside the sim, so std's float methods are fine here.
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::fmt::Write;

use sim::{DT, GroupId, MAP_SIZE, RockState, SHAPE_POINTS, Snapshot, World, wrapped_offset};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().map_or(1, |s| s.parse().expect("seed"));
    let seconds: f32 = args.next().map_or(300.0, |s| s.parse().expect("seconds"));

    let mut world = World::new(seed, MAP_SIZE);
    let start = world.snapshot();
    for _ in 0..(seconds / DT).round() as u32 {
        world.step(&BTreeMap::new());
    }
    let end = world.snapshot();

    let [w, h] = MAP_SIZE;
    let mut svg = String::new();
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" style="background:#0b0d14">"#,
        2.0 * w + 100.0,
        h + 80.0
    )
    .unwrap();
    writeln!(svg, r#"<clipPath id="map"><rect width="{w}" height="{h}"/></clipPath>"#).unwrap();
    for (panel, (snapshot, label)) in [(&start, "start".to_string()), (&end, format!("after {seconds} s"))]
        .into_iter()
        .enumerate()
    {
        let x0 = panel as f32 * (w + 100.0);
        writeln!(svg, r#"<g transform="translate({x0} 80)">"#).unwrap();
        writeln!(svg, r##"<text y="-25" fill="#ccc" font-size="48" font-family="sans-serif">seed {seed}, {label}</text>"##).unwrap();
        writeln!(svg, r##"<rect width="{w}" height="{h}" fill="none" stroke="#445" stroke-width="4"/>"##).unwrap();
        writeln!(svg, r#"<g clip-path="url(#map)">"#).unwrap();
        // The spawn point, with a lane's width around it.
        writeln!(svg, r##"<circle cx="{}" cy="{}" r="300" fill="none" stroke="#556" stroke-dasharray="20 20" stroke-width="4"/>"##, w / 2.0, h / 2.0).unwrap();
        for rock in &snapshot.rocks {
            draw_rock(&mut svg, rock);
        }
        writeln!(svg, "</g></g>").unwrap();
    }
    writeln!(svg, "</svg>").unwrap();

    let path = format!("field-{seed}.svg");
    std::fs::write(&path, svg).expect("write the SVG");
    println!("wrote {path}");
    println!("group  rocks  median gap to nearest member: start → end");
    for (group, start_gap) in nearest_gaps(&start) {
        let end_gap = nearest_gaps(&end)[&group];
        let count = start.rocks.iter().filter(|r| r.group == Some(group)).count();
        println!("{:>5}  {count:>5}  {start_gap:>6.0} → {end_gap:.0}", group.0);
    }
}

/// A rock's outline, in its group's color, drawn again wherever it wraps
/// across an edge.
fn draw_rock(svg: &mut String, rock: &RockState) {
    let [w, h] = MAP_SIZE;
    let fill = match rock.group {
        Some(g) => format!("hsl({:.0} 65% 55%)", (g.0 as f32 * 137.508) % 360.0),
        None => "#888".to_string(),
    };
    let r = rock.asteroid.radius;
    for dx in [-w, 0.0, w] {
        for dy in [-h, 0.0, h] {
            // Map coordinates are centered with +y up; SVG's start at the top
            // left with +y down.
            let cx = rock.body.pos[0] + dx + w / 2.0;
            let cy = h / 2.0 - (rock.body.pos[1] + dy);
            if cx + r < 0.0 || cx - r > w || cy + r < 0.0 || cy - r > h {
                continue;
            }
            let points: Vec<String> = (0..SHAPE_POINTS)
                .map(|i| {
                    let a = rock.body.angle + i as f32 * TAU / SHAPE_POINTS as f32;
                    let d = rock.asteroid.shape[i];
                    format!("{:.1},{:.1}", cx + d * a.cos(), cy - d * a.sin())
                })
                .collect();
            writeln!(svg, r#"<polygon points="{}" fill="{fill}" fill-opacity="0.8"/>"#, points.join(" ")).unwrap();
        }
    }
}

/// For each group, the mean edge-to-edge gap from each member to its nearest
/// fellow member, in px.
fn nearest_gaps(snapshot: &Snapshot) -> BTreeMap<GroupId, f32> {
    let mut gaps: BTreeMap<GroupId, Vec<f32>> = BTreeMap::new();
    for a in &snapshot.rocks {
        let Some(group) = a.group else { continue };
        let nearest = snapshot
            .rocks
            .iter()
            .filter(|b| b.group == Some(group) && b.id != a.id)
            .map(|b| {
                let d = wrapped_offset(a.body.pos, b.body.pos, snapshot.size);
                d[0].hypot(d[1]) - a.asteroid.radius - b.asteroid.radius
            })
            .fold(f32::INFINITY, f32::min);
        if nearest.is_finite() {
            gaps.entry(group).or_default().push(nearest);
        }
    }
    // The median, so a rock or two knocked out of the group doesn't skew it.
    gaps.into_iter()
        .map(|(g, mut v)| {
            v.sort_by(f32::total_cmp);
            (g, v[v.len() / 2])
        })
        .collect()
}
