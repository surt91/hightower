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

/// A richer scene: negative, odd-sized bounds; rectangles, loose segments and
/// zero-length point obstacles; endpoints sometimes on the boundary.
fn rich_scene(seed: u64) -> (ObstacleSet, Point, Point) {
    let mut rng = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
    let min = Point::new(-rng.below(200), -rng.below(200));
    let max = Point::new(min.x + 20 + rng.below(90), min.y + 20 + rng.below(90));
    let bounds = Bounds::new(min, max);
    let mut o = ObstacleSet::new(bounds);
    let w = bounds.width() as u64 + 1;
    let h = bounds.height() as u64 + 1;
    let inside = |rng: &mut Rng| Point::new(min.x + rng.below(w), min.y + rng.below(h));
    for _ in 0..rng.below(6) {
        let c = inside(&mut rng);
        let d = Point::new(
            (c.x + rng.below(25)).min(max.x),
            (c.y + rng.below(25)).min(max.y),
        );
        o.add_rect(c, d);
    }
    for _ in 0..rng.below(30) {
        let c = inside(&mut rng);
        let len = rng.below(30);
        if rng.below(2) == 0 {
            o.add_segment(Segment::horizontal(c.y, c.x, (c.x + len).min(max.x)));
        } else {
            o.add_segment(Segment::vertical(c.x, c.y, (c.y + len).min(max.y)));
        }
    }
    for _ in 0..rng.below(10) {
        let c = inside(&mut rng);
        o.add_segment(Segment::horizontal(c.y, c.x, c.x));
    }
    let free = |rng: &mut Rng| loop {
        let mut p = inside(rng);
        // every fourth endpoint sits on the boundary
        match rng.below(16) {
            0 => p.x = min.x,
            1 => p.x = max.x,
            2 => p.y = min.y,
            3 => p.y = max.y,
            _ => {}
        }
        if !o.is_on_obstacle(p) {
            return p;
        }
    };
    let a = free(&mut rng);
    let b = free(&mut rng);
    (o, a, b)
}

fn length(path: &[Point]) -> i64 {
    path.windows(2).map(|w| w[0].manhattan(w[1])).sum()
}

#[test]
fn rich_scenes_are_valid_and_sound_for_every_improvement() {
    let mut found = 0;
    for seed in 0..200u64 {
        let (o, a, b) = rich_scene(seed);
        let grid = route_grid(&o, a, b);
        for improve in [
            Improvement::None,
            Improvement::ExtensionOnly,
            Improvement::Full,
        ] {
            let r = route_with(
                &o,
                a,
                b,
                &RouterConfig {
                    improve,
                    ..Default::default()
                },
            );
            assert!(
                matches!(r.outcome, Outcome::Found | Outcome::NoEscape),
                "seed {seed}: {:?}",
                r.outcome
            );
            if let Some(path) = &r.path {
                validate_path(&o, a, b, path).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                assert!(grid.is_some(), "seed {seed}: unsound");
                // improvements never make the path longer than the raw one
                let raw = r.trace.events.iter().find_map(|e| match e {
                    hightower::TraceEvent::RawPath { corners } => Some(corners),
                    _ => None,
                });
                assert!(length(path) <= length(raw.unwrap()), "seed {seed}");
                if improve == Improvement::Full {
                    found += 1;
                }
            }
        }
    }
    assert!(found > 120, "solved only {found} of 200 rich scenes");
}

/// The trace must tell the same story as the result.
#[test]
fn trace_is_consistent_with_the_result() {
    use hightower::{NetId, TraceEvent};
    for seed in 0..250u64 {
        let (o, a, b) = if seed % 2 == 0 {
            random_scene(seed)
        } else {
            rich_scene(seed)
        };
        let config = RouterConfig {
            improve: Improvement::Full,
            boundary_retreat: seed % 4 == 3,
            ..Default::default()
        };
        let r = route_with(&o, a, b, &config);
        assert!(r.steps <= config.max_steps);
        let events = &r.trace.events;
        let mut lines: Vec<(NetId, Segment)> = Vec::new();
        let mut probes: Vec<(NetId, Segment)> = Vec::new();
        let mut last_point: [Point; 2] = [a, b];
        let mut intersections = 0;
        let mut no_escape = Vec::new();
        for e in events {
            match e {
                TraceEvent::ProbeLine { net, line, through } => {
                    assert!(line.contains(*through), "seed {seed}: {e:?}");
                    assert!(
                        o.is_free_segment(line),
                        "seed {seed}: trial line touches an obstacle"
                    );
                    probes.push((*net, *line));
                }
                TraceEvent::LineAdded { net, line, through } => {
                    assert!(line.contains(*through), "seed {seed}: {e:?}");
                    assert!(
                        o.is_free_segment(line),
                        "seed {seed}: line touches an obstacle"
                    );
                    // never the same line twice within one network
                    assert!(
                        !lines
                            .iter()
                            .any(|(n, l)| n == net && l.overlaps_collinear(line)),
                        "seed {seed}: used line constructed again {e:?}"
                    );
                    lines.push((*net, *line));
                }
                TraceEvent::EscapePoint { net, point, .. } => {
                    // an escape point lies on an earlier line or trial line of
                    // its own network, or on an escape line of the previous
                    // escape point (Process II retreat positions sit on the
                    // object point's other escape line, which need not be entered)
                    let k = usize::from(*net == NetId::B);
                    let prev = last_point[k];
                    assert!(
                        lines.iter().any(|(n, l)| n == net && l.contains(*point))
                            || probes.iter().any(|(n, l)| n == net && l.contains(*point))
                            || prev.x == point.x
                            || prev.y == point.y,
                        "seed {seed}: escape point off its network {e:?}"
                    );
                    assert!(o.is_free_point(*point));
                    last_point[k] = *point;
                }
                TraceEvent::Intersection {
                    point,
                    line_a,
                    line_b,
                } => {
                    intersections += 1;
                    assert!(line_a.contains(*point) && line_b.contains(*point));
                    assert!(lines.contains(&(NetId::A, *line_a)));
                    assert!(lines.contains(&(NetId::B, *line_b)));
                }
                TraceEvent::NoEscape { net } => no_escape.push(*net),
                TraceEvent::RawPath { .. } | TraceEvent::Improved { .. } => {}
            }
        }
        match r.outcome {
            Outcome::Found => {
                let path = r.path.as_ref().unwrap();
                assert_eq!(intersections, 1, "seed {seed}");
                assert_eq!(r.trace.final_path(), Some(path.as_slice()), "seed {seed}");
                let raw = events
                    .iter()
                    .find_map(|e| match e {
                        TraceEvent::RawPath { corners } => Some(corners),
                        _ => None,
                    })
                    .expect("raw path event");
                assert_eq!(raw.first(), Some(&a));
                assert_eq!(raw.last(), Some(&b));
                validate_path(&o, a, b, raw).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                // every raw segment runs along an escape line: an entered one,
                // a Process II trial line, or the (unentered) escape line of
                // the object point a retreat position was found on
                for w in raw.windows(2) {
                    let s = Segment::between(w[0], w[1]).unwrap();
                    let within = |l: &Segment| {
                        l.orientation == s.orientation
                            && l.fixed == s.fixed
                            && l.from <= s.from
                            && s.to <= l.to
                    };
                    assert!(
                        lines.iter().any(|(_, l)| within(l))
                            || probes.iter().any(|(_, l)| within(l))
                            || within(&o.escape_line(w[0], s.orientation)),
                        "seed {seed}: raw segment {s:?} is on no escape line"
                    );
                }
                // the crossing point lies on the raw path
                let x = events
                    .iter()
                    .find_map(|e| match e {
                        TraceEvent::Intersection { point, .. } => Some(*point),
                        _ => None,
                    })
                    .unwrap();
                assert!(
                    raw.windows(2)
                        .any(|w| Segment::between(w[0], w[1]).unwrap().contains(x)),
                    "seed {seed}: intersection {x:?} not on raw path {raw:?}"
                );
                let improved = events
                    .iter()
                    .filter(|e| matches!(e, TraceEvent::Improved { .. }))
                    .count();
                assert!(improved > 0 || raw == path, "seed {seed}");
                assert!(no_escape.len() <= 1, "seed {seed}");
            }
            Outcome::NoEscape => {
                assert!(r.path.is_none());
                assert_eq!(intersections, 0, "seed {seed}");
                assert!(r.trace.final_path().is_none());
                assert!(no_escape.contains(&NetId::A) && no_escape.contains(&NetId::B));
                assert_eq!(no_escape.len(), 2);
            }
            other => panic!("seed {seed}: {other:?}"),
        }
        assert_eq!(r.trace.line_count(), lines.len());
    }
}

#[test]
fn every_retreat_option_is_sound_and_terminates() {
    let mut solved = [0usize; 2];
    for seed in 0..150u64 {
        let (o, a, b) = if seed % 2 == 0 {
            random_scene(seed + 1000)
        } else {
            rich_scene(seed + 1000)
        };
        let grid = route_grid(&o, a, b);
        for (k, boundary_retreat) in [false, true].into_iter().enumerate() {
            let config = RouterConfig {
                improve: Improvement::Full,
                boundary_retreat,
                ..Default::default()
            };
            let r = route_with(&o, a, b, &config);
            assert!(
                matches!(r.outcome, Outcome::Found | Outcome::NoEscape),
                "seed {seed} {config:?}: {:?}",
                r.outcome
            );
            if let Some(path) = r.path {
                validate_path(&o, a, b, &path).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                assert!(grid.is_some(), "seed {seed} {config:?}: unsound");
                solved[k] += 1;
            }
        }
    }
    eprintln!("solved paper {} boundary_retreat {}", solved[0], solved[1]);
    assert!(solved.iter().all(|&n| n > 100), "{solved:?}");
}

/// The algorithm only ever compares coordinates, so shifting the whole scene
/// (also into negative territory) shifts the answer and nothing else.
#[test]
fn routing_is_translation_invariant() {
    let shift = |o: &ObstacleSet, d: Point| {
        let b = o.bounds();
        let mut s = ObstacleSet::new(Bounds::new(
            Point::new(b.min.x + d.x, b.min.y + d.y),
            Point::new(b.max.x + d.x, b.max.y + d.y),
        ));
        for seg in o.segments() {
            let f = if seg.orientation == hightower::Orientation::Horizontal {
                (d.y, d.x)
            } else {
                (d.x, d.y)
            };
            s.add_segment(Segment::new(
                seg.orientation,
                seg.fixed + f.0,
                seg.from + f.1,
                seg.to + f.1,
            ));
        }
        s
    };
    for seed in 0..120u64 {
        let (o, a, b) = random_scene(seed);
        let mut rng = Rng(seed + 77);
        let d = Point::new(rng.below(4001) - 2000, rng.below(4001) - 2000);
        let o2 = shift(&o, d);
        let (a2, b2) = (
            Point::new(a.x + d.x, a.y + d.y),
            Point::new(b.x + d.x, b.y + d.y),
        );
        for improve in [Improvement::ExtensionOnly, Improvement::Full] {
            let config = RouterConfig {
                improve,
                ..Default::default()
            };
            let r1 = route_with(&o, a, b, &config);
            let r2 = route_with(&o2, a2, b2, &config);
            assert_eq!(r1.outcome, r2.outcome, "seed {seed}");
            assert_eq!(r1.steps, r2.steps, "seed {seed}");
            let moved: Option<Vec<Point>> = r1
                .path
                .map(|p| p.iter().map(|c| Point::new(c.x + d.x, c.y + d.y)).collect());
            assert_eq!(moved, r2.path, "seed {seed}");
        }
        let g1 = route_grid(&o, a, b);
        let g2 = route_grid(&o2, a2, b2);
        assert_eq!(
            g1.map(|p| length(&p)),
            g2.map(|p| length(&p)),
            "seed {seed}"
        );
    }
}

/// Visibility graph vs. BFS on scenes with rectangles, point obstacles,
/// negative bounds and endpoints on the boundary.
#[test]
fn visibility_graph_matches_grid_bfs_on_rich_scenes() {
    use hightower::{VisibilityConfig, route_visibility_with};
    let bends = |path: &[Point]| path.len().saturating_sub(2);
    for seed in 0..200u64 {
        let (o, a, b) = rich_scene(seed);
        let grid = route_grid(&o, a, b);
        let vis = route_visibility_with(&o, a, b, &VisibilityConfig::default());
        let penalty = 7;
        let calm = route_visibility_with(
            &o,
            a,
            b,
            &VisibilityConfig {
                bend_penalty: penalty,
            },
        );
        match (grid, vis.path, calm.path) {
            (Some(g), Some(v), Some(c)) => {
                validate_path(&o, a, b, &v).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                validate_path(&o, a, b, &c).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
                assert_eq!(length(&g), length(&v), "seed {seed}: not shortest");
                assert_eq!(vis.cost, Some(length(&v)), "seed {seed}");
                // the penalised path minimises length + penalty * bends over
                // the same graph: at least as long, at most as many bends
                assert_eq!(
                    calm.cost,
                    Some(length(&c) + penalty * bends(&c) as i64),
                    "seed {seed}"
                );
                assert!(length(&c) >= length(&v), "seed {seed}");
                assert!(bends(&c) <= bends(&g), "seed {seed}: {c:?} vs {g:?}");
                assert!(
                    length(&c) + penalty * bends(&c) as i64
                        <= length(&g) + penalty * bends(&g) as i64,
                    "seed {seed}"
                );
            }
            (None, None, None) => {}
            (g, v, c) => panic!(
                "seed {seed}: grid {:?} visibility {:?} calm {:?}",
                g.is_some(),
                v.is_some(),
                c.is_some()
            ),
        }
    }
}
