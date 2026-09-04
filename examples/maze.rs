//! A Hampton-Court-style hedge maze, solved by the line router. Homage to the
//! paper's page-19 plot ("Solution to Hampton Court Maze, total time .0005").
//! Writes `out/maze.svg` and `out/maze_trace.svg`.

use std::fs;
use std::time::Instant;

use hightower::svg::{Layers, Scene, Style, render, render_path};
use hightower::{Bounds, ObstacleSet, Point, RouterConfig, Segment, route_with};

fn p(x: i64, y: i64) -> Point {
    Point::new(x, y)
}

/// Walls of a small maze in the spirit of Hampton Court: concentric hedges
/// with offset gaps and a few dead-end spurs. Coordinates in a 120 x 90 box.
pub fn hampton_court() -> ObstacleSet {
    let mut m = ObstacleSet::new(Bounds::new(p(0, 0), p(120, 90)));
    let mut h = |y, x1, x2| m.add_segment(Segment::horizontal(y, x1, x2));
    // outer hedge with the entrance at the bottom (x 56..64)
    h(2, 2, 56);
    h(2, 64, 118);
    h(88, 2, 118);
    let mut v = |x, y1, y2| m.add_segment(Segment::vertical(x, y1, y2));
    v(2, 2, 88);
    v(118, 2, 88);
    // second ring, gap at the right side (y 40..48)
    let mut h = |y, x1, x2| m.add_segment(Segment::horizontal(y, x1, x2));
    h(12, 12, 108);
    h(78, 12, 108);
    let mut v = |x, y1, y2| m.add_segment(Segment::vertical(x, y1, y2));
    v(12, 12, 78);
    v(108, 12, 40);
    v(108, 48, 78);
    // third ring, gap at the top left (x 22..30)
    let mut h = |y, x1, x2| m.add_segment(Segment::horizontal(y, x1, x2));
    h(22, 22, 98);
    h(68, 30, 98);
    let mut v = |x, y1, y2| m.add_segment(Segment::vertical(x, y1, y2));
    v(22, 22, 68);
    v(98, 22, 68);
    // fourth ring, gap at the bottom right (x 76..84)
    let mut h = |y, x1, x2| m.add_segment(Segment::horizontal(y, x1, x2));
    h(32, 32, 76);
    h(32, 84, 88);
    h(58, 32, 88);
    let mut v = |x, y1, y2| m.add_segment(Segment::vertical(x, y1, y2));
    v(32, 32, 58);
    v(88, 32, 58);
    // spurs and baffles that turn rings into corridors with dead ends
    let mut h = |y, x1, x2| m.add_segment(Segment::horizontal(y, x1, x2));
    h(7, 40, 80); // baffle behind the entrance
    h(45, 32, 60); // divider inside the centre
    h(17, 60, 108); // dead end in ring 2
    h(73, 12, 50); // dead end in ring 2 top
    h(27, 22, 50); // dead end in ring 3
    let mut v = |x, y1, y2| m.add_segment(Segment::vertical(x, y1, y2));
    v(7, 40, 88); // dead end ring 1 left
    v(113, 2, 60); // dead end ring 1 right
    v(60, 12, 17); // stub
    v(50, 22, 27); // stub
    v(70, 32, 45); // stub inside the centre
    v(17, 40, 78); // dead end ring 2 left
    v(93, 22, 50); // dead end ring 3 right
    m
}

fn main() {
    let maze = hampton_court();
    let a = p(60, 0); // outside the entrance
    let b = p(50, 40); // goal in the centre
    let start = Instant::now();
    let result = route_with(&maze, a, b, &RouterConfig::default());
    let elapsed = start.elapsed();
    println!(
        "outcome={:?} steps={} lines={} time={elapsed:?}",
        result.outcome,
        result.steps,
        result.trace.line_count()
    );
    fs::create_dir_all("out").expect("create out/");
    let scene = Scene {
        obstacles: &maze,
        a,
        b,
    };
    let style = Style {
        obstacle_width: 4.0,
        path_width: 4.0,
        ..Style::fit(maze.bounds(), 720.0)
    };
    fs::write(
        "out/maze_trace.svg",
        render(
            &scene,
            &result.trace,
            result.trace.len(),
            &style,
            Layers::default(),
        ),
    )
    .expect("write svg");
    match &result.path {
        Some(path) => {
            println!("path with {} corners: {path:?}", path.len());
            fs::write("out/maze.svg", render_path(&scene, path, &style)).expect("write svg");
        }
        None => println!("no path found"),
    }
}
