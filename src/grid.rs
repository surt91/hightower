//! A naive Lee-style grid router (breadth-first flood fill on the unit
//! lattice). It is complete and finds shortest paths, and it exists here only
//! as a reference for tests, benchmarks and figures: it does the work
//! Hightower's algorithm avoids.

use std::collections::VecDeque;

use crate::geometry::{Coord, Point};
use crate::obstacles::ObstacleSet;
use crate::refine::cleanup;

/// Result of a flood fill: the shortest path (if any) and the BFS distance of
/// every visited lattice point, for visualization.
#[derive(Clone, Debug)]
pub struct FloodResult {
    /// Corner list from `a` to `b`, or `None` if `b` is unreachable.
    pub path: Option<Vec<Point>>,
    /// Every visited lattice point with its BFS distance from `a`.
    pub visited: Vec<(Point, u32)>,
}

/// Breadth-first search on the unit lattice inside the bounds. A lattice point
/// is passable if it is not on an obstacle; because obstacles have integer
/// coordinates this is exactly the validity criterion of the line router.
pub fn flood(obstacles: &ObstacleSet, a: Point, b: Point) -> FloodResult {
    let bounds = obstacles.bounds();
    let w = (bounds.width() + 1) as usize;
    let h = (bounds.height() + 1) as usize;
    let idx =
        |p: Point| -> usize { ((p.y - bounds.min.y) as usize) * w + (p.x - bounds.min.x) as usize };
    let mut dist = vec![u32::MAX; w * h];
    let mut prev = vec![usize::MAX; w * h];
    let mut visited = Vec::new();
    if !obstacles.is_free_point(a) || !obstacles.is_free_point(b) {
        return FloodResult {
            path: None,
            visited,
        };
    }
    let mut queue = VecDeque::new();
    dist[idx(a)] = 0;
    queue.push_back(a);
    let mut found = false;
    while let Some(p) = queue.pop_front() {
        let d = dist[idx(p)];
        visited.push((p, d));
        if p == b {
            found = true;
            break;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let q = Point::new(p.x + dx, p.y + dy);
            if !bounds.contains(q) || dist[idx(q)] != u32::MAX || obstacles.is_on_obstacle(q) {
                continue;
            }
            dist[idx(q)] = d + 1;
            prev[idx(q)] = idx(p);
            queue.push_back(q);
        }
    }
    let path = found.then(|| {
        let mut corners = vec![b];
        let mut cur = idx(b);
        while cur != idx(a) {
            cur = prev[cur];
            let x = bounds.min.x + (cur % w) as Coord;
            let y = bounds.min.y + (cur / w) as Coord;
            corners.push(Point::new(x, y));
        }
        corners.reverse();
        cleanup(&mut corners);
        corners
    });
    FloodResult { path, visited }
}

/// Shortest rectilinear path on the unit lattice, or `None` if none exists.
pub fn route_grid(obstacles: &ObstacleSet, a: Point, b: Point) -> Option<Vec<Point>> {
    flood(obstacles, a, b).path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Bounds, Segment};
    use crate::refine::validate_path;

    fn p(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn bfs_works_with_negative_bounds_and_is_shortest() {
        let mut o = ObstacleSet::new(Bounds::new(p(-10, -10), p(9, 9)));
        o.add_segment(Segment::vertical(0, -10, 5)); // wall from the bottom edge up to y = 5
        let (a, b) = (p(-5, 0), p(5, 0));
        let path = route_grid(&o, a, b).expect("path over the top");
        validate_path(&o, a, b, &path).unwrap();
        let len: i64 = path.windows(2).map(|w| w[0].manhattan(w[1])).sum();
        // 10 across plus up to y = 6 and back down
        assert_eq!(len, 10 + 12);
        assert!(path.iter().any(|c| c.y >= 6));
        // fully blocked
        o.add_segment(Segment::vertical(0, 6, 9));
        assert!(route_grid(&o, a, b).is_none());
        // degenerate inputs
        assert_eq!(route_grid(&o, a, a), Some(vec![a]));
        assert!(route_grid(&o, p(0, 0), a).is_none()); // on an obstacle
        assert!(route_grid(&o, p(-11, 0), a).is_none()); // outside
    }

    #[test]
    fn flood_visits_every_reachable_point_in_a_closed_room() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(10, 10)));
        o.add_rect(p(2, 2), p(6, 6));
        // the inside is 3 x 3 = 9 free points, the target is outside
        let r = flood(&o, p(4, 4), p(9, 9));
        assert!(r.path.is_none());
        assert_eq!(r.visited.len(), 9);
        assert!(
            r.visited
                .iter()
                .all(|&(q, _)| (3..=5).contains(&q.x) && (3..=5).contains(&q.y))
        );
    }
}
