//! Basic integer geometry: points, orientations, axis-parallel segments and
//! the bounding box. All coordinates are `i64`; every predicate is exact.

/// Coordinate type used throughout the crate. Integer only, no floats.
pub type Coord = i64;

/// A point in the plane.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: Coord,
    /// Vertical coordinate (grows upward).
    pub y: Coord,
}

impl Point {
    /// Creates a point.
    pub const fn new(x: Coord, y: Coord) -> Self {
        Point { x, y }
    }

    /// Squared Euclidean distance, computed in `i128` so it cannot overflow.
    pub fn dist2(self, other: Point) -> i128 {
        let dx = (self.x - other.x) as i128;
        let dy = (self.y - other.y) as i128;
        dx * dx + dy * dy
    }

    /// Manhattan (taxicab) distance.
    pub fn manhattan(self, other: Point) -> Coord {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }

    /// The coordinate along the given orientation: `x` for horizontal, `y` for vertical.
    pub fn along(self, orientation: Orientation) -> Coord {
        match orientation {
            Orientation::Horizontal => self.x,
            Orientation::Vertical => self.y,
        }
    }

    /// The coordinate perpendicular to the given orientation: `y` for horizontal, `x` for vertical.
    pub fn across(self, orientation: Orientation) -> Coord {
        match orientation {
            Orientation::Horizontal => self.y,
            Orientation::Vertical => self.x,
        }
    }

    /// Builds a point from its coordinate along and across an orientation.
    pub fn from_along_across(orientation: Orientation, along: Coord, across: Coord) -> Point {
        match orientation {
            Orientation::Horizontal => Point::new(along, across),
            Orientation::Vertical => Point::new(across, along),
        }
    }
}

impl From<(Coord, Coord)> for Point {
    fn from((x, y): (Coord, Coord)) -> Self {
        Point::new(x, y)
    }
}

/// Orientation of an axis-parallel segment.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Orientation {
    /// Parallel to the x-axis.
    Horizontal,
    /// Parallel to the y-axis.
    Vertical,
}

impl Orientation {
    /// The other orientation.
    pub const fn perpendicular(self) -> Orientation {
        match self {
            Orientation::Horizontal => Orientation::Vertical,
            Orientation::Vertical => Orientation::Horizontal,
        }
    }
}

/// An axis-parallel segment: `fixed` is the constant coordinate (`y` for a
/// horizontal segment, `x` for a vertical one) and `[from, to]` the inclusive
/// span along the other axis. A point is a zero-length segment (`from == to`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Segment {
    /// Direction the segment runs in.
    pub orientation: Orientation,
    /// The constant coordinate (`y` if horizontal, `x` if vertical).
    pub fixed: Coord,
    /// Start of the inclusive span along the running axis.
    pub from: Coord,
    /// End of the inclusive span along the running axis (`from <= to`).
    pub to: Coord,
}

impl Segment {
    /// Creates a segment; the span is normalized so that `from <= to`.
    pub fn new(orientation: Orientation, fixed: Coord, a: Coord, b: Coord) -> Self {
        Segment {
            orientation,
            fixed,
            from: a.min(b),
            to: a.max(b),
        }
    }

    /// Horizontal segment `{ y, x in [x1, x2] }`.
    pub fn horizontal(y: Coord, x1: Coord, x2: Coord) -> Self {
        Segment::new(Orientation::Horizontal, y, x1, x2)
    }

    /// Vertical segment `{ x, y in [y1, y2] }`.
    pub fn vertical(x: Coord, y1: Coord, y2: Coord) -> Self {
        Segment::new(Orientation::Vertical, x, y1, y2)
    }

    /// The segment between two points, if they are axis-aligned.
    /// Two equal points yield a horizontal zero-length segment.
    pub fn between(a: Point, b: Point) -> Option<Segment> {
        if a.y == b.y {
            Some(Segment::horizontal(a.y, a.x, b.x))
        } else if a.x == b.x {
            Some(Segment::vertical(a.x, a.y, b.y))
        } else {
            None
        }
    }

    /// Length of the segment (zero for a point).
    pub fn len(&self) -> Coord {
        self.to - self.from
    }

    /// Is the segment a single point?
    pub fn is_empty(&self) -> bool {
        self.from == self.to
    }

    /// Endpoint with the smaller running coordinate.
    pub fn start(&self) -> Point {
        Point::from_along_across(self.orientation, self.from, self.fixed)
    }

    /// Endpoint with the larger running coordinate.
    pub fn end(&self) -> Point {
        Point::from_along_across(self.orientation, self.to, self.fixed)
    }

    /// Both endpoints.
    pub fn endpoints(&self) -> [Point; 2] {
        [self.start(), self.end()]
    }

    /// Does the perpendicular from `p` onto this segment's line hit the segment?
    /// (Hightower's "cover" verb.) The point may lie on either side or on the line.
    pub fn covers(&self, p: Point) -> bool {
        let a = p.along(self.orientation);
        self.from <= a && a <= self.to
    }

    /// Is `p` on the segment (inclusive endpoints)?
    pub fn contains(&self, p: Point) -> bool {
        p.across(self.orientation) == self.fixed && self.covers(p)
    }

    /// Do the two segments have the same orientation and fixed coordinate and
    /// share at least one point? Zero-length segments are handled.
    pub fn overlaps_collinear(&self, other: &Segment) -> bool {
        self.orientation == other.orientation
            && self.fixed == other.fixed
            && self.from <= other.to
            && other.from <= self.to
    }

    /// If the segments are perpendicular and cross (touching at an endpoint
    /// counts), returns the crossing point.
    pub fn crossing(&self, other: &Segment) -> Option<Point> {
        if self.orientation == other.orientation {
            return None;
        }
        let on_self = self.from <= other.fixed && other.fixed <= self.to;
        let on_other = other.from <= self.fixed && self.fixed <= other.to;
        if on_self && on_other {
            Some(Point::from_along_across(
                self.orientation,
                other.fixed,
                self.fixed,
            ))
        } else {
            None
        }
    }

    /// Any contact at all: perpendicular crossing, T-touch, endpoint touch or
    /// collinear overlap.
    pub fn touches(&self, other: &Segment) -> bool {
        if self.orientation == other.orientation {
            self.overlaps_collinear(other)
        } else {
            self.crossing(other).is_some()
        }
    }
}

/// The routing area. Escape lines are clipped to it; both limits are inclusive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bounds {
    /// Lower-left corner (inclusive).
    pub min: Point,
    /// Upper-right corner (inclusive).
    pub max: Point,
}

impl Bounds {
    /// Creates a bounding box from two corners (normalized).
    pub fn new(a: Point, b: Point) -> Self {
        Bounds {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Width along x.
    pub fn width(&self) -> Coord {
        self.max.x - self.min.x
    }

    /// Height along y.
    pub fn height(&self) -> Coord {
        self.max.y - self.min.y
    }

    /// Is the point inside or on the boundary?
    pub fn contains(&self, p: Point) -> bool {
        self.min.x <= p.x && p.x <= self.max.x && self.min.y <= p.y && p.y <= self.max.y
    }

    /// Lower limit of the running coordinate for a segment of this orientation.
    pub fn lo(&self, orientation: Orientation) -> Coord {
        self.min.along(orientation)
    }

    /// Upper limit of the running coordinate for a segment of this orientation.
    pub fn hi(&self, orientation: Orientation) -> Coord {
        self.max.along(orientation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_uses_perpendicular_projection() {
        let s = Segment::horizontal(10, 0, 5);
        assert!(s.covers(Point::new(0, 3)));
        assert!(s.covers(Point::new(5, 30)));
        assert!(!s.covers(Point::new(6, 10)));
        let v = Segment::vertical(2, -1, 1);
        assert!(v.covers(Point::new(100, 0)));
        assert!(!v.covers(Point::new(2, 2)));
    }

    #[test]
    fn crossing_is_inclusive_and_handles_points() {
        let h = Segment::horizontal(0, 0, 10);
        let v = Segment::vertical(10, 0, 10);
        assert_eq!(h.crossing(&v), Some(Point::new(10, 0)));
        assert_eq!(v.crossing(&h), Some(Point::new(10, 0)));
        let far = Segment::vertical(11, -5, 5);
        assert_eq!(h.crossing(&far), None);
        let point = Segment::vertical(4, 0, 0);
        assert_eq!(h.crossing(&point), Some(Point::new(4, 0)));
        let point_off = Segment::vertical(4, 1, 1);
        assert_eq!(h.crossing(&point_off), None);
    }

    #[test]
    fn collinear_overlap_and_touch() {
        let a = Segment::horizontal(3, 0, 5);
        let b = Segment::horizontal(3, 5, 9);
        let c = Segment::horizontal(3, 6, 9);
        let d = Segment::horizontal(4, 0, 5);
        assert!(a.touches(&b));
        assert!(!a.touches(&c));
        assert!(!a.touches(&d));
        let p = Segment::horizontal(3, 2, 2);
        assert!(a.touches(&p));
    }

    #[test]
    fn between_and_endpoints() {
        let s = Segment::between(Point::new(5, 2), Point::new(1, 2)).unwrap();
        assert_eq!(s, Segment::horizontal(2, 1, 5));
        assert_eq!(s.start(), Point::new(1, 2));
        assert_eq!(s.end(), Point::new(5, 2));
        assert!(Segment::between(Point::new(0, 0), Point::new(1, 1)).is_none());
        assert_eq!(
            Segment::between(Point::new(0, 0), Point::new(0, 0))
                .unwrap()
                .len(),
            0
        );
    }
}
