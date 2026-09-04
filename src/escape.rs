//! Escape Process I ("slip around the end of a cover") and Escape Process II
//! ("retreat along the escape line").

use crate::geometry::{Orientation, Point, Segment};
use crate::obstacles::ObstacleSet;
use crate::router::{Flag, Network, RouterConfig};
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

/// The two ends of a cover together with the coordinate one unit beyond each
/// end, on the side away from the cover: `from - 1` for the start, `to + 1`
/// for the end. Because a cover of `z` spans `z`'s coordinate this is also
/// "away from `z`", including the tie case where an end sits exactly at `z`'s
/// coordinate (where a plain "away from z" rule would step *into* the cover).
pub(crate) fn beyond_ends(cover: &Segment) -> [(Point, i64); 2] {
    [(cover.start(), cover.from - 1), (cover.end(), cover.to + 1)]
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
    let mut ends: Vec<(Point, i64, Segment)> = Vec::with_capacity(4);
    for cover in [neg, pos].into_iter().flatten() {
        for (f, e_along) in beyond_ends(&cover) {
            ends.push((f, e_along, cover));
        }
    }
    ends.sort_by_key(|(f, _, _)| f.dist2(z));

    for (_, e_along, cover) in ends {
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
/// The ends of `z`'s escape lines that are bounded by a cover walk back toward
/// `z` one unit at a time, round-robin. Ends on the bounding box are skipped
/// as in the paper, unless [`RouterConfig::boundary_retreat`] is set. With
/// [`RouterConfig::recursive_retreat`] the retreat continues along the probe
/// lines themselves. At every position `r` whose perpendicular escape line is
/// still unused, that line is constructed and tested against the other
/// network; then Process I is tried *at `r`*. If it succeeds, `r` and the
/// Process I point both become escape points (in that order).
pub(crate) fn process_ii(
    obstacles: &ObstacleSet,
    net: &mut Network,
    other: &Network,
    z_id: usize,
    z: Point,
    config: &RouterConfig,
    trace: &mut Trace,
) -> ProcessOutcome {
    let v = obstacles.escape_line(z, Orientation::Vertical);
    let h = obstacles.escape_line(z, Orientation::Horizontal);
    let bounds = obstacles.bounds();
    // The paper defines r_i as the intersection of an escape line with the
    // cover that bounds it. An end that lies on the bounding box has no cover
    // and therefore no retreat position unless `boundary_retreat` is set; we
    // mark such ends as exhausted (== z).
    let end = |point: Point, on_boundary: bool| {
        if on_boundary && !config.boundary_retreat {
            z
        } else {
            point
        }
    };
    let mut r = [
        end(Point::new(z.x, v.to), v.to == bounds.max.y), // top end
        end(Point::new(h.to, z.y), h.to == bounds.max.x), // right end
        end(Point::new(z.x, v.from), v.from == bounds.min.y), // bottom end
        end(Point::new(h.from, z.y), h.from == bounds.min.x), // left end
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
                // Recursive reading of "try to find a Process I escape point
                // ... as outlined in the Escape Algorithm": retreat along the
                // new probe line as well. Terminates because every level
                // constructs at least one new line.
                if config.recursive_retreat {
                    match process_ii(obstacles, net, other, r_id, r[i], config, trace) {
                        ProcessOutcome::Failed => {}
                        outcome => {
                            trace.push(TraceEvent::EscapePoint {
                                net: net.id,
                                point: r[i],
                                process: Process::II,
                            });
                            return outcome;
                        }
                    }
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
    fn toward_helper() {
        let z = Point::new(5, 5);
        assert_eq!(toward(Point::new(9, 5), z), Point::new(8, 5));
        assert_eq!(toward(Point::new(5, 1), z), Point::new(5, 2));
        assert_eq!(toward(Point::new(5, 6), z), z);
    }

    #[test]
    fn beyond_ends_steps_outward_from_the_cover() {
        let cover = Segment::horizontal(12, 10, 20);
        assert_eq!(
            beyond_ends(&cover),
            [(Point::new(10, 12), 9), (Point::new(20, 12), 21)]
        );
        // a zero-length cover yields both neighbours
        let point = Segment::vertical(4, 7, 7);
        assert_eq!(
            beyond_ends(&point),
            [(Point::new(4, 7), 6), (Point::new(4, 7), 8)]
        );
    }
}
