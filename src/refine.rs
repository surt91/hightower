//! Path reconstruction from the two escape-point trees, collinear cleanup,
//! the paper's "second improvement" and a validity checker.

use crate::geometry::{Orientation, Point, Segment};
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
/// in. Removes staircases. Returns whether the path changed.
pub fn improve_probe(obstacles: &ObstacleSet, path: &mut Vec<Point>) -> bool {
    let mut changed = false;
    // Every splice strictly shortens the path, so this terminates; the cap
    // only guards against pathological inputs.
    let mut budget = 100_000usize;
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
            let mut q_along = end + step; // one unit before the far end
            while q_along != start + step {
                budget = budget.saturating_sub(1);
                if budget == 0 {
                    return changed;
                }
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
}
