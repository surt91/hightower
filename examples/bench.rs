//! Micro-benchmark: Hightower's line router vs. the naive grid BFS on the
//! same scenes. Writes `out/bench.csv` (one row per measurement).
//!
//! Series A: board side length grows, the number of obstacles stays fixed
//! (obstacle positions scale with the board).
//! Series B: board side fixed at 256, the number of obstacles grows.

use std::fs;
use std::hint::black_box;
use std::time::{Duration, Instant};

use hightower::grid::route_grid;
use hightower::{Bounds, ObstacleSet, Point, route};

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

/// `count` random rectangles (diagram boxes) plus two free endpoints.
fn scene(seed: u64, side: i64, count: usize) -> (ObstacleSet, Point, Point) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(side, side)));
    let unit = (side / 32).max(1);
    for _ in 0..count {
        let w = unit * (1 + rng.below(4));
        let h = unit * (1 + rng.below(3));
        let x = rng.below(side - w);
        let y = rng.below(side - h);
        o.add_rect(Point::new(x, y), Point::new(x + w, y + h));
    }
    let mut free = || loop {
        let q = Point::new(rng.below(side + 1), rng.below(side + 1));
        if !o.is_on_obstacle(q) {
            return q;
        }
    };
    let a = free();
    let b = free();
    (o, a, b)
}

/// Median time of `f` over several scenes; only scenes both routers solve.
fn measure(side: i64, count: usize, scenes: usize) -> (Duration, Duration, usize) {
    let mut ht = Vec::new();
    let mut gr = Vec::new();
    let mut misses = 0;
    let mut seed = 0;
    while ht.len() < scenes {
        seed += 1;
        let (o, a, b) = scene(seed, side, count);
        let grid_path = route_grid(&o, a, b);
        if grid_path.is_none() {
            continue;
        }
        let path = route(&o, a, b);
        if path.is_none() {
            misses += 1;
            continue;
        }
        let reps = 20;
        let t = Instant::now();
        for _ in 0..reps {
            black_box(route(black_box(&o), a, b));
        }
        ht.push(t.elapsed() / reps);
        let reps = 3;
        let t = Instant::now();
        for _ in 0..reps {
            black_box(route_grid(black_box(&o), a, b));
        }
        gr.push(t.elapsed() / reps);
    }
    ht.sort();
    gr.sort();
    (ht[ht.len() / 2], gr[gr.len() / 2], misses)
}

fn main() {
    fs::create_dir_all("out").expect("create out/");
    let mut csv = String::from("series,side,obstacles,hightower_ns,grid_ns,misses\n");
    println!(
        "{:>8} {:>6} {:>10} {:>14} {:>14} {:>6}",
        "series", "side", "obstacles", "hightower", "grid", "misses"
    );
    for side in [64, 128, 256, 512, 1024, 2048] {
        let (h, g, misses) = measure(side, 20, 41);
        println!(
            "{:>8} {side:>6} {:>10} {:>14?} {:>14?} {misses:>6}",
            "area", 20, h, g
        );
        csv.push_str(&format!(
            "area,{side},20,{},{},{misses}\n",
            h.as_nanos(),
            g.as_nanos()
        ));
    }
    for count in [0, 5, 10, 20, 40, 80, 160, 320] {
        let (h, g, misses) = measure(256, count, 41);
        println!(
            "{:>8} {:>6} {count:>10} {:>14?} {:>14?} {misses:>6}",
            "clutter", 256, h, g
        );
        csv.push_str(&format!(
            "clutter,256,{count},{},{},{misses}\n",
            h.as_nanos(),
            g.as_nanos()
        ));
    }
    fs::write("out/bench.csv", csv).expect("write csv");
    println!("written out/bench.csv");
}
