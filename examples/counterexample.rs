//! Searches random "rooms with doors" scenes for cases where a path exists
//! (grid BFS finds it) but the line router does not: the algorithm's
//! documented incompleteness. Prints the smallest one found and writes
//! `out/counterexample.svg`.

use std::fs;

use hightower::grid::{flood, route_grid};
use hightower::svg::{Canvas, Layers, Scene, Style, render};
use hightower::{Bounds, ObstacleSet, Outcome, Point, RouterConfig, Segment, route_with};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: i64) -> i64 {
        (self.next() % n as u64) as i64
    }
}

fn p(x: i64, y: i64) -> Point {
    Point::new(x, y)
}

/// Adds a rectangle whose four walls each get zero or one door of the given width.
fn room(o: &mut ObstacleSet, rng: &mut Rng, min: Point, max: Point, door: i64) {
    let walls = [
        (Segment::horizontal(min.y, min.x, max.x), true),
        (Segment::horizontal(max.y, min.x, max.x), true),
        (Segment::vertical(min.x, min.y, max.y), false),
        (Segment::vertical(max.x, min.y, max.y), false),
    ];
    for (wall, _) in walls {
        if wall.len() > door + 4 && rng.below(3) == 0 {
            let start = wall.from + 2 + rng.below(wall.len() - door - 3);
            o.add_segment(Segment::new(wall.orientation, wall.fixed, wall.from, start));
            o.add_segment(Segment::new(
                wall.orientation,
                wall.fixed,
                start + door,
                wall.to,
            ));
        } else {
            o.add_segment(wall);
        }
    }
}

fn scene(seed: u64) -> (ObstacleSet, Point, Point) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let size = 60;
    let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(size, size)));
    let rooms = 1 + rng.below(4);
    for _ in 0..rooms {
        let x = rng.below(size - 12);
        let y = rng.below(size - 12);
        let w = 8 + rng.below(size - x - 8);
        let h = 8 + rng.below(size - y - 8);
        let door = 2 + rng.below(2);
        room(&mut o, &mut rng, p(x, y), p(x + w, y + h), door);
    }
    for _ in 0..rng.below(6) {
        let len = 3 + rng.below(20);
        let x = rng.below(size);
        let y = rng.below(size);
        if rng.below(2) == 0 {
            o.add_segment(Segment::horizontal(y, x, (x + len).min(size)));
        } else {
            o.add_segment(Segment::vertical(x, y, (y + len).min(size)));
        }
    }
    let mut free = || loop {
        let q = p(rng.below(size + 1), rng.below(size + 1));
        if !o.is_on_obstacle(q) {
            return q;
        }
    };
    let a = free();
    let b = free();
    (o, a, b)
}

fn main() {
    let tries = 20_000u64;
    let mut best: Option<(u64, usize)> = None;
    let mut misses = 0;
    let mut misses_retreat = 0;
    let mut solvable = 0;
    let retreat = RouterConfig {
        boundary_retreat: true,
        ..RouterConfig::default()
    };
    for seed in 0..tries {
        let (o, a, b) = scene(seed);
        let grid = route_grid(&o, a, b);
        if grid.is_none() {
            continue;
        }
        solvable += 1;
        let r = route_with(&o, a, b, &RouterConfig::default());
        if r.path.is_none() {
            misses += 1;
            assert_eq!(r.outcome, Outcome::NoEscape);
            if best.is_none_or(|(_, n)| o.len() < n) {
                best = Some((seed, o.len()));
            }
        }
        if route_with(&o, a, b, &retreat).path.is_none() {
            misses_retreat += 1;
        }
    }
    println!(
        "{solvable} solvable scenes, {misses} missed by the line router ({misses_retreat} with boundary_retreat)"
    );
    let Some((seed, _)) = best else {
        println!("no counterexample found");
        return;
    };
    let (o, a, b) = scene(seed);
    let r = route_with(&o, a, b, &RouterConfig::default());
    let grid = flood(&o, a, b);
    println!(
        "smallest: seed {seed}, {} segments, {} steps, {} lines",
        o.len(),
        r.steps,
        r.trace.line_count()
    );
    println!("A = {a:?}, B = {b:?}");
    for s in o.segments() {
        println!("  {s:?}");
    }
    fs::create_dir_all("out").expect("create out/");
    let sc = Scene {
        obstacles: &o,
        a,
        b,
    };
    let style = Style::fit(o.bounds(), 600.0);
    // Hightower's stuck networks plus the grid path ghosted in gray.
    let mut c = Canvas::new(o.bounds(), style.clone());
    c.frame();
    c.obstacles(&o);
    if let Some(gp) = &grid.path {
        c.polyline(gp, "#999999", style.path_width, r#"stroke-dasharray="8 6""#);
    }
    c.trace(&r.trace, r.trace.len(), Layers::default());
    c.endpoints(a, b);
    fs::write("out/counterexample.svg", c.finish()).expect("write svg");
    fs::write(
        "out/counterexample_plain.svg",
        render(&sc, &r.trace, r.trace.len(), &style, Layers::default()),
    )
    .expect("write svg");
}
