//! Scenario tests from the implementation plan (§6).

use hightower::{
    Bounds, Improvement, ObstacleSet, Outcome, Point, RouterConfig, Segment, route, route_with,
    validate_path,
};

fn p(x: i64, y: i64) -> Point {
    Point::new(x, y)
}

fn arena() -> ObstacleSet {
    ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)))
}

fn bends(path: &[Point]) -> usize {
    path.len().saturating_sub(2)
}

fn assert_found(obstacles: &ObstacleSet, a: Point, b: Point) -> Vec<Point> {
    let path = route(obstacles, a, b).unwrap_or_else(|| panic!("no path from {a:?} to {b:?}"));
    validate_path(obstacles, a, b, &path).unwrap();
    path
}

#[test]
fn s1_no_obstacles() {
    let path = assert_found(&arena(), p(10, 10), p(80, 60));
    assert!(bends(&path) <= 2, "{path:?}");
}

#[test]
fn s2_same_row_clear() {
    let path = assert_found(&arena(), p(10, 40), p(80, 40));
    assert!(bends(&path) <= 2, "{path:?}");
    assert_eq!(path, vec![p(10, 40), p(80, 40)]);
}

#[test]
fn s3_single_wall_between() {
    let mut o = arena();
    o.add_segment(Segment::vertical(50, 0, 80));
    let path = assert_found(&o, p(10, 40), p(90, 40));
    assert!(bends(&path) >= 2, "{path:?}");
    // must pass over the top of the wall
    assert!(path.iter().any(|c| c.y > 80), "{path:?}");
}

#[test]
fn s4_full_height_wall_is_impassable() {
    let mut o = arena();
    o.add_segment(Segment::vertical(50, 0, 100));
    let r = route_with(&o, p(10, 40), p(90, 40), &RouterConfig::default());
    assert!(r.path.is_none());
    assert_eq!(r.outcome, Outcome::NoEscape);
}

#[test]
fn s5_paper_fig14_topology() {
    let mut o = arena();
    let a = p(10, 10);
    let b = p(38, 25);
    o.add_segment(Segment::vertical(15, 9, 21)); // wall next to A
    o.add_segment(Segment::horizontal(30, 31, 46)); // box top
    o.add_segment(Segment::vertical(31, 20, 30)); // box left
    o.add_segment(Segment::vertical(46, 20, 30)); // box right
    o.add_segment(Segment::horizontal(20, 36, 46)); // box bottom with opening x in [31,36]
    let path = assert_found(&o, a, b);
    // the path threads the opening: some corner strictly inside the box mouth column range
    assert!(
        path.windows(2).any(|w| {
            let s = Segment::between(w[0], w[1]).unwrap();
            s.crossing(&Segment::horizontal(20, 32, 35)).is_some()
        }),
        "{path:?}"
    );
}

#[test]
fn s6_closed_box_gives_no_escape() {
    let mut o = arena();
    o.add_rect(p(30, 30), p(50, 50));
    let r = route_with(&o, p(10, 10), p(40, 40), &RouterConfig::default());
    assert!(r.path.is_none());
    assert_eq!(r.outcome, Outcome::NoEscape, "steps={}", r.steps);
}

#[test]
fn s7_identical_points() {
    assert_eq!(route(&arena(), p(5, 5), p(5, 5)), Some(vec![p(5, 5)]));
}

#[test]
fn s8_point_on_obstacle_is_invalid() {
    let mut o = arena();
    o.add_segment(Segment::horizontal(10, 0, 20));
    let r = route_with(&o, p(10, 10), p(90, 90), &RouterConfig::default());
    assert_eq!(r.outcome, Outcome::InvalidInput);
    let r = route_with(&o, p(-1, 10), p(90, 90), &RouterConfig::default());
    assert_eq!(r.outcome, Outcome::InvalidInput);
}

/// Scenario 9: a box with a 3-unit mouth and a baffle behind it. The path
/// exists (the grid router finds it); whether Hightower finds it documents the
/// algorithm's incompleteness. We assert only that the answer is *sound* and
/// print which case occurred.
#[test]
fn s9_narrow_mouth_with_baffle_is_sound() {
    let mut o = arena();
    // box 30..70 x 30..70 with a mouth at the bottom, x in [48,52]
    o.add_segment(Segment::horizontal(70, 30, 70));
    o.add_segment(Segment::vertical(30, 30, 70));
    o.add_segment(Segment::vertical(70, 30, 70));
    o.add_segment(Segment::horizontal(30, 30, 47));
    o.add_segment(Segment::horizontal(30, 53, 70));
    // baffle right behind the mouth
    o.add_segment(Segment::horizontal(36, 40, 60));
    let (a, b) = (p(50, 10), p(50, 60));
    assert!(
        hightower::grid::route_grid(&o, a, b).is_some(),
        "the path exists"
    );
    let r = route_with(&o, a, b, &RouterConfig::default());
    match &r.path {
        Some(path) => validate_path(&o, a, b, path).unwrap(),
        None => assert_eq!(r.outcome, Outcome::NoEscape),
    }
}

#[test]
fn improvement_reduces_bends_and_keeps_validity() {
    let mut o = arena();
    o.add_rect(p(20, 40), p(35, 60));
    o.add_rect(p(50, 20), p(65, 45));
    o.add_segment(Segment::horizontal(75, 10, 80));
    let (a, b) = (p(10, 50), p(90, 30));
    let raw = route_with(
        &o,
        a,
        b,
        &RouterConfig {
            improve: Improvement::None,
            ..Default::default()
        },
    );
    let ext = route_with(&o, a, b, &RouterConfig::default());
    let full = route_with(
        &o,
        a,
        b,
        &RouterConfig {
            improve: Improvement::Full,
            ..Default::default()
        },
    );
    let raw = raw.path.unwrap();
    let ext = ext.path.unwrap();
    let full = full.path.unwrap();
    for path in [&raw, &ext, &full] {
        validate_path(&o, a, b, path).unwrap();
    }
    assert!(bends(&ext) <= bends(&raw), "raw {raw:?} ext {ext:?}");
    assert!(bends(&full) <= bends(&ext), "ext {ext:?} full {full:?}");
}

#[test]
fn step_limit_is_reported() {
    let mut o = arena();
    for y in (5..100).step_by(10) {
        o.add_segment(Segment::horizontal(y, 0, 90));
        o.add_segment(Segment::horizontal(y + 5, 10, 100));
    }
    let r = route_with(
        &o,
        p(5, 2),
        p(95, 97),
        &RouterConfig {
            max_steps: 3,
            ..Default::default()
        },
    );
    assert!(matches!(r.outcome, Outcome::StepLimit | Outcome::Found));
    if r.outcome == Outcome::StepLimit {
        assert_eq!(r.steps, 3);
    }
}

#[test]
fn already_routed_paths_can_become_obstacles() {
    let mut o = arena();
    o.add_rect(p(40, 40), p(60, 60));
    let first = assert_found(&o, p(10, 50), p(90, 50));
    o.add_path(&first);
    // the first path runs horizontally below or above the box and blocks the direct way
    let second = assert_found(&o, p(50, 10), p(50, 90));
    // the second path must not touch the first one
    for w in second.windows(2) {
        let s = Segment::between(w[0], w[1]).unwrap();
        for f in first.windows(2) {
            let t = Segment::between(f[0], f[1]).unwrap();
            assert!(!s.touches(&t), "{second:?} touches {first:?}");
        }
    }
}

/// A scene found by `examples/counterexample.rs`: B sits in a box with a
/// one-unit gap; the only route runs around the left of the box. With the
/// paper's Process II (no retreat from boundary ends) the router misses the
/// path; with `boundary_retreat` it finds it. This documents the
/// incompleteness, it is not a bug.
#[test]
fn s9b_known_miss_and_boundary_retreat() {
    let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(60, 60)));
    o.add_segment(Segment::horizontal(8, 3, 56));
    o.add_segment(Segment::horizontal(27, 43, 60));
    o.add_segment(Segment::horizontal(45, 3, 56));
    o.add_segment(Segment::vertical(3, 8, 45));
    o.add_segment(Segment::vertical(7, 49, 60));
    o.add_segment(Segment::vertical(56, 8, 15));
    o.add_segment(Segment::vertical(56, 17, 45));
    let (a, b) = (p(23, 55), p(42, 12));
    assert!(
        hightower::grid::route_grid(&o, a, b).is_some(),
        "the path exists"
    );
    let paper = route_with(&o, a, b, &RouterConfig::default());
    assert_eq!(paper.outcome, Outcome::NoEscape);
    let retreat = route_with(
        &o,
        a,
        b,
        &RouterConfig {
            boundary_retreat: true,
            ..Default::default()
        },
    );
    let path = retreat
        .path
        .expect("boundary retreat finds the way around the box");
    validate_path(&o, a, b, &path).unwrap();
}

fn length(path: &[Point]) -> i64 {
    path.windows(2).map(|w| w[0].manhattan(w[1])).sum()
}

/// Regression: a cover whose end sits exactly at `Z.x` used to make Process I
/// step *into* the cover (`away` chose `+1` for the `from` end) and only the
/// far end was tried. The escape point must be one unit beyond the cover's
/// end, so the router slips around the near end.
#[test]
fn process_i_escapes_around_the_near_end_in_the_tie_case() {
    let mut o = arena();
    o.add_segment(Segment::horizontal(12, 10, 20)); // starts exactly above A and B
    let (a, b) = (p(10, 10), p(10, 30));
    let path = assert_found(&o, a, b);
    assert_eq!(path, vec![a, p(9, 10), p(9, 30), b], "{path:?}");
    assert_eq!(length(&path), 22);
    // mirrored: the cover ends exactly at the endpoints' column
    let mut o = arena();
    o.add_segment(Segment::horizontal(12, 0, 10));
    let path = assert_found(&o, a, b);
    assert_eq!(path, vec![a, p(11, 10), p(11, 30), b], "{path:?}");
}

#[test]
fn endpoints_wedged_between_walls_at_distance_one() {
    let mut o = arena();
    // A in a slot open only at the top, B in a slot open only to the right
    let a = p(10, 10);
    o.add_segment(Segment::vertical(9, 0, 20));
    o.add_segment(Segment::vertical(11, 0, 20));
    o.add_segment(Segment::horizontal(9, 9, 11));
    let b = p(80, 60);
    o.add_segment(Segment::horizontal(61, 70, 81));
    o.add_segment(Segment::horizontal(59, 70, 81));
    o.add_segment(Segment::vertical(79, 59, 61));
    let path = assert_found(&o, a, b);
    assert!(path[1].x == 10 && path[1].y >= 21, "{path:?}");
    let last = path[path.len() - 2];
    assert!(last.y == 60 && last.x >= 82, "{path:?}");
    // the slot's zero-length horizontal escape line does not panic anywhere
    let r = route_with(
        &o,
        a,
        b,
        &RouterConfig {
            improve: Improvement::Full,
            boundary_retreat: true,
            ..Default::default()
        },
    );
    validate_path(&o, a, b, &r.path.unwrap()).unwrap();
}

#[test]
fn endpoints_on_the_bounds() {
    // corners of the box, a box in the middle
    let mut o = arena();
    o.add_rect(p(30, 30), p(70, 70));
    let path = assert_found(&o, p(0, 0), p(100, 100));
    assert!(bends(&path) <= 2, "{path:?}");
    // on opposite edges, wall from the bottom edge: must go over the top
    let mut o = arena();
    o.add_segment(Segment::vertical(50, 0, 80));
    let path = assert_found(&o, p(0, 50), p(100, 50));
    assert!(path.iter().any(|c| c.y > 80), "{path:?}");
    // on the top edge, wall reaching the top edge: over the bottom
    let path = assert_found(&o, p(20, 100), p(80, 100));
    assert!(path.iter().all(|c| c.y <= 100));
    // a wall spanning the whole height separates two points on the bottom edge
    let mut o = arena();
    o.add_segment(Segment::vertical(50, 0, 100));
    let r = route_with(&o, p(10, 0), p(90, 0), &RouterConfig::default());
    assert_eq!(r.outcome, Outcome::NoEscape);
    assert!(hightower::grid::route_grid(&o, p(10, 0), p(90, 0)).is_none());
    // exactly on the boundary corner next to a wall at distance one
    let mut o = arena();
    o.add_segment(Segment::vertical(1, 0, 99));
    let path = assert_found(&o, p(0, 0), p(50, 50));
    assert!(path.iter().any(|c| c.y == 100), "{path:?}");
}

#[test]
fn zero_length_obstacles_block_and_are_avoided() {
    // four point obstacles around A at distance one: no rectilinear way out
    let mut o = arena();
    let a = p(50, 50);
    for c in [p(49, 50), p(51, 50), p(50, 49), p(50, 51)] {
        o.add_segment(Segment::horizontal(c.y, c.x, c.x));
    }
    let r = route_with(&o, a, p(10, 10), &RouterConfig::default());
    assert_eq!(r.outcome, Outcome::NoEscape, "{:?}", r.path);
    assert!(hightower::grid::route_grid(&o, a, p(10, 10)).is_none());
    // three of them: the path leaves through the remaining side
    let mut o = arena();
    for c in [p(49, 50), p(51, 50), p(50, 49)] {
        o.add_segment(Segment::vertical(c.x, c.y, c.y));
    }
    let path = assert_found(&o, a, p(10, 10));
    assert!(path[1].x == 50 && path[1].y > 50, "{path:?}");
    // a lattice of point obstacles between A and B
    let mut o = arena();
    for x in (20..=80).step_by(3) {
        for y in (20..=80).step_by(3) {
            o.add_segment(Segment::horizontal(y, x, x));
        }
    }
    let (a, b) = (p(50, 10), p(50, 90));
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
        let path = r.path.expect("path through the lattice");
        validate_path(&o, a, b, &path).unwrap();
    }
}

fn fig14(shift: Point) -> (ObstacleSet, Point, Point) {
    let s = |x: i64, y: i64| p(x + shift.x, y + shift.y);
    let mut o = ObstacleSet::new(Bounds::new(s(0, 0), s(100, 100)));
    o.add_segment(Segment::vertical(15 + shift.x, 9 + shift.y, 21 + shift.y));
    o.add_segment(Segment::horizontal(
        30 + shift.y,
        31 + shift.x,
        46 + shift.x,
    ));
    o.add_segment(Segment::vertical(31 + shift.x, 20 + shift.y, 30 + shift.y));
    o.add_segment(Segment::vertical(46 + shift.x, 20 + shift.y, 30 + shift.y));
    o.add_segment(Segment::horizontal(
        20 + shift.y,
        36 + shift.x,
        46 + shift.x,
    ));
    (o, s(10, 10), s(38, 25))
}

#[test]
fn negative_coordinates_are_a_pure_translation() {
    let (o0, a0, b0) = fig14(p(0, 0));
    let shift = p(-1000, -777);
    let (o1, a1, b1) = fig14(shift);
    for improve in [
        Improvement::None,
        Improvement::ExtensionOnly,
        Improvement::Full,
    ] {
        let config = RouterConfig {
            improve,
            ..Default::default()
        };
        let r0 = route_with(&o0, a0, b0, &config);
        let r1 = route_with(&o1, a1, b1, &config);
        assert_eq!(r0.outcome, r1.outcome);
        assert_eq!(r0.steps, r1.steps);
        let path0 = r0.path.unwrap();
        let path1 = r1.path.unwrap();
        validate_path(&o1, a1, b1, &path1).unwrap();
        let shifted: Vec<Point> = path0
            .iter()
            .map(|c| p(c.x + shift.x, c.y + shift.y))
            .collect();
        assert_eq!(path1, shifted);
    }
    let g = hightower::grid::route_grid(&o1, a1, b1).unwrap();
    validate_path(&o1, a1, b1, &g).unwrap();
    let v = hightower::route_visibility(&o1, a1, b1).unwrap();
    validate_path(&o1, a1, b1, &v).unwrap();
    assert_eq!(length(&g), length(&v));
}

fn s_corridor() -> (ObstacleSet, Point, Point) {
    // B sits at the end of an S-shaped corridor inside a box, A outside.
    let mut o = arena();
    o.add_segment(Segment::horizontal(80, 20, 80));
    o.add_segment(Segment::vertical(20, 20, 80));
    o.add_segment(Segment::vertical(80, 20, 80));
    o.add_segment(Segment::horizontal(20, 20, 69)); // mouth at x in [70, 79]
    o.add_segment(Segment::horizontal(35, 30, 80)); // shelves alternate sides
    o.add_segment(Segment::horizontal(50, 20, 70));
    o.add_segment(Segment::horizontal(65, 30, 80));
    (o, p(90, 10), p(50, 72))
}

#[test]
fn all_retreat_options_thread_the_s_corridor() {
    let (o, a, b) = s_corridor();
    for boundary_retreat in [false, true] {
        for _ in [()] {
            for improve in [Improvement::None, Improvement::Full] {
                let config = RouterConfig {
                    improve,
                    boundary_retreat,
                    ..Default::default()
                };
                let r = route_with(&o, a, b, &config);
                assert_eq!(r.outcome, Outcome::Found, "{config:?}");
                let path = r.path.unwrap();
                validate_path(&o, a, b, &path).unwrap();
                // it really goes through the corridor: crosses all three shelf gaps
                for gap in [
                    Segment::horizontal(35, 21, 29),
                    Segment::horizontal(50, 71, 79),
                    Segment::horizontal(65, 21, 29),
                ] {
                    assert!(
                        path.windows(2).any(|w| {
                            Segment::between(w[0], w[1])
                                .unwrap()
                                .crossing(&gap)
                                .is_some()
                        }),
                        "{config:?}: {path:?} misses {gap:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn duplicate_obstacles_give_the_same_path() {
    let (mut o, a, b) = s_corridor();
    let once = route_with(&o, a, b, &RouterConfig::default());
    let segments: Vec<Segment> = o.segments().collect();
    for s in segments {
        o.add_segment(s);
        o.add_segment(s);
    }
    let thrice = route_with(&o, a, b, &RouterConfig::default());
    assert_eq!(once.path, thrice.path);
    assert_eq!(once.steps, thrice.steps);
    assert_eq!(once.trace, thrice.trace);
}

#[test]
fn pert_mode_corners_may_be_crossed_but_not_used() {
    let mut o = arena();
    let first = assert_found(&o, p(10, 50), p(90, 50));
    o.add_path_corners(&first); // only (10,50) and (90,50)
    // a second path crosses the first one freely ...
    let second = assert_found(&o, p(50, 10), p(50, 90));
    assert_eq!(second, vec![p(50, 10), p(50, 90)]);
    // ... but cannot end on or pass through one of its corners
    let r = route_with(&o, p(10, 50), p(50, 90), &RouterConfig::default());
    assert_eq!(r.outcome, Outcome::InvalidInput);
    let third = assert_found(&o, p(10, 49), p(10, 51));
    assert!(!third.iter().any(|c| *c == p(10, 50)), "{third:?}");
    assert!(third.len() >= 4, "{third:?}");
}

/// Hightower's own Hampton Court maze (page 19 of the paper), traced from
/// the scan. The algorithm as written must solve it, and with the default
/// improvement the path stays within 20 % of the shortest one and leaves A
/// towards the exit on the left, as the 1969 plot does.
#[test]
fn hampton_court_maze_is_solved() {
    const DATA: &str = include_str!("../examples/data/hampton_court.txt");
    let mut grid = (0, 0);
    let (mut a, mut b) = (p(0, 0), p(0, 0));
    let mut walls = Vec::new();
    for line in DATA.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| f[i].parse::<i64>().expect("integer");
        match f.first().copied() {
            Some("#") if f.get(1) == Some(&"grid") => grid = (n(2), n(3)),
            Some("h") => walls.push(Segment::horizontal(n(1), n(2), n(3))),
            Some("v") => walls.push(Segment::vertical(n(1), n(2), n(3))),
            Some("A") => a = p(n(1), n(2)),
            Some("B") => b = p(n(1), n(2)),
            _ => {}
        }
    }
    let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(grid.0, grid.1)));
    for w in walls {
        o.add_segment(w);
    }
    let len = |path: &[Point]| path.windows(2).map(|w| w[0].manhattan(w[1])).sum::<i64>();
    let shortest = hightower::route_visibility(&o, a, b).expect("maze is solvable");
    let r = route_with(&o, a, b, &RouterConfig::default());
    assert_eq!(
        r.outcome,
        Outcome::Found,
        "the paper's algorithm solves its own maze"
    );
    let path = r.path.unwrap();
    validate_path(&o, a, b, &path).unwrap();
    assert!(
        len(&path) * 10 <= len(&shortest) * 12,
        "maze path {} vs shortest {}",
        len(&path),
        len(&shortest)
    );
    assert!(
        path[1].x <= a.x || path[1].y != a.y,
        "first move {:?}",
        path[1]
    );
}

/// A serpentine corridor forces several Process II retreats; the default
/// improvement must still deliver a path close to the shortest one.
#[test]
fn serpentine_corridor_path_is_close_to_shortest() {
    let mut o = arena();
    // horizontal baffles from alternating sides leave a 10-unit gap at the end
    for (i, y) in (15..100).step_by(15).enumerate() {
        if i % 2 == 0 {
            o.add_segment(Segment::horizontal(y, 0, 88));
        } else {
            o.add_segment(Segment::horizontal(y, 12, 100));
        }
    }
    let (a, b) = (p(5, 5), p(95, 95));
    let len = |path: &[Point]| path.windows(2).map(|w| w[0].manhattan(w[1])).sum::<i64>();
    let shortest = hightower::route_visibility(&o, a, b).expect("solvable");
    let r = route_with(&o, a, b, &RouterConfig::default());
    let path = r.path.expect("line search threads the serpentine");
    validate_path(&o, a, b, &path).unwrap();
    assert!(
        len(&path) * 10 <= len(&shortest) * 11,
        "path {} vs shortest {}",
        len(&path),
        len(&shortest)
    );
    // the paper's second improvement (both parts) is the default
    assert_eq!(RouterConfig::default().improve, Improvement::Full);
    let ext = route_with(
        &o,
        a,
        b,
        &RouterConfig {
            improve: Improvement::ExtensionOnly,
            ..Default::default()
        },
    );
    assert!(len(ext.path.as_deref().unwrap()) >= len(&path));
}
