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

/// Escape Process II at object point `z` (tree node `z_id`), after Figure 6
/// of the paper.
///
/// The four retreat positions start where `z`'s escape lines meet their
/// covers and walk back toward `z` one unit at a time, round-robin. At every
/// position `r_i` the perpendicular escape line through `r_i` is constructed
/// as a *trial* line: it is tested against the other network, and Process I
/// is tried at `r_i`. Only when `r_i` becomes an escape point does anything
/// stay behind; a trial line that led nowhere is forgotten and does **not**
/// count as "previously used" (the text says "enter the line in L" for the
/// object point's line in P1, but only "construct" here). Keeping failed
/// trial lines poisons the search and, for instance, makes the paper's own
/// Hampton Court maze unsolvable.
///
/// Ends on the bounding box have no cover and therefore no retreat position,
/// unless [`RouterConfig::boundary_retreat`] is set.
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
    // Orientation of the trial line through r[i]: perpendicular to the line it sits on.
    const NEW: [Orientation; 4] = [
        Orientation::Horizontal,
        Orientation::Vertical,
        Orientation::Horizontal,
        Orientation::Vertical,
    ];

    // A walker that has just stepped onto `z` still owes the paper's last
    // trial: Figure 6 constructs the escape line through `r_i` *after* moving
    // it, so the final position probed is `z` itself. There the trial line is
    // `z`'s own escape line perpendicular to the one the walker came along;
    // Process I at `z` has already failed (P3), only the intersection test is
    // new. Each orientation is tested at most once.
    let mut pending = [false; 4];
    let mut z_tested = [false; 2];

    loop {
        if r.iter().all(|&p| p == z) && !pending.iter().any(|&b| b) {
            return ProcessOutcome::Failed;
        }
        for i in 0..4 {
            if r[i] == z {
                if pending[i] && !z_tested[i % 2] {
                    pending[i] = false;
                    z_tested[i % 2] = true;
                    let line = obstacles.escape_line(z, NEW[i]);
                    if !net.is_used(&line) {
                        trace.push(TraceEvent::ProbeLine {
                            net: net.id,
                            line,
                            through: z,
                        });
                        if let Some((point, line_other)) = other.find_crossing(&line) {
                            let line_here = net.add_line(line, z_id, trace);
                            return ProcessOutcome::Intersection {
                                point,
                                line_here,
                                line_other,
                            };
                        }
                    }
                }
                pending[i] = false;
                continue;
            }
            let probe = obstacles.escape_line(r[i], NEW[i]);
            trace.push(TraceEvent::ProbeLine {
                net: net.id,
                line: probe,
                through: r[i],
            });
            if let Some((point, line_other)) = other.find_crossing(&probe) {
                let r_id = net.push_point(r[i], Some(z_id));
                let line_here = net.line_id_or_add(probe, r_id, trace);
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
                let r_id = net.push_point(r[i], Some(z_id));
                trace.push(TraceEvent::EscapePoint {
                    net: net.id,
                    point: r[i],
                    process: Process::II,
                });
                // `r_i` became an escape point, so its trial line stays in the
                // network like every other constructed escape line.
                net.line_id_or_add(probe, r_id, trace);
                net.push_point(e, Some(r_id));
                net.flag = Flag::One(o);
                trace.push(TraceEvent::EscapePoint {
                    net: net.id,
                    point: e,
                    process: Process::I,
                });
                return ProcessOutcome::Escaped;
            }
            r[i] = toward(r[i], z);
            pending[i] = r[i] == z;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Bounds;
    use crate::trace::NetId;

    fn p(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    /// Figure 6 walks `r_1` all the way onto `Z` and constructs the trial
    /// line there: `Z`'s horizontal escape line, which was never built
    /// because `Z`'s flag was VERTICAL. It is the only line that crosses the
    /// other network, so the paper ends with the intersect flag set.
    #[test]
    fn process_ii_probes_the_object_point_itself_last() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(20, 20)));
        o.add_segment(Segment::horizontal(13, 0, 20)); // bar above Z, wall to wall
        o.add_segment(Segment::horizontal(8, 14, 16)); // short covers that bound
        o.add_segment(Segment::horizontal(11, 14, 16)); // the other net's line
        let z = p(10, 10);
        let mut net = Network::new(NetId::A, z);
        let mut trace = Trace::default();
        net.add_line(o.escape_line(z, Orientation::Vertical), 0, &mut trace);
        net.flag = Flag::One(Orientation::Vertical);
        let mut other = Network::new(NetId::B, p(15, 10));
        let x15 = o.escape_line(p(15, 10), Orientation::Vertical);
        assert_eq!(x15, Segment::vertical(15, 9, 10));
        other.add_line(x15, 0, &mut trace);

        assert!(process_i(&o, &net, z).is_none());
        let config = RouterConfig::default();
        match process_ii(&o, &mut net, &other, 0, z, &config, &mut trace) {
            ProcessOutcome::Intersection {
                point,
                line_here,
                line_other,
            } => {
                assert_eq!(point, p(15, 10));
                assert_eq!(line_other, 0);
                assert_eq!(net.lines[line_here].segment, Segment::horizontal(10, 0, 20));
                assert_eq!(net.lines[line_here].through, 0);
            }
            _ => panic!("the trial line through Z must find the intersection"),
        }
        // the probes at (10,12) and (10,11) came first, Z's own line last
        let probes: Vec<Point> = trace
            .events
            .iter()
            .filter_map(|e| match e {
                TraceEvent::ProbeLine { through, .. } => Some(*through),
                _ => None,
            })
            .collect();
        assert_eq!(probes, vec![p(10, 12), p(10, 11), z]);
        // Z is not pushed a second time
        assert_eq!(net.points.len(), 1);
    }

    /// When `r_i` becomes an escape point (Process I succeeds there) the
    /// trial line constructed through it is a real escape line of the
    /// network: it is entered in `L` and counts as used from then on.
    #[test]
    fn process_ii_enters_the_trial_line_of_a_successful_retreat_position() {
        let mut o = ObstacleSet::new(Bounds::new(p(0, 0), p(20, 20)));
        o.add_segment(Segment::horizontal(13, 0, 20)); // bar above Z, wall to wall
        o.add_segment(Segment::vertical(14, 12, 20)); // covers only y >= 12
        let z = p(10, 10);
        let mut net = Network::new(NetId::A, z);
        let mut trace = Trace::default();
        net.add_line(o.escape_line(z, Orientation::Horizontal), 0, &mut trace);
        net.add_line(o.escape_line(z, Orientation::Vertical), 0, &mut trace);
        let other = Network::new(NetId::B, p(1, 1));

        assert!(process_i(&o, &net, z).is_none());
        let config = RouterConfig::default();
        assert!(matches!(
            process_ii(&o, &mut net, &other, 0, z, &config, &mut trace),
            ProcessOutcome::Escaped
        ));
        let pts: Vec<(Point, Option<usize>)> =
            net.points.iter().map(|e| (e.point, e.parent)).collect();
        assert_eq!(
            pts,
            vec![(z, None), (p(10, 12), Some(0)), (p(10, 11), Some(1))]
        );
        assert_eq!(net.flag, Flag::One(Orientation::Horizontal));
        let trial = Segment::horizontal(12, 0, 13);
        assert!(net.is_used(&trial));
        let line = net
            .lines
            .iter()
            .find(|l| l.segment == trial)
            .expect("trial line through r_1 is entered");
        assert_eq!(line.through, 1);
    }

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
