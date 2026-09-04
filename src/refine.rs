//! Path reconstruction from the two escape-point trees, collinear cleanup,
//! the paper's "second improvement" and a validity checker.

use crate::geometry::{Coord, Orientation, Point, Segment};
use crate::obstacles::ObstacleSet;
use crate::router::Network;

/// Concatenates the corner chains of both networks: `A ... X ... B`.
pub(crate) fn reconstruct(
    net_a: &Network,
    line_a: usize,
    net_b: &Network,
    line_b: usize,
    x: Point,
) -> Vec<Point> {
    let mut path = net_a.chain(line_a, x);
    path.reverse();
    path.extend(net_b.chain(line_b, x).into_iter().skip(1));
    path
}

/// Removes consecutive duplicates and every point whose neighbours are
/// collinear with it (this also removes overshoots), until nothing changes.
/// Afterwards consecutive segments alternate orientation.
pub fn cleanup(path: &mut Vec<Point>) {
    loop {
        let before = path.len();
        path.dedup();
        let mut i = 1;
        while i + 1 < path.len() {
            let (p, q, r) = (path[i - 1], path[i], path[i + 1]);
            if (p.x == q.x && q.x == r.x) || (p.y == q.y && q.y == r.y) {
                path.remove(i);
            } else {
                i += 1;
            }
        }
        if path.len() == before {
            return;
        }
    }
}

fn segment_at(path: &[Point], i: usize) -> Segment {
    Segment::between(path[i], path[i + 1]).expect("path is rectilinear")
}

/// Improvement A (paper Fig. 10 → 11): extend every path segment as far as the
/// obstacles allow; if the extension crosses a later perpendicular segment,
/// cut out everything in between. Returns whether the path changed.
pub fn improve_extension(obstacles: &ObstacleSet, path: &mut Vec<Point>) -> bool {
    let mut changed = false;
    'restart: loop {
        if path.len() < 4 {
            return changed;
        }
        for i in 0..path.len() - 1 {
            let seg = segment_at(path, i);
            let ext = obstacles.escape_line(path[i], seg.orientation);
            let mut j = i + 3;
            while j + 1 < path.len() {
                let later = segment_at(path, j);
                if let Some(x) = ext.crossing(&later) {
                    path.splice(i + 1..=j, [x]);
                    cleanup(path);
                    changed = true;
                    continue 'restart;
                }
                j += 2;
            }
        }
        return changed;
    }
}

/// Improvement B (paper Fig. 8 → 9): slide a probe point along every segment
/// (from its far end back to its start) and shoot a perpendicular escape line
/// from it; if that line crosses a later parallel segment, splice the shortcut
/// in. Removes staircases and the wall-to-wall zigzags that Process II leaves
/// behind in corridors. Returns whether the path changed.
///
/// A probe can only hit a later *parallel* segment whose span overlaps the
/// probe position, so only those positions are visited (far end first, as in
/// the paper). After a splice the scan resumes at the spliced segment; a pass
/// that changed anything is followed by another full pass, so the result is
/// the same fixed point the exhaustive scan reaches.
pub fn improve_probe(obstacles: &ObstacleSet, path: &mut Vec<Point>) -> bool {
    let mut changed = false;
    loop {
        let changed_this_pass = probe_pass(obstacles, path);
        changed |= changed_this_pass;
        if !changed_this_pass {
            return changed;
        }
    }
}

/// One pass of [`improve_probe`] over the whole path.
fn probe_pass(obstacles: &ObstacleSet, path: &mut Vec<Point>) -> bool {
    let mut changed = false;
    let mut i = 0;
    'segments: while path.len() >= 4 && i + 1 < path.len() {
        let seg = segment_at(path, i);
        let o = seg.orientation;
        let end = path[i + 1].along(o);

        // positions on this segment (excluding the far end) that some later
        // parallel segment could be hit from, ordered from the far end inward
        let mut candidates: Vec<Coord> = Vec::new();
        let mut j = i + 2;
        while j + 1 < path.len() {
            let later = segment_at(path, j);
            let lo = seg.from.max(later.from);
            let hi = seg.to.min(later.to);
            for q_along in lo..=hi {
                if q_along != end {
                    candidates.push(q_along);
                }
            }
            j += 2;
        }
        candidates.sort_unstable_by_key(|&q| (q - end).abs());
        candidates.dedup();

        for q_along in candidates {
            let q = Point::from_along_across(o, q_along, seg.fixed);
            let probe = obstacles.escape_line(q, o.perpendicular());
            let mut j = i + 2;
            while j + 1 < path.len() {
                let later = segment_at(path, j);
                if let Some(x) = probe.crossing(&later) {
                    path.splice(i + 1..=j, [q, x]);
                    cleanup(path);
                    changed = true;
                    i = i.saturating_sub(1);
                    continue 'segments;
                }
                j += 2;
            }
        }
        i += 1;
    }
    changed
}

/// Checks that `path` is a valid rectilinear route from `a` to `b`:
/// endpoints match, consecutive corners differ in exactly one coordinate, all
/// corners lie inside the bounds and no segment touches an obstacle.
pub fn validate_path(
    obstacles: &ObstacleSet,
    a: Point,
    b: Point,
    path: &[Point],
) -> Result<(), String> {
    if path.first() != Some(&a) {
        return Err(format!("path does not start at {a:?}: {:?}", path.first()));
    }
    if path.last() != Some(&b) {
        return Err(format!("path does not end at {b:?}: {:?}", path.last()));
    }
    for (i, &p) in path.iter().enumerate() {
        if !obstacles.bounds().contains(p) {
            return Err(format!("corner {i} {p:?} is outside the bounds"));
        }
    }
    if path.len() == 1 {
        return if obstacles.is_free_point(a) {
            Ok(())
        } else {
            Err(format!("{a:?} is on an obstacle"))
        };
    }
    let mut prev: Option<Orientation> = None;
    for (i, pair) in path.windows(2).enumerate() {
        let (p, q) = (pair[0], pair[1]);
        if p == q {
            return Err(format!("zero-length segment at corner {i}: {p:?}"));
        }
        let seg =
            Segment::between(p, q).ok_or_else(|| format!("diagonal segment {p:?} -> {q:?}"))?;
        if prev == Some(seg.orientation) {
            return Err(format!("collinear consecutive segments at corner {i}"));
        }
        prev = Some(seg.orientation);
        if !obstacles.is_free_segment(&seg) {
            return Err(format!(
                "segment {p:?} -> {q:?} touches an obstacle or leaves the bounds"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Bounds;

    /// The paper's exhaustive probing (every unit position, restart from the
    /// first segment after each splice), kept as the reference the optimised
    /// [`improve_probe`] must agree with.
    fn improve_probe_exhaustive(obstacles: &ObstacleSet, path: &mut Vec<Point>) -> bool {
        let mut changed = false;
        'restart: loop {
            if path.len() < 4 {
                return changed;
            }
            for i in 0..path.len() - 1 {
                let seg = segment_at(path, i);
                let o = seg.orientation;
                let start = path[i].along(o);
                let end = path[i + 1].along(o);
                let step = (start - end).signum();
                let mut q_along = end + step;
                while q_along != start + step {
                    let q = Point::from_along_across(o, q_along, seg.fixed);
                    let probe = obstacles.escape_line(q, o.perpendicular());
                    let mut j = i + 2;
                    while j + 1 < path.len() {
                        let later = segment_at(path, j);
                        if let Some(x) = probe.crossing(&later) {
                            path.splice(i + 1..=j, [q, x]);
                            cleanup(path);
                            changed = true;
                            continue 'restart;
                        }
                        j += 2;
                    }
                    q_along += step;
                }
            }
            return changed;
        }
    }

    #[test]
    fn optimised_probe_matches_the_exhaustive_scan() {
        use crate::router::{Improvement, RouterConfig, route_with};
        // deterministic xorshift scenes with rectangles and loose segments
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let mut compared = 0;
        for _ in 0..150 {
            let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(80, 80)));
            for _ in 0..(next() % 6 + 2) {
                let x = (next() % 70) as i64;
                let y = (next() % 70) as i64;
                let w = (next() % 10 + 1) as i64;
                let h = (next() % 10 + 1) as i64;
                o.add_rect(p(x, y), p(x + w, y + h));
            }
            for _ in 0..(next() % 5) {
                let x = (next() % 80) as i64;
                let y = (next() % 80) as i64;
                let l = (next() % 25) as i64;
                if next() % 2 == 0 {
                    o.add_segment(Segment::horizontal(y, x, (x + l).min(80)));
                } else {
                    o.add_segment(Segment::vertical(x, y, (y + l).min(80)));
                }
            }
            let mut free = |o: &ObstacleSet| loop {
                let q = p((next() % 81) as i64, (next() % 81) as i64);
                if o.is_free_point(q) {
                    return q;
                }
            };
            let (a, b) = (free(&o), free(&o));
            let raw = route_with(
                &o,
                a,
                b,
                &RouterConfig {
                    improve: Improvement::None,
                    ..Default::default()
                },
            );
            let Some(raw) = raw.path else { continue };
            let mut fast = raw.clone();
            let mut slow = raw.clone();
            let changed_fast = improve_probe(&o, &mut fast);
            let changed_slow = improve_probe_exhaustive(&o, &mut slow);
            assert_eq!(fast, slow, "scene {a:?}->{b:?}");
            assert_eq!(changed_fast, changed_slow);
            compared += 1;
        }
        assert!(compared > 100);
    }

    fn p(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn cleanup_removes_collinear_and_overshoot() {
        let mut path = vec![
            p(0, 0),
            p(5, 0),
            p(10, 0),
            p(10, 5),
            p(10, 5),
            p(10, 3),
            p(20, 3),
        ];
        cleanup(&mut path);
        assert_eq!(path, vec![p(0, 0), p(10, 0), p(10, 3), p(20, 3)]);
    }

    #[test]
    fn cleanup_handles_backtracking_overshoot() {
        let mut path = vec![p(0, 0), p(10, 0), p(4, 0), p(4, 7)];
        cleanup(&mut path);
        assert_eq!(path, vec![p(0, 0), p(4, 0), p(4, 7)]);
    }

    #[test]
    fn extension_shortcut_removes_detour() {
        let obstacles = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        // A detour: up, right, down, right, up, right
        let mut path = vec![
            p(0, 0),
            p(0, 20),
            p(30, 20),
            p(30, 5),
            p(60, 5),
            p(60, 20),
            p(90, 20),
        ];
        assert!(improve_extension(&obstacles, &mut path));
        assert_eq!(path, vec![p(0, 0), p(0, 20), p(90, 20)]);
        assert!(validate_path(&obstacles, p(0, 0), p(90, 20), &path).is_ok());
    }

    #[test]
    fn probe_removes_u_turn_bulge() {
        let obstacles = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        // Goes right to x=10, up, back left to x=4, then up again.
        let mut path = vec![p(0, 0), p(10, 0), p(10, 10), p(4, 10), p(4, 20)];
        assert!(!improve_extension(&obstacles, &mut path));
        assert!(improve_probe(&obstacles, &mut path));
        assert_eq!(path, vec![p(0, 0), p(4, 0), p(4, 20)]);
        assert!(validate_path(&obstacles, p(0, 0), p(4, 20), &path).is_ok());
    }

    #[test]
    fn probe_leaves_monotone_staircase_alone() {
        let obstacles = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        let mut path = vec![
            p(0, 0),
            p(10, 0),
            p(10, 10),
            p(20, 10),
            p(20, 20),
            p(30, 20),
        ];
        assert!(!improve_probe(&obstacles, &mut path));
        assert_eq!(path.len(), 6);
    }

    #[test]
    fn validator_rejects_bad_paths() {
        let mut obstacles = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        obstacles.add_segment(Segment::vertical(50, 0, 80));
        assert!(validate_path(&obstacles, p(10, 10), p(90, 10), &[p(10, 10), p(90, 10)]).is_err());
        assert!(
            validate_path(
                &obstacles,
                p(10, 10),
                p(90, 10),
                &[p(10, 10), p(50, 90), p(90, 10)]
            )
            .is_err()
        );
        assert!(
            validate_path(
                &obstacles,
                p(10, 10),
                p(90, 10),
                &[p(10, 10), p(10, 90), p(90, 90), p(90, 10)]
            )
            .is_ok()
        );
    }

    #[test]
    fn cleanup_handles_short_and_fully_collinear_paths() {
        let mut empty: Vec<Point> = vec![];
        cleanup(&mut empty);
        assert!(empty.is_empty());
        let mut one = vec![p(1, 1)];
        cleanup(&mut one);
        assert_eq!(one, vec![p(1, 1)]);
        let mut same = vec![p(1, 1), p(1, 1), p(1, 1)];
        cleanup(&mut same);
        assert_eq!(same, vec![p(1, 1)]);
        let mut line = vec![p(0, 0), p(3, 0), p(9, 0), p(2, 0), p(7, 0)];
        cleanup(&mut line);
        assert_eq!(line, vec![p(0, 0), p(7, 0)]);
        // an exact back-and-forth collapses to a single point
        let mut back = vec![p(0, 0), p(5, 0), p(0, 0)];
        cleanup(&mut back);
        assert_eq!(back, vec![p(0, 0)]);
    }

    #[test]
    fn validator_rejects_every_kind_of_defect() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(20, 20)));
        o.add_segment(Segment::vertical(10, 5, 15));
        o.add_segment(Segment::horizontal(3, 3, 3)); // point obstacle
        let (a, b) = (p(2, 10), p(18, 10));
        let err = |path: &[Point]| validate_path(&o, a, b, path).unwrap_err();
        assert!(err(&[]).contains("start"));
        assert!(err(&[b, a]).contains("start"));
        assert!(err(&[a, p(2, 16)]).contains("end"));
        assert!(err(&[a, p(2, 21), p(18, 21), b]).contains("bounds"));
        assert!(err(&[a, p(18, 16), b]).contains("diagonal"));
        assert!(err(&[a, a, p(2, 16), p(18, 16), b]).contains("zero-length"));
        assert!(err(&[a, p(2, 16), p(2, 17), p(18, 17), b]).contains("collinear"));
        // crossing the wall, T-touching its end, touching the point obstacle
        assert!(err(&[a, b]).contains("obstacle"));
        assert!(err(&[a, p(2, 15), p(18, 15), b]).contains("obstacle"));
        assert!(err(&[a, p(2, 3), p(18, 3), b]).contains("obstacle"));
        // one unit of clearance is enough, also along the bounds
        assert!(validate_path(&o, a, b, &[a, p(2, 16), p(18, 16), b]).is_ok());
        assert!(validate_path(&o, a, b, &[a, p(2, 20), p(18, 20), b]).is_ok());
        assert!(validate_path(&o, a, b, &[a, p(2, 4), p(18, 4), b]).is_ok());
        // single-point paths
        assert!(validate_path(&o, a, a, &[a]).is_ok());
        assert!(validate_path(&o, p(3, 3), p(3, 3), &[p(3, 3)]).is_err());
        assert!(validate_path(&o, p(30, 3), p(30, 3), &[p(30, 3)]).is_err());
    }

    #[test]
    fn extension_shortcut_respects_obstacles() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        // the same detour as above, but a wall blocks the shortcut at y = 20
        o.add_segment(Segment::vertical(45, 15, 25));
        let mut path = vec![
            p(0, 0),
            p(0, 20),
            p(30, 20),
            p(30, 5),
            p(60, 5),
            p(60, 20),
            p(90, 20),
        ];
        let before = path.clone();
        assert!(!improve_extension(&o, &mut path));
        assert_eq!(path, before);
        assert!(validate_path(&o, p(0, 0), p(90, 20), &path).is_ok());
    }

    #[test]
    fn probe_respects_obstacles_and_keeps_validity() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        // the u-turn bulge from above, but a point obstacle sits at (4, 5)
        o.add_segment(Segment::horizontal(5, 4, 4));
        let mut path = vec![p(0, 0), p(10, 0), p(10, 10), p(4, 10), p(4, 20)];
        assert!(improve_probe(&o, &mut path));
        // the probe from x = 4 is blocked, the one from x = 5 goes through
        assert_eq!(path, vec![p(0, 0), p(5, 0), p(5, 10), p(4, 10), p(4, 20)]);
        assert!(validate_path(&o, p(0, 0), p(4, 20), &path).is_ok());
    }
}
