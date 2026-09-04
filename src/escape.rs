//! Escape Process I ("slip around the end of a cover") and Escape Process II
//! ("retreat along the escape line").

use crate::geometry::{Orientation, Point, Segment};
use crate::obstacles::ObstacleSet;
use crate::router::{Flag, Network};
use crate::trace::{Process, Trace, TraceEvent};

/// Moves `p` one unit toward `z` along the axis in which they differ.
/// (They differ in at most one axis because `p` lies on an escape line of `z`.)
pub(crate) fn toward(p: Point, z: Point) -> Point {
    if p.x != z.x {
        Point::new(p.x + (z.x - p.x).signum(), p.y)
    } else {
        Point::new(p.x, p.y + (z.y - p.y).signum())
    }
}

/// The coordinate one unit beyond `f`, on the side away from `z`.
/// If `f == z` the positive direction is chosen.
pub(crate) fn away(f: i64, z: i64) -> i64 {
    if f >= z { f + 1 } else { f - 1 }
}

/// Escape Process I at object point `z`.
///
/// Returns the escape point and the orientation of the new escape line to be
/// constructed through it. Phase 1 tries to get past the horizontal covers of
/// `z` by moving sideways on its horizontal escape line and then vertically;
/// phase 2 is the symmetric case.
pub(crate) fn process_i(
    obstacles: &ObstacleSet,
    net: &Network,
    z: Point,
) -> Option<(Point, Orientation)> {
    process_i_phase(obstacles, net, z, Orientation::Vertical)
        .or_else(|| process_i_phase(obstacles, net, z, Orientation::Horizontal))
}

/// One phase of Process I: look for an escape point whose *new* escape line
/// has orientation `new`. Candidates lie on `z`'s escape line of the
/// perpendicular orientation, one unit beyond an end of a cover that bounds
/// `z`'s `new`-oriented escape line.
fn process_i_phase(
    obstacles: &ObstacleSet,
    net: &Network,
    z: Point,
    new: Orientation,
) -> Option<(Point, Orientation)> {
    let run = new.perpendicular();
    let run_line = obstacles.escape_line(z, run);
    let (neg, pos) = obstacles.bounding_covers(z, new);

    // Endpoints of the (up to two) covers, sorted by Euclidean distance to Z.
    let mut ends: Vec<(Point, Segment)> = Vec::with_capacity(4);
    for cover in [neg, pos].into_iter().flatten() {
        for f in cover.endpoints() {
            ends.push((f, cover));
        }
    }
    ends.sort_by_key(|(f, _)| f.dist2(z));

    for (f, cover) in ends {
        let e_along = away(f.along(run), z.along(run));
        let e = Point::from_along_across(run, e_along, z.across(run));
        if !run_line.covers(e) {
            continue; // not reachable on Z's escape line
        }
        let new_line = obstacles.escape_line(e, new);
        // The new line must reach past the cover that was blocking Z (D6).
        let past = if cover.fixed > z.along(new) {
            new_line.to > cover.fixed
        } else {
            new_line.from < cover.fixed
        };
        if !past || net.is_used(&new_line) {
            continue;
        }
        return Some((e, new));
    }
    None
}

/// What Process II achieved.
pub(crate) enum ProcessOutcome {
    /// A new object point was pushed and the orientation flag updated.
    Escaped,
    /// One of the probe lines crossed a line of the other network.
    Intersection {
        point: Point,
        line_here: usize,
        line_other: usize,
    },
    /// No escape point exists for this object point.
    Failed,
}

/// Escape Process II at object point `z` (tree node `z_id`).
///
/// The four ends of `z`'s escape lines walk back toward `z` one unit at a
/// time, round-robin. At every position `r` whose perpendicular escape line is
/// still unused, that line is constructed and tested against the other
/// network; then Process I is tried *at `r`*. If it succeeds, `r` and the
/// Process I point both become escape points (in that order).
pub(crate) fn process_ii(
    obstacles: &ObstacleSet,
    net: &mut Network,
    other: &Network,
    z_id: usize,
    z: Point,
    trace: &mut Trace,
) -> ProcessOutcome {
    let v = obstacles.escape_line(z, Orientation::Vertical);
    let h = obstacles.escape_line(z, Orientation::Horizontal);
    let mut r = [
        Point::new(z.x, v.to),   // top end
        Point::new(h.to, z.y),   // right end
        Point::new(z.x, v.from), // bottom end
        Point::new(h.from, z.y), // left end
    ];
    // Orientation of the new line through r[i]: perpendicular to the line it sits on.
    const NEW: [Orientation; 4] = [
        Orientation::Horizontal,
        Orientation::Vertical,
        Orientation::Horizontal,
        Orientation::Vertical,
    ];

    loop {
        if r.iter().all(|&p| p == z) {
            return ProcessOutcome::Failed;
        }
        for i in 0..4 {
            if r[i] == z {
                continue;
            }
            let probe = obstacles.escape_line(r[i], NEW[i]);
            if !net.is_used(&probe) {
                let r_id = net.push_point(r[i], Some(z_id));
                let line_here = net.add_line(probe, r_id, trace);
                if let Some((point, line_other)) = other.find_crossing(&probe) {
                    trace.push(TraceEvent::EscapePoint {
                        net: net.id,
                        point: r[i],
                        process: Process::II,
                    });
                    return ProcessOutcome::Intersection {
                        point,
                        line_here,
                        line_other,
                    };
                }
                if let Some((e, o)) = process_i(obstacles, net, r[i]) {
                    trace.push(TraceEvent::EscapePoint {
                        net: net.id,
                        point: r[i],
                        process: Process::II,
                    });
                    net.push_point(e, Some(r_id));
                    net.flag = Flag::One(o);
                    trace.push(TraceEvent::EscapePoint {
                        net: net.id,
                        point: e,
                        process: Process::I,
                    });
                    return ProcessOutcome::Escaped;
                }
            }
            r[i] = toward(r[i], z);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toward_and_away_helpers() {
        let z = Point::new(5, 5);
        assert_eq!(toward(Point::new(9, 5), z), Point::new(8, 5));
        assert_eq!(toward(Point::new(5, 1), z), Point::new(5, 2));
        assert_eq!(toward(Point::new(5, 6), z), z);
        assert_eq!(away(7, 5), 8);
        assert_eq!(away(2, 5), 1);
        assert_eq!(away(5, 5), 6);
    }
}
