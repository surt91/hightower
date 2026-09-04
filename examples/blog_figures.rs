//! Generates every figure of the blog post into `out/blog/`. All figures are
//! program output: the SVG renderer replays the router's trace.

use std::fs;

use hightower::grid::flood;
use hightower::svg::{
    Canvas, Layers, Scene, Style, render, render_covers, render_flood, render_path,
};
use hightower::{
    Bounds, Improvement, ObstacleSet, Orientation, Point, RouterConfig, Segment, TraceEvent,
    VisibilityConfig, VisibilityGraph, route_with,
};

fn p(x: i64, y: i64) -> Point {
    Point::new(x, y)
}

fn arena(size: i64) -> ObstacleSet {
    ObstacleSet::new(Bounds::new(p(0, 0), p(size, size)))
}

fn write(name: &str, svg: &str) {
    fs::write(format!("out/blog/{name}.svg"), svg).expect("write figure");
    println!("out/blog/{name}.svg");
}

/// Places two SVG documents side by side with a small gap.
fn two_up(left: &str, right: &str, gap: f64) -> String {
    let dims = |svg: &str| -> (f64, f64) {
        let attr = |name: &str| -> f64 {
            let key = format!("{name}=\"");
            let start = svg.find(&key).expect("attr") + key.len();
            let end = svg[start..].find('"').expect("attr end") + start;
            svg[start..end].parse().expect("number")
        };
        (attr("width"), attr("height"))
    };
    let (lw, lh) = dims(left);
    let (rw, rh) = dims(right);
    let w = lw + gap + rw;
    let h = lh.max(rh);
    let strip = |svg: &str| svg.replacen("<svg xmlns=\"http://www.w3.org/2000/svg\"", "<svg", 1);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n<svg x=\"0\" y=\"0\">{}</svg>\n<svg x=\"{:.0}\" y=\"0\">{}</svg>\n</svg>\n",
        strip(left),
        lw + gap,
        strip(right)
    )
}

/// The diagram scene: a 3x3 grid of boxes, connect two of them.
fn diagram_scene() -> (ObstacleSet, Point, Point) {
    let mut o = arena(100);
    for (x, y) in [
        (10, 10),
        (40, 10),
        (70, 10),
        (10, 45),
        (40, 45),
        (70, 45),
        (10, 75),
        (40, 75),
        (70, 75),
    ] {
        o.add_rect(p(x, y), p(x + 20, y + 15));
    }
    (o, p(20, 26), p(80, 74))
}

/// The running example of the article (figures 4, 6, 8 and 9): eight boxes,
/// a run with several steps for both networks, and a raw path with a U-turn
/// next to B that only the probing improvement removes.
fn running_scene() -> (ObstacleSet, Point, Point) {
    let mut o = arena(100);
    for (x1, y1, x2, y2) in [
        (5, 2, 34, 19),
        (13, 12, 42, 23),
        (41, 33, 55, 56),
        (30, 47, 62, 67),
        (78, 53, 87, 75),
        (72, 56, 98, 76),
        (1, 76, 31, 97),
        (59, 84, 70, 96),
    ] {
        o.add_rect(p(x1, y1), p(x2, y2));
    }
    (o, p(38, 46), p(63, 20))
}

fn fig_hero() {
    let (o, a, b) = diagram_scene();
    let scene = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style::fit(o.bounds(), 480.0);
    let grid = flood(&o, a, b);
    let left = render_flood(&scene, &grid.visited, grid.path.as_deref(), &style);
    let r = route_with(&o, a, b, &RouterConfig::default());
    let right = render(&scene, &r.trace, r.trace.len(), &style, Layers::default());
    println!(
        "hero: grid visited {} cells, Hightower constructed {} lines in {} steps",
        grid.visited.len(),
        r.trace.line_count(),
        r.steps
    );
    write("01_hero_flood_vs_hightower", &two_up(&left, &right, 30.0));
    write("01a_flood", &left);
    write("01b_hightower", &right);
}

fn fig_staircase() {
    // Two staggered boxes: the shortest path squeezes through the gap between
    // them (4 bends), the line router goes around (2 bends, longer).
    let mut o = arena(100);
    o.add_rect(p(30, 20), p(45, 55));
    o.add_rect(p(55, 45), p(70, 80));
    let (a, b) = (p(10, 50), p(90, 50));
    let scene = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style::fit(o.bounds(), 480.0);
    let grid = flood(&o, a, b);
    let left = render_path(&scene, grid.path.as_deref().expect("grid path"), &style);
    let r = route_with(&o, a, b, &RouterConfig::default());
    let right = render_path(&scene, r.path.as_deref().expect("path"), &style);
    let len = |path: &[Point]| path.windows(2).map(|w| w[0].manhattan(w[1])).sum::<i64>();
    println!(
        "staircase: grid path {} corners, length {}; Hightower {} corners, length {}",
        grid.path.as_ref().map(Vec::len).unwrap_or(0),
        grid.path.as_deref().map(len).unwrap_or(0),
        r.path.as_ref().map(Vec::len).unwrap_or(0),
        r.path.as_deref().map(len).unwrap_or(0)
    );
    write("02_shortest_vs_straight", &two_up(&left, &right, 30.0));
}

/// A point of the running scene that has all four covers, chosen so that the
/// nearest cover is as far away as possible (ties: closest to the centre).
fn point_with_four_covers(o: &ObstacleSet) -> Point {
    let b = o.bounds();
    let centre = Point::new((b.min.x + b.max.x) / 2, (b.min.y + b.max.y) / 2);
    let mut best: Option<(i64, i128, Point)> = None;
    for x in b.min.x + 1..b.max.x {
        for y in b.min.y + 1..b.max.y {
            let q = Point::new(x, y);
            let inside_box = o
                .rects()
                .iter()
                .any(|&(min, max)| min.x < q.x && q.x < max.x && min.y < q.y && q.y < max.y);
            if !o.is_free_point(q) || inside_box {
                continue;
            }
            let covers = [
                o.cover_above(q),
                o.cover_below(q),
                o.cover_left(q),
                o.cover_right(q),
            ];
            if covers.iter().any(Option::is_none) {
                continue;
            }
            let nearest = covers
                .iter()
                .flatten()
                .map(|c| (c.fixed - q.across(c.orientation)).abs())
                .min()
                .unwrap_or(0);
            let d = q.dist2(centre);
            if best.is_none_or(|(bn, bd, _)| nearest > bn || (nearest == bn && d < bd)) {
                best = Some((nearest, d, q));
            }
        }
    }
    best.expect("a point with four covers").2
}

fn fig_covers() {
    let (o, a, b) = running_scene();
    let q = point_with_four_covers(&o);
    println!("covers: point {q:?}");
    let scene = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style {
        labels: false,
        ..Style::fit(o.bounds(), 480.0)
    };
    let left = render_covers(&scene, q, &style, false);
    let right = render_covers(&scene, q, &style, true);
    write("04_covers_and_escape_lines", &two_up(&left, &right, 30.0));
}

/// Redraw of the paper's Figure 3.1: which segments cover p?
fn fig_cover_definition() {
    let mut o = arena(60);
    let covering = [
        Segment::horizontal(50, 10, 40), // above, covers
        Segment::horizontal(12, 25, 55), // below, covers
        Segment::vertical(8, 20, 45),    // left, covers
        Segment::vertical(50, 15, 35),   // right, covers
    ];
    let not_covering = [
        Segment::horizontal(40, 35, 55), // above but to the right
        Segment::vertical(20, 40, 55),   // upper left, does not reach p.y
        Segment::vertical(42, 2, 20),    // lower right, does not reach p.y
    ];
    for s in covering.iter().chain(not_covering.iter()) {
        o.add_segment(*s);
    }
    let q = p(30, 30);
    let style = Style::fit(o.bounds(), 360.0);
    let mut c = Canvas::new(o.bounds(), style.clone());
    c.frame();
    for s in &not_covering {
        c.segment(s, "#c8c8c8", style.obstacle_width, "");
    }
    for s in &covering {
        c.segment(s, style.obstacle_stroke, style.obstacle_width, "");
        // perpendicular from p to the segment's line, dotted
        let foot = match s.orientation {
            Orientation::Horizontal => p(q.x, s.fixed),
            Orientation::Vertical => p(s.fixed, q.y),
        };
        let perp = Segment::between(q, foot).expect("axis-aligned");
        c.segment(&perp, "#888888", 1.0, r#"stroke-dasharray="3 3""#);
    }
    c.dot(q, style.endpoint_fill, style.dot_radius + 1.5);
    c.text(q, 8.0, -8.0, "p", style.label);
    write("05_cover_definition", &c.finish());
}

/// Process I in three panels, taken from the first Process I step of network
/// A in the running scene and zoomed in on it.
fn fig_process_i() {
    let (o, a, b) = running_scene();
    let r = route_with(&o, a, b, &RouterConfig::default());
    let events = &r.trace.events;
    let idx = events
        .iter()
        .position(|e| {
            matches!(
                e,
                TraceEvent::EscapePoint {
                    net: hightower::NetId::A,
                    process: hightower::Process::I,
                    ..
                }
            )
        })
        .expect("A uses Process I");
    let TraceEvent::EscapePoint { point: e, .. } = events[idx] else {
        unreachable!()
    };
    // the object point Z: the last A escape point before idx, or A itself
    let z = events[..idx]
        .iter()
        .rev()
        .find_map(|ev| match ev {
            TraceEvent::EscapePoint {
                net: hightower::NetId::A,
                point,
                ..
            } => Some(*point),
            _ => None,
        })
        .unwrap_or(a);
    // the cover that e slips around: e lies one unit beyond one of its ends,
    // on the escape line of Z that is perpendicular to the new line
    let new_orientation = if e.y == z.y {
        Orientation::Vertical
    } else {
        Orientation::Horizontal
    };
    let (neg, pos) = o.bounding_covers(z, new_orientation);
    let cover = [neg, pos]
        .into_iter()
        .flatten()
        .find(|c| {
            c.endpoints().iter().any(|f| {
                (f.along(new_orientation.perpendicular())
                    - e.along(new_orientation.perpendicular()))
                .abs()
                    == 1
            })
        })
        .expect("cover next to e");
    let new_line = o.escape_line(e, new_orientation);
    let h = o.escape_line(z, Orientation::Horizontal);
    let v = o.escape_line(z, Orientation::Vertical);
    println!("process i: Z {z:?}, e {e:?}, cover {cover:?}, new line {new_line:?}");

    let pts = [z, e, cover.start(), cover.end()];
    let x0 = pts.iter().map(|q| q.x).min().unwrap() - 12;
    let x1 = pts.iter().map(|q| q.x).max().unwrap() + 12;
    let y0 = pts.iter().map(|q| q.y).min().unwrap() - 12;
    let y1 = pts.iter().map(|q| q.y).max().unwrap() + 12;
    let zoom = Bounds::new(p(x0.max(0), y0.max(0)), p(x1.min(100), y1.min(100)));
    let style = Style {
        labels: false,
        ..Style::fit(zoom, 420.0)
    };
    let base = |c: &mut Canvas| {
        c.frame();
        c.obstacles(&o);
        c.segment(&h, style.net_a, style.line_width * 1.6, "");
        c.segment(&v, style.net_a, style.line_width * 1.6, "");
        c.dot(z, style.endpoint_fill, style.dot_radius + 1.5);
        c.text(z, 8.0, 16.0, "Z", style.label);
    };
    let mut c1 = Canvas::new(zoom, style.clone());
    base(&mut c1);
    c1.segment(&cover, style.intersection, style.obstacle_width + 2.0, "");
    let mut c2 = Canvas::new(zoom, style.clone());
    base(&mut c2);
    c2.segment(&cover, style.intersection, style.obstacle_width + 2.0, "");
    c2.ring(e, style.intersection, style.dot_radius * 2.0, 2.0);
    c2.dot(e, style.net_a, style.dot_radius);
    c2.text(e, 8.0, 16.0, "e", style.label);
    let mut c3 = Canvas::new(zoom, style.clone());
    base(&mut c3);
    c3.segment(&new_line, style.net_a, style.line_width * 1.6, "");
    c3.dot(e, style.net_a, style.dot_radius);
    c3.text(e, 8.0, 16.0, "e", style.label);
    let panel12 = two_up(&c1.finish(), &c2.finish(), 20.0);
    write("06_process_i", &two_up(&panel12, &c3.finish(), 20.0));

    // overview: where the zoom window sits in the whole scene
    let full = Style::fit(o.bounds(), 300.0);
    let mut c = Canvas::new(o.bounds(), full.clone());
    c.frame();
    c.obstacles(&o);
    c.trace(
        &r.trace,
        idx + 1,
        Layers {
            final_path: false,
            ..Layers::default()
        },
    );
    c.endpoints(a, b);
    for s in hightower::svg::rect_segments(zoom.min, zoom.max) {
        c.marker_segment(&s, full.intersection, 1.5);
    }
    write("06_process_i_overview", &c.finish());
}

/// Process II: Z sits in a pocket whose lid has a gap, but a small shelf
/// right under the gap blocks the direct slip-around. The retreat position at
/// the top of Z's vertical escape line sees past the shelf.
fn process_ii_scene() -> (ObstacleSet, Point, Point) {
    let mut o = arena(60);
    o.add_segment(Segment::vertical(20, 10, 40));
    o.add_segment(Segment::vertical(30, 10, 40));
    o.add_segment(Segment::horizontal(10, 20, 30));
    o.add_segment(Segment::horizontal(25, 15, 23));
    o.add_segment(Segment::horizontal(25, 27, 35));
    o.add_segment(Segment::horizontal(20, 24, 28));
    o.add_rect(p(35, 5), p(55, 20));
    o.add_rect(p(40, 50), p(58, 58));
    (o, p(22, 16), p(50, 45))
}

fn fig_process_ii() {
    let (o, a, b) = process_ii_scene();
    let r = route_with(&o, a, b, &RouterConfig::default());
    let scene = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style::fit(o.bounds(), 480.0);
    // Only network A's first steps: find the index of the first B event after A's Process II point.
    let upto = r
        .trace
        .events
        .iter()
        .position(|e| {
            matches!(
                e,
                TraceEvent::EscapePoint {
                    net: hightower::NetId::A,
                    process: hightower::Process::I,
                    ..
                }
            )
        })
        .map(|i| i + 1)
        .unwrap_or(r.trace.len());
    let zoom = Bounds::new(p(10, 5), p(40, 35));
    let zstyle = Style {
        labels: false,
        ..Style::fit(zoom, 480.0)
    };
    let mut c = Canvas::new(zoom, zstyle.clone());
    c.frame();
    c.obstacles(&o);
    c.trace(
        &r.trace,
        upto,
        Layers {
            final_path: false,
            ..Layers::default()
        },
    );
    c.dot(a, zstyle.endpoint_fill, zstyle.dot_radius + 1.5);
    c.text(a, -20.0, 5.0, "Z", zstyle.label);
    let left = c.finish();
    let right = render(&scene, &r.trace, r.trace.len(), &style, Layers::default());
    println!(
        "process ii: {:?}, {} steps, {} lines",
        r.outcome,
        r.steps,
        r.trace.line_count()
    );
    for e in &r.trace.events {
        println!("  {e:?}");
    }
    write("07_process_ii", &two_up(&left, &right, 30.0));
}

fn fig_animation() {
    let (o, a, b) = running_scene();
    let r = route_with(&o, a, b, &RouterConfig::default());
    let scene = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style::fit(o.bounds(), 480.0);
    fs::create_dir_all("out/blog/frames").expect("frames dir");
    let layers = Layers {
        raw_path: true,
        ..Layers::default()
    };
    for upto in 0..=r.trace.len() {
        let svg = render(&scene, &r.trace, upto, &style, layers);
        fs::write(format!("out/blog/frames/frame_{upto:03}.svg"), svg).expect("write frame");
    }
    println!(
        "animation: {:?}, {} steps, {} lines, {} frames",
        r.outcome,
        r.steps,
        r.trace.line_count(),
        r.trace.len() + 1
    );
    write(
        "08_full_run",
        &render(&scene, &r.trace, r.trace.len(), &style, Layers::default()),
    );
    write(
        "08_full_run_path",
        &render_path(&scene, r.path.as_deref().expect("path"), &style),
    );
}

fn fig_refine() {
    let (o, a, b) = running_scene();
    let scene = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style::fit(o.bounds(), 480.0);
    let raw = route_with(
        &o,
        a,
        b,
        &RouterConfig {
            improve: Improvement::None,
            ..Default::default()
        },
    );
    let full = route_with(
        &o,
        a,
        b,
        &RouterConfig {
            improve: Improvement::Full,
            ..Default::default()
        },
    );
    println!(
        "refine: raw {:?} corners, full {:?} corners",
        raw.path.as_ref().map(Vec::len),
        full.path.as_ref().map(Vec::len)
    );
    let left = render(
        &scene,
        &raw.trace,
        raw.trace.len(),
        &style,
        Layers::default(),
    );
    let right = render_path(&scene, full.path.as_deref().expect("path"), &style);
    write("09_raw_vs_refined", &two_up(&left, &right, 30.0));
}

fn fig_counterexample() {
    let mut o = arena(60);
    o.add_segment(Segment::horizontal(8, 3, 56));
    o.add_segment(Segment::horizontal(27, 43, 60));
    o.add_segment(Segment::horizontal(45, 3, 56));
    o.add_segment(Segment::vertical(3, 8, 45));
    o.add_segment(Segment::vertical(7, 49, 60));
    o.add_segment(Segment::vertical(56, 8, 15));
    o.add_segment(Segment::vertical(56, 17, 45));
    let (a, b) = (p(23, 55), p(42, 12));
    let r = route_with(&o, a, b, &RouterConfig::default());
    let grid = flood(&o, a, b);
    let style = Style::fit(o.bounds(), 480.0);
    let mut c = Canvas::new(o.bounds(), style.clone());
    c.frame();
    c.obstacles(&o);
    if let Some(gp) = &grid.path {
        c.polyline(gp, "#aaaaaa", style.path_width, r#"stroke-dasharray="8 6""#);
    }
    c.trace(&r.trace, r.trace.len(), Layers::default());
    c.endpoints(a, b);
    println!(
        "counterexample: {:?}, {} steps, {} lines",
        r.outcome,
        r.steps,
        r.trace.line_count()
    );
    write("11_incompleteness", &c.finish());
}

/// The orthogonal visibility graph of the diagram scene, with the A* path
/// (bend penalty 0 left, 20 right), next to Hightower's five lines.
fn fig_visibility_graph() {
    let (o, a, b) = diagram_scene();
    let scene = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style::fit(o.bounds(), 480.0);
    let graph = VisibilityGraph::new(&o, &[a, b]);
    let edges = graph.edges();
    let mut panels = Vec::new();
    for penalty in [0, 20] {
        let r = graph.route(
            a,
            b,
            &VisibilityConfig {
                bend_penalty: penalty,
            },
        );
        let path = r.path.as_deref().expect("path");
        println!(
            "visibility (penalty {penalty}): {} nodes, {} edges, {} expanded, {} corners, cost {:?}",
            r.graph_nodes,
            edges.len(),
            r.expanded,
            path.len(),
            r.cost
        );
        let mut c = Canvas::new(o.bounds(), style.clone());
        c.frame();
        for e in &edges {
            c.segment(e, "#b8c4d8", 1.0, "");
        }
        for &x in graph.xs() {
            for &y in graph.ys() {
                let q = p(x, y);
                if o.is_free_point(q) {
                    c.dot(q, "#8fa3c4", 1.3);
                }
            }
        }
        c.obstacles(&o);
        c.polyline(path, style.path, style.path_width, "");
        c.endpoints(a, b);
        panels.push(c.finish());
    }
    write("13_visibility_graph", &two_up(&panels[0], &panels[1], 30.0));
    let _ = scene;
}

fn main() {
    fs::create_dir_all("out/blog").expect("create out/blog");
    fig_visibility_graph();
    fig_hero();
    fig_staircase();
    fig_covers();
    fig_cover_definition();
    fig_process_i();
    fig_process_ii();
    fig_animation();
    fig_refine();
    fig_counterexample();
}
