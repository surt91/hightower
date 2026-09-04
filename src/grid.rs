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
