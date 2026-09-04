//! Hightower's Hampton Court maze, solved by the line router.
//!
//! The walls are traced from the plot on page 19 of the 1969 paper
//! (`scripts/trace_maze.py`, data in `examples/data/hampton_court.txt`), so
//! this is the maze Hightower's FORTRAN program solved, in a 392 x 372 unit
//! box. The plot's own path is drawn underneath ours for comparison, and the
//! run time is reported the way the plot did it: in hours.
//! Writes `out/maze.svg` (paper style) and `out/maze_trace.svg` (with both networks).

use std::fs;
use std::time::{Duration, Instant};

use hightower::svg::{Canvas, Layers, Style};
use hightower::{
    Bounds, ObstacleSet, Outcome, Point, RouterConfig, Segment, route_visibility, route_with,
};

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
    let config = RouterConfig::default();

    // time it: median over many runs, the maze is small
    let runs = 2000;
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(route_with(&maze.obstacles, a, b, &config));
            t.elapsed()
        })
        .collect();
    times.sort();
    let median = times[runs / 2];
    let result = route_with(&maze.obstacles, a, b, &config);

    println!("SOLUTION TO HAMPTON COURT MAZE");
    println!("{} walls, A = {a:?}, B = {b:?}", maze.obstacles.len());
    match result.outcome {
        Outcome::Found => println!("FOUND PATH FROM A TO B"),
        other => println!("NO PATH ({other:?})"),
    }
    println!(
        "TOTAL TIME {}  (that is {median:?}; {} steps, {} lines)",
        hours(median),
        result.steps,
        result.trace.line_count()
    );

    // Hightower's router is not complete; if it gives up here, say so and
    // show the visibility-graph route instead.
    let (path, label) = match &result.path {
        Some(p) => (p.clone(), format!("TOTAL TIME {}", hours(median))),
        None => {
            let vis = route_visibility(&maze.obstacles, a, b).expect("the maze has a solution");
            println!("(falling back to the visibility graph for the drawing)");
            (
                vis,
                "NO PATH FOUND BY LINE SEARCH; VISIBILITY GRAPH SHOWN".to_string(),
            )
        }
    };

    fs::create_dir_all("out").expect("create out/");
    let bounds = maze.obstacles.bounds();
    let style = Style {
        margin: 46.0,
        obstacle_width: 2.0,
        path_width: 7.0,
        font_size: 15.0,
        labels: true,
        ..Style::fit(bounds, 720.0)
    };
    let paper = |with_trace: bool| {
        let mut c = Canvas::new(bounds, style.clone());
        c.frame();
        for s in &maze.paper_path {
            c.segment(s, "#cfcfcf", style.path_width * 1.6, "");
        }
        c.obstacles(&maze.obstacles);
        if with_trace {
            c.trace(
                &result.trace,
                result.trace.len(),
                Layers {
                    final_path: false,
                    ..Layers::default()
                },
            );
        }
        c.polyline(&path, style.path, style.path_width, "");
        c.endpoints(a, b);
        let mono = |c: &mut Canvas, y: f64, text: &str| {
            c.raw(&format!(
                r##"<text x="50%" y="{y:.0}" text-anchor="middle" font-family="Courier New, Courier, monospace" font-size="{}" fill="#111">{}</text>"##,
                style.font_size, text
            ));
        };
        mono(&mut c, 24.0, "SOLUTION TO HAMPTON COURT MAZE");
        mono(&mut c, 46.0, "FOUND PATH FROM A TO B");
        mono(&mut c, 64.0, &label);
        mono(
            &mut c,
            84.0,
            "(gray: the path plotted in 1969, red: this implementation)",
        );
        c.finish()
    };
    fs::write("out/maze.svg", paper(false)).expect("write svg");
    fs::write("out/maze_trace.svg", paper(true)).expect("write svg");
    println!(
        "path with {} corners; wrote out/maze.svg and out/maze_trace.svg",
        path.len()
    );
}
