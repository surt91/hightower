//! Hightower's Hampton Court maze, solved by the line router.
//!
//! The walls are traced from the plot on page 19 of the 1969 paper
//! (`scripts/trace_maze.py`, data in `examples/data/hampton_court.txt`), so
//! this is the maze Hightower's FORTRAN program solved, in a 224 x 212 unit
//! box (seven scan pixels per unit, corridors about three units wide). The plot's own path is drawn underneath ours for comparison, and the
//! run time is reported the way the plot did it: in hours.
//! Writes `out/maze.svg` (paper style) and `out/maze_trace.svg` (with both networks).

use std::fs;
use std::time::{Duration, Instant};

use hightower::svg::{Canvas, Layers, Style};
use hightower::{Bounds, ObstacleSet, Outcome, Point, RouterConfig, Segment, route_with};

const DATA: &str = include_str!("data/hampton_court.txt");

struct Maze {
    obstacles: ObstacleSet,
    paper_path: Vec<Segment>,
    a: Point,
    b: Point,
}

fn load() -> Maze {
    let mut grid = (0, 0);
    let mut walls = Vec::new();
    let mut paper_path = Vec::new();
    let (mut a, mut b) = (None, None);
    for line in DATA.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| f[i].parse::<i64>().expect("integer");
        match f.first().copied() {
            Some("#") if f.get(1) == Some(&"grid") => grid = (num(2), num(3)),
            Some("h") => walls.push(Segment::horizontal(num(1), num(2), num(3))),
            Some("v") => walls.push(Segment::vertical(num(1), num(2), num(3))),
            Some("ph") => paper_path.push(Segment::horizontal(num(1), num(2), num(3))),
            Some("pv") => paper_path.push(Segment::vertical(num(1), num(2), num(3))),
            Some("A") => a = Some(Point::new(num(1), num(2))),
            Some("B") => b = Some(Point::new(num(1), num(2))),
            _ => {}
        }
    }
    let mut obstacles = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(grid.0, grid.1)));
    for w in walls {
        obstacles.add_segment(w);
    }
    Maze {
        obstacles,
        paper_path,
        a: a.expect("A in data file"),
        b: b.expect("B in data file"),
    }
}

/// The plot on page 19 says "TOTAL TIME .0005" without a unit; hours is the
/// most plausible reading for 1969 batch accounting. We return the favour.
fn hours(d: Duration) -> String {
    let h = d.as_secs_f64() / 3600.0;
    // the plotter printed no leading zero and no exponent
    let s = format!("{h:.12}");
    s.trim_start_matches('0').to_string()
}

fn main() {
    let maze = load();
    let (a, b) = (maze.a, maze.b);
    println!("SOLUTION TO HAMPTON COURT MAZE");
    println!("{} walls, A = {a:?}, B = {b:?}", maze.obstacles.len());
    fs::create_dir_all("out").expect("create out/");

    let (result, median) = timed(&maze, &RouterConfig::default());
    report(&result, median);
    draw(
        &maze,
        &result,
        median,
        "out/maze.svg",
        "(gray: the path plotted in 1969, red: this implementation)",
    );
    fs::write("out/maze_trace.svg", String::new()).ok();
    draw_trace(&maze, &result, "out/maze_trace.svg");
}

/// Median run time over many runs plus one traced run.
fn timed(maze: &Maze, config: &RouterConfig) -> (hightower::RouteResult, Duration) {
    let runs = 500;
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(route_with(&maze.obstacles, maze.a, maze.b, config));
            t.elapsed()
        })
        .collect();
    times.sort();
    (
        route_with(&maze.obstacles, maze.a, maze.b, config),
        times[runs / 2],
    )
}

fn report(result: &hightower::RouteResult, median: Duration) {
    match result.outcome {
        Outcome::Found => println!("FOUND PATH FROM A TO B"),
        other => println!("NO PATH ({other:?})"),
    }
    println!(
        "TOTAL TIME {}  (that is {median:?}; {} steps, {} lines entered, {} trial lines{})",
        hours(median),
        result.steps,
        result.trace.line_count(),
        result.trace.probe_count(),
        result
            .path
            .as_ref()
            .map(|p| format!(", {} corners", p.len()))
            .unwrap_or_default()
    );
}

/// Draws the maze, Hightower's 1969 path in gray, our networks and path.
fn draw(maze: &Maze, result: &hightower::RouteResult, median: Duration, file: &str, note: &str) {
    let bounds = maze.obstacles.bounds();
    // 68 units on 900 px: one unit of clearance is 13 px.
    let style = Style {
        margin: 8.0,
        obstacle_width: 2.5,
        path_width: 3.5,
        dot_radius: 3.0,
        font_size: 15.0,
        labels: true,
        ..Style::fit(bounds, 900.0)
    };
    let mut c = Canvas::new(bounds, style.clone());
    c.frame();
    for s in &maze.paper_path {
        c.segment(s, "#d9d9d9", style.path_width * 2.5, "");
    }
    c.obstacles(&maze.obstacles);
    if let Some(path) = &result.path {
        c.polyline(path, style.path, style.path_width, "");
    } else {
        c.trace(&result.trace, result.trace.len(), Layers::default());
    }
    c.endpoints(maze.a, maze.b);
    let mono = |c: &mut Canvas, y: f64, text: &str| {
        c.raw(&format!(
            r##"<text x="50%" y="{y:.0}" text-anchor="middle" font-family="Courier New, Courier, monospace" font-size="{}" fill="#111">{}</text>"##,
            style.font_size, text
        ));
    };
    mono(&mut c, 24.0, "SOLUTION TO HAMPTON COURT MAZE");
    mono(
        &mut c,
        46.0,
        if result.path.is_some() {
            "FOUND PATH FROM A TO B"
        } else {
            "NO PATH FOUND FROM A TO B"
        },
    );
    mono(&mut c, 64.0, &format!("TOTAL TIME {}", hours(median)));
    mono(&mut c, 84.0, note);
    fs::write(file, c.finish()).expect("write svg");
    println!("wrote {file}");
}

/// Both networks with all trial lines, for the curious.
fn draw_trace(maze: &Maze, result: &hightower::RouteResult, file: &str) {
    let bounds = maze.obstacles.bounds();
    let style = Style {
        margin: 2.0,
        obstacle_width: 2.5,
        path_width: 3.0,
        dot_radius: 2.0,
        ..Style::fit(bounds, 900.0)
    };
    let mut c = Canvas::new(bounds, style.clone());
    c.frame();
    c.obstacles(&maze.obstacles);
    c.trace(&result.trace, result.trace.len(), Layers::default());
    c.endpoints(maze.a, maze.b);
    fs::write(file, c.finish()).expect("write svg");
    println!("wrote {file}");
}
