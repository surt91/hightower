//! Hightower's line-search routing algorithm on the continuous plane.
//!
//! An implementation of D. W. Hightower, *"A Solution to Line-Routing Problems
//! on the Continuous Plane"*, DAC 1969 (DOI 10.1145/800260.809014): find a
//! rectilinear path (axis-parallel segments, right-angle bends) between two
//! points that avoids a set of axis-parallel obstacle segments.
//!
//! Two networks of *escape lines* grow alternately from the start and the
//! target point. Each escape line is the longest obstacle-free horizontal or
//! vertical segment through a point; new lines branch off around the ends of
//! the obstacles that stopped the old ones. As soon as a line of one network
//! crosses a line of the other, a path exists and is read off the two trees.
//!
//! # Properties
//!
//! * **Fast and memory-light.** The router only ever constructs line segments;
//!   there is no grid, so cost depends on the clutter, not on the area.
//! * **Few bends, not shortest.** Paths tend to be straight and calm but are
//!   *not* guaranteed to be shortest.
//! * **Not complete.** Escape lines are never reused. This guarantees
//!   termination but means the algorithm can miss a path that exists (for
//!   example out of a box entered through a narrow mouth). Callers must handle
//!   `None` even for connected instances, e.g. by falling back to a grid search.
//! * **Exact.** All coordinates are `i64`; one unit is the minimum clearance
//!   between the path and any obstacle.
//!
//! # Example
//!
//! ```
//! use hightower::{Bounds, ObstacleSet, Point, route};
//!
//! let mut obstacles = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(100, 100)));
//! obstacles.add_rect(Point::new(40, 20), Point::new(60, 80));
//!
//! let path = route(&obstacles, Point::new(10, 50), Point::new(90, 50)).expect("a path exists");
//! assert_eq!(path.first(), Some(&Point::new(10, 50)));
//! assert_eq!(path.last(), Some(&Point::new(90, 50)));
//! assert!(hightower::validate_path(&obstacles, path[0], path[path.len() - 1], &path).is_ok());
//! ```
//!
//! For the full record of what the router did (every escape line, escape point
//! and intersection) use [`route_with`] and render the [`Trace`] with the
//! [`svg`] module.
//!
//! Two reference routers ship alongside: [`grid::route_grid`], a Lee-style
//! breadth-first search on the unit lattice (complete, shortest, cost grows
//! with the area), and [`route_visibility`], A* over the orthogonal visibility
//! graph (complete, shortest or bend-optimised, cost grows with the number of
//! obstacles). The latter is the modern standard for diagram editors and the
//! natural fallback when Hightower returns `None`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod escape;
pub mod geometry;
pub mod grid;
pub mod obstacles;
pub mod refine;
pub mod router;
pub mod svg;
pub mod trace;
pub mod visibility;

pub use geometry::{Bounds, Coord, Orientation, Point, Segment};
pub use obstacles::ObstacleSet;
pub use refine::{cleanup, validate_path};
pub use router::{Improvement, Outcome, RouteResult, RouterConfig, route, route_with};
pub use trace::{NetId, Process, Trace, TraceEvent};
pub use visibility::{
    VisibilityConfig, VisibilityGraph, VisibilityResult, route_visibility, route_visibility_with,
};
