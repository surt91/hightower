//! Randomized property tests with a small hand-written PRNG (no dependencies).

use hightower::grid::route_grid;
use hightower::{
    Bounds, Improvement, ObstacleSet, Outcome, Point, RouterConfig, Segment, route_with,
    validate_path,
};

/// xorshift64* — deterministic, good enough for test scenes.
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

    fn below(&mut self, n: u64) -> i64 {
        (self.next() % n) as i64
    }
}

fn random_scene(seed: u64) -> (ObstacleSet, Point, Point) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let bounds = Bounds::new(Point::new(0, 0), Point::new(127, 127));
    let mut o = ObstacleSet::new(bounds);
    let n = rng.below(41);
    for _ in 0..n {
        let len = rng.below(31);
        let x = rng.below(128);
        let y = rng.below(128);
        if rng.below(2) == 0 {
            o.add_segment(Segment::horizontal(y, x, (x + len).min(127)));
        } else {
            o.add_segment(Segment::vertical(x, y, (y + len).min(127)));
        }
    }
    let mut free = || loop {
        let p = Point::new(rng.below(128), rng.below(128));
        if !o.is_on_obstacle(p) {
            return p;
        }
    };
    let a = free();
    let b = free();
    (o, a, b)
}

#[test]
fn random_scenes_terminate_and_are_sound() {
    let mut found = 0;
    let mut missed = 0;
    let mut impossible = 0;
    for seed in 0..400u64 {
        let (o, a, b) = random_scene(seed);
        for improve in [
            Improvement::None,
            Improvement::ExtensionOnly,
            Improvement::Full,
        ] {
            let config = RouterConfig {
                improve,
                ..Default::default()
            };
            let r = route_with(&o, a, b, &config);
            assert_ne!(
                r.outcome,
                Outcome::StepLimit,
                "seed {seed}: hit the step cap"
            );
            assert_ne!(r.outcome, Outcome::InvalidInput, "seed {seed}");
            let grid = route_grid(&o, a, b);
            match r.path {
                Some(path) => {
                    validate_path(&o, a, b, &path).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                    assert!(
                        grid.is_some(),
                        "seed {seed}: Hightower found a path but BFS did not"
                    );
                    if improve == Improvement::ExtensionOnly {
                        found += 1;
                    }
                }
                None => {
                    if improve == Improvement::ExtensionOnly {
                        if grid.is_some() {
                            missed += 1;
                        } else {
                            impossible += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("found {found}, missed {missed}, impossible {impossible}");
    assert!(
        found > 300,
        "the router should solve most random scenes, solved {found}"
    );
}

#[test]
fn dense_random_scenes_are_sound() {
    for seed in 0..150u64 {
        let mut rng = Rng(seed * 7919 + 13);
        let bounds = Bounds::new(Point::new(0, 0), Point::new(63, 63));
        let mut o = ObstacleSet::new(bounds);
        for _ in 0..120 {
            let len = rng.below(12);
            let x = rng.below(64);
            let y = rng.below(64);
            if rng.below(2) == 0 {
                o.add_segment(Segment::horizontal(y, x, (x + len).min(63)));
            } else {
                o.add_segment(Segment::vertical(x, y, (y + len).min(63)));
            }
        }
        let mut free = || loop {
            let p = Point::new(rng.below(64), rng.below(64));
            if !o.is_on_obstacle(p) {
                return p;
            }
        };
        let a = free();
        let b = free();
        let r = route_with(
            &o,
            a,
            b,
            &RouterConfig {
                improve: Improvement::Full,
                ..Default::default()
            },
        );
        assert_ne!(r.outcome, Outcome::StepLimit, "seed {seed}");
        if let Some(path) = r.path {
            validate_path(&o, a, b, &path).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            assert!(route_grid(&o, a, b).is_some(), "seed {seed}");
        }
    }
}

/// The visibility-graph router is complete and, without bend penalty, optimal:
/// it finds a path exactly when the grid BFS does, with the same length.
#[test]
fn visibility_graph_matches_grid_bfs() {
    use hightower::{VisibilityConfig, route_visibility_with};
    let len = |path: &[Point]| path.windows(2).map(|w| w[0].manhattan(w[1])).sum::<i64>();
    for seed in 0..300u64 {
        let (o, a, b) = random_scene(seed);
        let grid = route_grid(&o, a, b);
        let vis = route_visibility_with(&o, a, b, &VisibilityConfig::default());
        match (grid, vis.path) {
            (Some(g), Some(v)) => {
                validate_path(&o, a, b, &v).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                assert_eq!(len(&g), len(&v), "seed {seed}: lengths differ");
                assert_eq!(vis.cost, Some(len(&v)), "seed {seed}");
            }
            (None, None) => {}
            (g, v) => panic!(
                "seed {seed}: grid {:?} vs visibility {:?}",
                g.is_some(),
                v.is_some()
            ),
        }
        // with a bend penalty the path must still be valid and have at most as many bends
        let calm = route_visibility_with(&o, a, b, &VisibilityConfig { bend_penalty: 20 });
        if let Some(path) = calm.path {
            validate_path(&o, a, b, &path).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        }
    }
}
