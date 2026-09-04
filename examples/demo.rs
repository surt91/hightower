//! Routes a handful of scenes and writes `out/demo_*.svg`.

use std::fs;

use hightower::svg::{Layers, Scene, Style, render, render_path};
use hightower::{Bounds, ObstacleSet, Point, RouterConfig, Segment, route_with};

fn p(x: i64, y: i64) -> Point {
    Point::new(x, y)
}

fn scenes() -> Vec<(&'static str, ObstacleSet, Point, Point)> {
    let bounds = Bounds::new(p(0, 0), p(100, 100));
    let mut list = Vec::new();

    let mut wall = ObstacleSet::new(bounds);
    wall.add_segment(Segment::vertical(50, 0, 80));
    list.push(("wall", wall, p(10, 40), p(90, 40)));

    let mut fig14 = ObstacleSet::new(bounds);
    fig14.add_segment(Segment::vertical(15, 9, 21));
    fig14.add_segment(Segment::horizontal(30, 31, 46));
    fig14.add_segment(Segment::vertical(31, 20, 30));
    fig14.add_segment(Segment::vertical(46, 20, 30));
    fig14.add_segment(Segment::horizontal(20, 36, 46));
    list.push(("fig14", fig14, p(10, 10), p(38, 25)));

    let mut boxes = ObstacleSet::new(bounds);
    boxes.add_rect(p(15, 55), p(35, 80));
    boxes.add_rect(p(45, 20), p(65, 45));
    boxes.add_rect(p(70, 60), p(90, 75));
    boxes.add_rect(p(20, 15), p(35, 35));
    boxes.add_segment(Segment::horizontal(50, 40, 85));
    list.push(("boxes", boxes, p(8, 30), p(93, 88)));

    let mut diagram = ObstacleSet::new(bounds);
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
        diagram.add_rect(p(x, y), p(x + 20, y + 15));
    }
    list.push(("diagram", diagram, p(20, 26), p(80, 74)));

    list
}

fn main() {
    fs::create_dir_all("out").expect("create out/");
    for (name, obstacles, a, b) in scenes() {
        let result = route_with(&obstacles, a, b, &RouterConfig::default());
        let scene = Scene {
            obstacles: &obstacles,
            a,
            b,
        };
        let style = Style::fit(obstacles.bounds(), 600.0);
        let svg = render(
            &scene,
            &result.trace,
            result.trace.len(),
            &style,
            Layers::default(),
        );
        fs::write(format!("out/demo_{name}.svg"), svg).expect("write svg");
        if let Some(path) = &result.path {
            fs::write(
                format!("out/demo_{name}_path.svg"),
                render_path(&scene, path, &style),
            )
            .expect("write svg");
        }
        println!(
            "{name:8} outcome={:?} steps={} lines={} corners={:?}",
            result.outcome,
            result.steps,
            result.trace.line_count(),
            result.path.as_ref().map(Vec::len)
        );
    }
}
