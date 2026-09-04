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
