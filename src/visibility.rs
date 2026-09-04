//! Orthogonal visibility graph plus A*: the modern successor of line search
//! (Wybrow, Marriott, Stuckey, *Orthogonal Connector Routing*, GD 2009; the
//! approach behind libavoid).
//!
//! The horizontal and vertical lines through every obstacle end (offset by
//! one unit of clearance), through both endpoints and along the bounds form a
//! sparse grid whose free intersections are the nodes and whose free stretches
//! between neighbouring nodes are the edges. Every rectilinear path can be
//! pushed onto this grid without getting longer, so a search over it is
//! **complete** and, for [`VisibilityConfig::bend_penalty`] `== 0`, finds a
//! **shortest** path. With a positive penalty it minimises
//! `length + bend_penalty * bends` over the graph, which is the objective a
//! diagram editor actually wants.
//!
//! Like Hightower's router this needs no area grid: the graph has
//! `O(n²)` nodes for `n` obstacle segments, independent of the board size. The
//! graph is never materialised; A* discovers nodes and edges on demand and asks
//! the [`ObstacleSet`] whether a stretch is free.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use crate::geometry::{Coord, Point, Segment};
use crate::obstacles::ObstacleSet;
use crate::refine::cleanup;

/// Cost model of the visibility-graph router.
#[derive(Clone, Debug, Default)]
pub struct VisibilityConfig {
    /// Extra cost of every bend, in coordinate units. `0` gives a shortest
    /// path; a value comparable to the typical obstacle size gives calm paths
    /// with few bends.
    pub bend_penalty: Coord,
}

/// Result of [`route_visibility_with`].
#[derive(Clone, Debug)]
pub struct VisibilityResult {
    /// Corner list from `a` to `b`, or `None` if `b` is unreachable.
    pub path: Option<Vec<Point>>,
    /// Total cost (`length + bend_penalty * bends`) of the path.
    pub cost: Option<Coord>,
    /// Number of graph nodes (candidate x-coordinates times candidate y-coordinates).
    pub graph_nodes: usize,
    /// Number of A* states expanded.
    pub expanded: usize,
}

/// The sparse grid of candidate coordinates over an obstacle set.
#[derive(Clone, Debug)]
pub struct VisibilityGraph<'a> {
    obstacles: &'a ObstacleSet,
    xs: Vec<Coord>,
    ys: Vec<Coord>,
}

impl<'a> VisibilityGraph<'a> {
    /// Builds the candidate coordinates: for every rectangle its four sides
    /// pushed one unit outward; for every loose segment its fixed coordinate
    /// ± 1 and its two ends ∓ 1 (so a path can hug it at unit clearance on
    /// either side); plus the coordinates of `terminals` and the bounds.
    pub fn new(obstacles: &'a ObstacleSet, terminals: &[Point]) -> Self {
        let bounds = obstacles.bounds();
        let mut xs = vec![bounds.min.x, bounds.max.x];
        let mut ys = vec![bounds.min.y, bounds.max.y];
        for p in terminals {
            xs.push(p.x);
            ys.push(p.y);
        }
        let mut rect_edges = HashSet::new();
        for &(min, max) in obstacles.rects() {
            xs.push(min.x - 1);
            xs.push(max.x + 1);
            ys.push(min.y - 1);
            ys.push(max.y + 1);
            rect_edges.extend(crate::svg::rect_segments(min, max));
        }
        for s in obstacles.segments() {
            if rect_edges.contains(&s) {
                continue;
            }
            let (along, across) = match s.orientation {
                crate::geometry::Orientation::Horizontal => (&mut xs, &mut ys),
                crate::geometry::Orientation::Vertical => (&mut ys, &mut xs),
            };
            along.push(s.from - 1);
            along.push(s.to + 1);
            across.push(s.fixed - 1);
            across.push(s.fixed + 1);
        }
        let clip = |v: &mut Vec<Coord>, lo: Coord, hi: Coord| {
            v.retain(|&c| lo <= c && c <= hi);
            v.sort_unstable();
            v.dedup();
        };
        clip(&mut xs, bounds.min.x, bounds.max.x);
        clip(&mut ys, bounds.min.y, bounds.max.y);
        VisibilityGraph { obstacles, xs, ys }
    }

    /// Candidate x-coordinates (sorted).
    pub fn xs(&self) -> &[Coord] {
        &self.xs
    }

    /// Candidate y-coordinates (sorted).
    pub fn ys(&self) -> &[Coord] {
        &self.ys
    }

    /// Number of nodes of the (implicit) graph.
    pub fn node_count(&self) -> usize {
        self.xs.len() * self.ys.len()
    }

    /// All free edges between neighbouring candidate points. Only needed for
    /// drawing the graph; routing never enumerates it.
    pub fn edges(&self) -> Vec<Segment> {
        let mut edges = Vec::new();
        for &y in &self.ys {
            for w in self.xs.windows(2) {
                let s = Segment::horizontal(y, w[0], w[1]);
                if self.obstacles.is_free_segment(&s) {
                    edges.push(s);
                }
            }
        }
        for &x in &self.xs {
            for w in self.ys.windows(2) {
                let s = Segment::vertical(x, w[0], w[1]);
                if self.obstacles.is_free_segment(&s) {
                    edges.push(s);
                }
            }
        }
        edges
    }

    fn index_of(&self, p: Point) -> Option<(usize, usize)> {
        let i = self.xs.binary_search(&p.x).ok()?;
        let j = self.ys.binary_search(&p.y).ok()?;
        Some((i, j))
    }

    /// A* from `a` to `b` over the implicit graph.
    pub fn route(&self, a: Point, b: Point, config: &VisibilityConfig) -> VisibilityResult {
        let graph_nodes = self.node_count();
        let fail = |expanded| VisibilityResult {
            path: None,
            cost: None,
            graph_nodes,
            expanded,
        };
        if !self.obstacles.is_free_point(a) || !self.obstacles.is_free_point(b) {
            return fail(0);
        }
        let (Some(start), Some(goal)) = (self.index_of(a), self.index_of(b)) else {
            return fail(0);
        };
        if a == b {
            return VisibilityResult {
                path: Some(vec![a]),
                cost: Some(0),
                graph_nodes,
                expanded: 0,
            };
        }
        let penalty = config.bend_penalty;

        // A state is a node plus the axis of the edge that led to it
        // (AXIS_NONE at the start), so bends can be charged. States are
        // indexed densely: (i * |ys| + j) * 3 + axis.
        const AXIS_H: u8 = 0;
        const AXIS_V: u8 = 1;
        const AXIS_NONE: u8 = 2;
        let ny = self.ys.len();
        let key = |i: usize, j: usize, axis: u8| -> usize { (i * ny + j) * 3 + axis as usize };
        let unkey =
            |k: usize| -> (usize, usize, u8) { ((k / 3) / ny, (k / 3) % ny, (k % 3) as u8) };
        let heuristic = |i: usize, j: usize, axis: u8| -> Coord {
            let dx = (self.xs[i] - b.x).abs();
            let dy = (self.ys[j] - b.y).abs();
            let bends_needed = match (dx > 0, dy > 0, axis) {
                (true, true, _) => 1,
                (true, false, AXIS_V) | (false, true, AXIS_H) => 1,
                _ => 0,
            };
            dx + dy + bends_needed * penalty
        };

        let states = graph_nodes * 3;
        let mut g: Vec<Coord> = vec![Coord::MAX; states];
        let mut parent: Vec<usize> = vec![usize::MAX; states];
        // Ordered by f, then by *larger* g (deeper first), which breaks the
        // many ties of a rectilinear heuristic in favour of finishing a path.
        let mut open = BinaryHeap::new();
        let s = key(start.0, start.1, AXIS_NONE);
        g[s] = 0;
        open.push(Reverse((
            heuristic(start.0, start.1, AXIS_NONE),
            Reverse(0),
            s,
        )));
        let mut expanded = 0usize;
        let mut goal_key = None;

        while let Some(Reverse((_, Reverse(cost), k))) = open.pop() {
            if cost > g[k] {
                continue; // stale entry
            }
            let (i, j, axis) = unkey(k);
            if (i, j) == goal {
                goal_key = Some(k);
                break;
            }
            expanded += 1;
            let here = Point::new(self.xs[i], self.ys[j]);
            let mut relax = |ni: usize, nj: usize, naxis: u8, edge: Segment| {
                if !self.obstacles.is_free_segment(&edge) {
                    return;
                }
                let bend = if axis != AXIS_NONE && axis != naxis {
                    penalty
                } else {
                    0
                };
                let ng = cost + edge.len() + bend;
                let nk = key(ni, nj, naxis);
                if ng < g[nk] {
                    g[nk] = ng;
                    parent[nk] = k;
                    open.push(Reverse((ng + heuristic(ni, nj, naxis), Reverse(ng), nk)));
                }
            };
            if i + 1 < self.xs.len() {
                relax(
                    i + 1,
                    j,
                    AXIS_H,
                    Segment::horizontal(here.y, here.x, self.xs[i + 1]),
                );
            }
            if i > 0 {
                relax(
                    i - 1,
                    j,
                    AXIS_H,
                    Segment::horizontal(here.y, self.xs[i - 1], here.x),
                );
            }
            if j + 1 < self.ys.len() {
                relax(
                    i,
                    j + 1,
                    AXIS_V,
                    Segment::vertical(here.x, here.y, self.ys[j + 1]),
                );
            }
            if j > 0 {
                relax(
                    i,
                    j - 1,
                    AXIS_V,
                    Segment::vertical(here.x, self.ys[j - 1], here.y),
                );
            }
        }

        let Some(mut k) = goal_key else {
            return fail(expanded);
        };
        let cost = g[k];
        let mut path = Vec::new();
        loop {
            let (i, j, _) = unkey(k);
            path.push(Point::new(self.xs[i], self.ys[j]));
            if parent[k] == usize::MAX {
                break;
            }
            k = parent[k];
        }
        path.reverse();
        cleanup(&mut path);
        VisibilityResult {
            path: Some(path),
            cost: Some(cost),
            graph_nodes,
            expanded,
        }
    }
}

/// Shortest rectilinear path over the orthogonal visibility graph, or `None`
/// if none exists. Complete: `None` means `b` is unreachable.
pub fn route_visibility(obstacles: &ObstacleSet, a: Point, b: Point) -> Option<Vec<Point>> {
    route_visibility_with(obstacles, a, b, &VisibilityConfig::default()).path
}

/// Routes over the orthogonal visibility graph with the given cost model.
pub fn route_visibility_with(
    obstacles: &ObstacleSet,
    a: Point,
    b: Point,
    config: &VisibilityConfig,
) -> VisibilityResult {
    VisibilityGraph::new(obstacles, &[a, b]).route(a, b, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Bounds;
    use crate::refine::validate_path;

    fn p(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn candidate_coordinates_hug_obstacles() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(50, 50)));
        o.add_rect(p(10, 20), p(30, 40));
        let g = VisibilityGraph::new(&o, &[p(5, 5)]);
        assert_eq!(g.xs(), &[0, 5, 9, 31, 50]);
        assert_eq!(g.ys(), &[0, 5, 19, 41, 50]);
        // a loose segment gets both sides
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(50, 50)));
        o.add_segment(Segment::horizontal(20, 10, 30));
        let g = VisibilityGraph::new(&o, &[]);
        assert_eq!(g.xs(), &[0, 9, 31, 50]);
        assert_eq!(g.ys(), &[0, 19, 21, 50]);
    }

    #[test]
    fn routes_around_a_box_with_shortest_length() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        o.add_rect(p(40, 20), p(60, 80));
        let (a, b) = (p(10, 50), p(90, 50));
        let r = route_visibility_with(&o, a, b, &VisibilityConfig::default());
        let path = r.path.expect("path");
        validate_path(&o, a, b, &path).unwrap();
        let len: i64 = path.windows(2).map(|w| w[0].manhattan(w[1])).sum();
        // 80 across plus 2 * 31 to clear the box (to y = 19 or 81)
        assert_eq!(len, 80 + 62);
        assert_eq!(r.cost, Some(len));
    }

    #[test]
    fn bend_penalty_trades_length_for_bends() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        o.add_rect(p(30, 20), p(45, 55));
        o.add_rect(p(55, 45), p(70, 80));
        let (a, b) = (p(10, 50), p(90, 50));
        let short = route_visibility_with(&o, a, b, &VisibilityConfig { bend_penalty: 0 })
            .path
            .unwrap();
        let calm = route_visibility_with(&o, a, b, &VisibilityConfig { bend_penalty: 30 })
            .path
            .unwrap();
        validate_path(&o, a, b, &short).unwrap();
        validate_path(&o, a, b, &calm).unwrap();
        assert!(short.len() > calm.len(), "short {short:?} calm {calm:?}");
    }

    #[test]
    fn unreachable_and_degenerate_inputs() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(100, 100)));
        o.add_rect(p(30, 30), p(50, 50));
        assert!(route_visibility(&o, p(10, 10), p(40, 40)).is_none());
        assert_eq!(route_visibility(&o, p(5, 5), p(5, 5)), Some(vec![p(5, 5)]));
        assert!(route_visibility(&o, p(30, 40), p(5, 5)).is_none()); // on an obstacle
    }
}
