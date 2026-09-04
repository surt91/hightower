//! Writes one SVG per trace event (`out/frames/frame_000.svg`, ...) so the
//! growth of both networks can be turned into an animation, e.g. with
//! `rsvg-convert` + `magick`/`ffmpeg`.

use std::fs;

use hightower::svg::{Layers, Scene, Style, render};
use hightower::{Bounds, ObstacleSet, Point, RouterConfig, Segment, TraceEvent, route_with};

fn p(x: i64, y: i64) -> Point {
    Point::new(x, y)
}

fn main() {
    let mut obstacles = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
    obstacles.add_rect(p(15, 55), p(35, 80));
    obstacles.add_rect(p(45, 20), p(65, 45));
    obstacles.add_rect(p(70, 60), p(90, 75));
    obstacles.add_rect(p(20, 15), p(35, 35));
    obstacles.add_segment(Segment::horizontal(50, 40, 85));
    let (a, b) = (p(8, 30), p(93, 88));

    let result = route_with(&obstacles, a, b, &RouterConfig::default());
    let scene = Scene {
        obstacles: &obstacles,
        a,
        b,
    };
    let style = Style::fit(obstacles.bounds(), 600.0);
    fs::create_dir_all("out/frames").expect("create out/frames");
    let layers = Layers {
        raw_path: true,
        ..Layers::default()
    };
    // Frame 0 shows the empty scene; frame i shows the first i events.
    for upto in 0..=result.trace.len() {
        let svg = render(&scene, &result.trace, upto, &style, layers);
        fs::write(format!("out/frames/frame_{upto:03}.svg"), svg).expect("write frame");
    }
    for (i, e) in result.trace.events.iter().enumerate() {
        let label = match e {
            TraceEvent::LineAdded { net, line, .. } => format!("{net:?}: line {line:?}"),
            TraceEvent::ProbeLine { net, line, .. } => format!("{net:?}: trial line {line:?}"),
            TraceEvent::EscapePoint {
                net,
                point,
                process,
            } => format!("{net:?}: escape point {point:?} ({process:?})"),
            TraceEvent::NoEscape { net } => format!("{net:?}: no escape"),
            TraceEvent::Intersection { point, .. } => format!("intersection at {point:?}"),
            TraceEvent::RawPath { corners } => format!("raw path, {} corners", corners.len()),
            TraceEvent::Improved { corners } => format!("improved path, {} corners", corners.len()),
        };
        println!("frame {:03}: {label}", i + 1);
    }
    println!("{} frames written to out/frames/", result.trace.len() + 1);
}
