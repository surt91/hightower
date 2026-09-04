//! Obstacle storage and the cover / escape-line queries the router is built on.

use std::collections::BTreeMap;

use crate::geometry::{Bounds, Coord, Orientation, Point, Segment};

/// A set of non-traversable axis-parallel segments inside a bounding box.
///
/// Horizontal segments are kept in a map from `y` to the x-spans on that row,
/// vertical segments in a map from `x` to the y-spans in that column. Cover
/// queries walk the map outward from the query point and stop at the first
/// row/column whose spans contain the point, so they cost O(rows visited).
///
/// The four edges of the bounding box are *not* stored as obstacles; escape
/// lines are clipped to the box directly.
#[derive(Clone, Debug)]
pub struct ObstacleSet {
    bounds: Bounds,
    horizontal: BTreeMap<Coord, Vec<(Coord, Coord)>>,
    vertical: BTreeMap<Coord, Vec<(Coord, Coord)>>,
    /// Rectangles added with [`add_rect`](Self::add_rect); their edges are also
    /// in the maps. Kept so that the visibility graph can skip the inside.
    rects: Vec<(Point, Point)>,
}

impl ObstacleSet {
    /// An empty obstacle set with the given routing area.
    pub fn new(bounds: Bounds) -> Self {
        ObstacleSet {
            bounds,
            horizontal: BTreeMap::new(),
            vertical: BTreeMap::new(),
            rects: Vec::new(),
        }
    }

    /// The routing area.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// Adds one segment. Duplicates and overlaps are allowed.
    pub fn add_segment(&mut self, s: Segment) {
        let map = self.map_mut(s.orientation);
        let spans = map.entry(s.fixed).or_default();
        let pos = spans.partition_point(|&(from, _)| from < s.from);
        spans.insert(pos, (s.from, s.to));
    }

    /// Adds the four edges of an axis-parallel rectangle.
    pub fn add_rect(&mut self, a: Point, b: Point) {
        let (x1, x2) = (a.x.min(b.x), a.x.max(b.x));
        let (y1, y2) = (a.y.min(b.y), a.y.max(b.y));
        self.add_segment(Segment::horizontal(y1, x1, x2));
        self.add_segment(Segment::horizontal(y2, x1, x2));
        self.add_segment(Segment::vertical(x1, y1, y2));
        self.add_segment(Segment::vertical(x2, y1, y2));
        self.rects.push((Point::new(x1, y1), Point::new(x2, y2)));
    }

    /// Rectangles added with [`add_rect`](Self::add_rect), as `(min, max)` corners.
    pub fn rects(&self) -> &[(Point, Point)] {
        &self.rects
    }

    /// Adds a rectilinear polyline (e.g. an already routed path) as obstacles,
    /// one segment per pair of consecutive corners. Diagonal pairs are ignored.
    pub fn add_path(&mut self, corners: &[Point]) {
        for pair in corners.windows(2) {
            if let Some(s) = Segment::between(pair[0], pair[1]) {
                self.add_segment(s);
            }
        }
    }

    /// Adds only the corners of a path as zero-length obstacles. This is the
    /// paper's "PERT diagram" mode: later paths may cross this one but cannot
    /// run along it or bend on it.
    pub fn add_path_corners(&mut self, corners: &[Point]) {
        for &c in corners {
            self.add_segment(Segment::horizontal(c.y, c.x, c.x));
        }
    }

    /// Iterates over all stored segments.
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        let h = self.horizontal.iter().flat_map(|(&y, spans)| {
            spans
                .iter()
                .map(move |&(a, b)| Segment::horizontal(y, a, b))
        });
        let v = self
            .vertical
            .iter()
            .flat_map(|(&x, spans)| spans.iter().map(move |&(a, b)| Segment::vertical(x, a, b)));
        h.chain(v)
    }

    /// Number of stored segments.
    pub fn len(&self) -> usize {
        self.horizontal.values().map(Vec::len).sum::<usize>()
            + self.vertical.values().map(Vec::len).sum::<usize>()
    }

    /// True if no segments are stored.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn map(&self, orientation: Orientation) -> &BTreeMap<Coord, Vec<(Coord, Coord)>> {
        match orientation {
            Orientation::Horizontal => &self.horizontal,
            Orientation::Vertical => &self.vertical,
        }
    }

    fn map_mut(&mut self, orientation: Orientation) -> &mut BTreeMap<Coord, Vec<(Coord, Coord)>> {
        match orientation {
            Orientation::Horizontal => &mut self.horizontal,
            Orientation::Vertical => &mut self.vertical,
        }
    }

    /// Does the point lie on any obstacle segment?
    pub fn is_on_obstacle(&self, p: Point) -> bool {
        let on_h = self
            .horizontal
            .get(&p.y)
            .is_some_and(|spans| spans.iter().any(|&(a, b)| a <= p.x && p.x <= b));
        let on_v = self
            .vertical
            .get(&p.x)
            .is_some_and(|spans| spans.iter().any(|&(a, b)| a <= p.y && p.y <= b));
        on_h || on_v
    }

    /// Is the point inside the bounds and not on an obstacle?
    pub fn is_free_point(&self, p: Point) -> bool {
        self.bounds.contains(p) && !self.is_on_obstacle(p)
    }

    /// Does the segment stay inside the bounds and avoid every obstacle
    /// (no crossing, no touching, no collinear overlap)?
    pub fn is_free_segment(&self, s: &Segment) -> bool {
        if !self.bounds.contains(s.start()) || !self.bounds.contains(s.end()) {
            return false;
        }
        let same = self.map(s.orientation);
        if same
            .get(&s.fixed)
            .is_some_and(|spans| spans.iter().any(|&(a, b)| a <= s.to && s.from <= b))
        {
            return false;
        }
        let perp = self.map(s.orientation.perpendicular());
        !perp
            .range(s.from..=s.to)
            .any(|(_, spans)| spans.iter().any(|&(a, b)| a <= s.fixed && s.fixed <= b))
    }

    /// Nearest segment of the given orientation that covers `p` and lies
    /// strictly on the positive side (above for horizontal, right for vertical).
    fn cover_positive(&self, p: Point, orientation: Orientation) -> Option<Segment> {
        let along = p.along(orientation);
        let across = p.across(orientation);
        self.map(orientation)
            .range(across + 1..)
            .find_map(|(&fixed, spans)| {
                spans
                    .iter()
                    .find(|&&(a, b)| a <= along && along <= b)
                    .map(|&(a, b)| Segment::new(orientation, fixed, a, b))
            })
    }

    /// Nearest segment of the given orientation that covers `p` and lies
    /// strictly on the negative side (below for horizontal, left for vertical).
    fn cover_negative(&self, p: Point, orientation: Orientation) -> Option<Segment> {
        let along = p.along(orientation);
        let across = p.across(orientation);
        self.map(orientation)
            .range(..across)
            .rev()
            .find_map(|(&fixed, spans)| {
                spans
                    .iter()
                    .find(|&&(a, b)| a <= along && along <= b)
                    .map(|&(a, b)| Segment::new(orientation, fixed, a, b))
            })
    }

    /// Nearest horizontal segment strictly above `p` whose x-span contains `p.x`.
    pub fn cover_above(&self, p: Point) -> Option<Segment> {
        self.cover_positive(p, Orientation::Horizontal)
    }

    /// Nearest horizontal segment strictly below `p` whose x-span contains `p.x`.
    pub fn cover_below(&self, p: Point) -> Option<Segment> {
        self.cover_negative(p, Orientation::Horizontal)
    }

    /// Nearest vertical segment strictly right of `p` whose y-span contains `p.y`.
    pub fn cover_right(&self, p: Point) -> Option<Segment> {
        self.cover_positive(p, Orientation::Vertical)
    }

    /// Nearest vertical segment strictly left of `p` whose y-span contains `p.y`.
    pub fn cover_left(&self, p: Point) -> Option<Segment> {
        self.cover_negative(p, Orientation::Vertical)
    }

    /// The two covers perpendicular to `orientation` that bound the escape
    /// line of that orientation: `(negative side, positive side)`.
    /// For a horizontal escape line these are the vertical covers (left, right),
    /// for a vertical escape line the horizontal covers (below, above).
    pub fn bounding_covers(
        &self,
        p: Point,
        orientation: Orientation,
    ) -> (Option<Segment>, Option<Segment>) {
        let perp = orientation.perpendicular();
        (self.cover_negative(p, perp), self.cover_positive(p, perp))
    }

    /// The maximal obstacle-free segment of the given orientation through `p`.
    ///
    /// It ends one unit short of the bounding covers (giving the path a
    /// clearance of one unit), or on the bounding box, and also stops one unit
    /// short of any collinear obstacle on the same row/column. The result can
    /// have zero length. `p` must be a free point.
    pub fn escape_line(&self, p: Point, orientation: Orientation) -> Segment {
        let along = p.along(orientation);
        let across = p.across(orientation);
        let mut lo = self.bounds.lo(orientation);
        let mut hi = self.bounds.hi(orientation);
        let (neg, pos) = self.bounding_covers(p, orientation);
        if let Some(c) = neg {
            lo = lo.max(c.fixed + 1);
        }
        if let Some(c) = pos {
            hi = hi.min(c.fixed - 1);
        }
        if let Some(spans) = self.map(orientation).get(&across) {
            for &(a, b) in spans {
                if b < along {
                    lo = lo.max(b + 1);
                } else if a > along {
                    hi = hi.min(a - 1);
                }
            }
        }
        debug_assert!(
            lo <= along && along <= hi,
            "escape_line called for a blocked point {p:?}"
        );
        Segment {
            orientation,
            fixed: across,
            from: lo.min(along),
            to: hi.max(along),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> ObstacleSet {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(100, 100)));
        o.add_segment(Segment::horizontal(50, 20, 60)); // ceiling
        o.add_segment(Segment::horizontal(10, 0, 100)); // floor
        o.add_segment(Segment::vertical(70, 0, 100)); // wall right
        o
    }

    #[test]
    fn covers_are_nearest_covering_segments() {
        let o = set();
        let p = Point::new(30, 30);
        assert_eq!(o.cover_above(p), Some(Segment::horizontal(50, 20, 60)));
        assert_eq!(o.cover_below(p), Some(Segment::horizontal(10, 0, 100)));
        assert_eq!(o.cover_right(p), Some(Segment::vertical(70, 0, 100)));
        assert_eq!(o.cover_left(p), None);
        // exactly beside the ceiling's end: not covered
        assert_eq!(o.cover_above(Point::new(61, 30)), None);
        // exactly at the end: covered (inclusive)
        assert_eq!(
            o.cover_above(Point::new(60, 30)),
            Some(Segment::horizontal(50, 20, 60))
        );
    }

    #[test]
    fn escape_lines_stop_one_unit_short() {
        let o = set();
        let p = Point::new(30, 30);
        assert_eq!(
            o.escape_line(p, Orientation::Vertical),
            Segment::vertical(30, 11, 49)
        );
        assert_eq!(
            o.escape_line(p, Orientation::Horizontal),
            Segment::horizontal(30, 0, 69)
        );
        // point in the open: clipped to the bounds
        let q = Point::new(80, 80);
        assert_eq!(
            o.escape_line(q, Orientation::Horizontal),
            Segment::horizontal(80, 71, 100)
        );
        assert_eq!(
            o.escape_line(q, Orientation::Vertical),
            Segment::vertical(80, 11, 100)
        );
    }

    #[test]
    fn zero_length_escape_line() {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(10, 10)));
        o.add_segment(Segment::horizontal(4, 0, 10));
        o.add_segment(Segment::horizontal(6, 0, 10));
        let p = Point::new(5, 5);
        let l = o.escape_line(p, Orientation::Vertical);
        assert_eq!(l, Segment::vertical(5, 5, 5));
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn collinear_obstacle_blocks_escape_line() {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(10, 10)));
        o.add_segment(Segment::horizontal(5, 7, 9));
        let l = o.escape_line(Point::new(3, 5), Orientation::Horizontal);
        assert_eq!(l, Segment::horizontal(5, 0, 6));
    }

    #[test]
    fn point_and_segment_freedom() {
        let o = set();
        assert!(o.is_on_obstacle(Point::new(20, 50)));
        assert!(o.is_on_obstacle(Point::new(70, 3)));
        assert!(!o.is_on_obstacle(Point::new(19, 50)));
        assert!(o.is_free_segment(&Segment::vertical(30, 11, 49)));
        assert!(!o.is_free_segment(&Segment::vertical(30, 11, 50))); // T-touch
        assert!(!o.is_free_segment(&Segment::horizontal(50, 60, 65))); // collinear touch at 60
        assert!(o.is_free_segment(&Segment::horizontal(50, 61, 65)));
        assert!(!o.is_free_segment(&Segment::horizontal(30, 0, 101))); // leaves the bounds
    }

    #[test]
    fn rect_and_path_helpers() {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(10, 10)));
        o.add_rect(Point::new(2, 2), Point::new(5, 4));
        assert_eq!(o.len(), 4);
        o.add_path(&[Point::new(7, 7), Point::new(9, 7), Point::new(9, 9)]);
        assert_eq!(o.len(), 6);
        assert!(o.is_on_obstacle(Point::new(9, 8)));
    }

    #[test]
    fn covers_and_escape_lines_with_negative_coordinates() {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(-50, -50), Point::new(-10, -10)));
        o.add_segment(Segment::horizontal(-20, -40, -30)); // above p
        o.add_segment(Segment::vertical(-45, -50, -10)); // left of p, full height
        let p = Point::new(-35, -35);
        assert_eq!(o.cover_above(p), Some(Segment::horizontal(-20, -40, -30)));
        assert_eq!(o.cover_below(p), None);
        assert_eq!(o.cover_left(p), Some(Segment::vertical(-45, -50, -10)));
        assert_eq!(o.cover_right(p), None);
        assert_eq!(
            o.escape_line(p, Orientation::Vertical),
            Segment::vertical(-35, -50, -21)
        );
        assert_eq!(
            o.escape_line(p, Orientation::Horizontal),
            Segment::horizontal(-35, -44, -10)
        );
    }

    #[test]
    fn duplicate_and_overlapping_obstacles_do_not_change_queries() {
        let mut once = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(50, 50)));
        let mut twice = once.clone();
        for s in [
            Segment::horizontal(30, 10, 40),
            Segment::vertical(20, 0, 25),
            Segment::horizontal(30, 15, 45), // overlaps the first
        ] {
            once.add_segment(s);
            twice.add_segment(s);
            twice.add_segment(s);
        }
        assert_eq!(once.len() * 2, twice.len());
        for x in 0..=50 {
            for y in 0..=50 {
                let p = Point::new(x, y);
                assert_eq!(once.is_on_obstacle(p), twice.is_on_obstacle(p));
                if once.is_on_obstacle(p) {
                    continue;
                }
                for o in [Orientation::Horizontal, Orientation::Vertical] {
                    assert_eq!(once.escape_line(p, o), twice.escape_line(p, o), "{p:?}");
                }
                // an escape line is always a free segment
                assert!(once.is_free_segment(&once.escape_line(p, Orientation::Horizontal)));
                assert!(once.is_free_segment(&once.escape_line(p, Orientation::Vertical)));
            }
        }
    }

    #[test]
    fn zero_length_obstacles_act_as_covers_and_collinear_blockers() {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(10, 10)));
        o.add_segment(Segment::horizontal(5, 5, 5)); // the point (5,5)
        assert!(o.is_on_obstacle(Point::new(5, 5)));
        // directly below the point: it is the cover above, at distance 1
        let below = Point::new(5, 4);
        assert_eq!(o.cover_above(below), Some(Segment::horizontal(5, 5, 5)));
        assert_eq!(
            o.escape_line(below, Orientation::Vertical),
            Segment::vertical(5, 0, 4)
        );
        // on the same row: blocks collinearly
        assert_eq!(
            o.escape_line(Point::new(2, 5), Orientation::Horizontal),
            Segment::horizontal(5, 0, 4)
        );
        assert_eq!(
            o.escape_line(Point::new(8, 5), Orientation::Horizontal),
            Segment::horizontal(5, 6, 10)
        );
        // the vertical line at x = 5 through a point above is stopped, too
        assert_eq!(
            o.escape_line(Point::new(5, 8), Orientation::Vertical),
            Segment::vertical(5, 6, 10)
        );
        // one column over the point is invisible
        assert_eq!(
            o.escape_line(Point::new(6, 8), Orientation::Vertical),
            Segment::vertical(6, 0, 10)
        );
        // a zero-length path segment (a point) is free iff the point is
        assert!(!o.is_free_segment(&Segment::horizontal(5, 5, 5)));
        assert!(!o.is_free_segment(&Segment::vertical(5, 5, 5)));
        assert!(o.is_free_segment(&Segment::vertical(5, 6, 6)));
        // add_path_corners stores points
        o.add_path_corners(&[Point::new(1, 1), Point::new(1, 3)]);
        assert!(o.is_on_obstacle(Point::new(1, 1)));
        assert!(o.is_on_obstacle(Point::new(1, 3)));
        assert!(!o.is_on_obstacle(Point::new(1, 2)));
    }

    #[test]
    fn points_on_the_bounds_and_covers_at_distance_one() {
        let mut o = ObstacleSet::new(Bounds::new(Point::new(0, 0), Point::new(10, 10)));
        o.add_segment(Segment::vertical(1, 0, 10)); // wall right next to the left edge
        let corner = Point::new(0, 0);
        assert!(o.is_free_point(corner));
        assert_eq!(
            o.escape_line(corner, Orientation::Horizontal),
            Segment::horizontal(0, 0, 0)
        );
        assert_eq!(
            o.escape_line(corner, Orientation::Vertical),
            Segment::vertical(0, 0, 10)
        );
        let top = Point::new(5, 10);
        assert_eq!(
            o.escape_line(top, Orientation::Horizontal),
            Segment::horizontal(10, 2, 10)
        );
        assert!(!o.is_free_point(Point::new(11, 5)));
        assert!(!o.is_free_point(Point::new(5, -1)));
        // a wall touching the bounds leaves no gap
        assert!(!o.is_free_segment(&Segment::horizontal(0, 0, 2)));
    }
}
