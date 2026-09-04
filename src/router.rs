//! The two escape-line networks, the main procedure and one escape step.

use crate::escape::{ProcessOutcome, process_i, process_ii};
use crate::geometry::{Orientation, Point, Segment};
use crate::obstacles::ObstacleSet;
use crate::refine::{cleanup, improve_extension, improve_probe, reconstruct, validate_path};
use crate::trace::{NetId, Process, Trace, TraceEvent};

/// Orientation of the next escape line a network will construct.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Flag {
    /// Only at the root: construct both lines.
    Both,
    One(Orientation),
}

/// A node of a network's escape-point tree. Each point lies on an escape line
/// through its parent, so consecutive points share an `x` or a `y`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EscapePoint {
    pub point: Point,
    pub parent: Option<usize>,
}

/// A constructed escape line and the escape point it was drawn through.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EscapeLine {
    pub segment: Segment,
    pub through: usize,
}

/// One of the two growing networks (rooted at `A` or at `B`).
#[derive(Clone, Debug)]
pub(crate) struct Network {
    pub id: NetId,
    pub points: Vec<EscapePoint>,
    pub lines: Vec<EscapeLine>,
    pub flag: Flag,
    pub no_escape: bool,
}

impl Network {
    pub fn new(id: NetId, root: Point) -> Self {
        Network {
            id,
            points: vec![EscapePoint {
                point: root,
                parent: None,
            }],
            lines: Vec::new(),
            flag: Flag::Both,
            no_escape: false,
        }
    }

    /// The current object point `Z`: the escape point found last.
    pub fn object_point(&self) -> (usize, Point) {
        let id = self.points.len() - 1;
        (id, self.points[id].point)
    }

    /// Has an escape line with this orientation and fixed coordinate and an
    /// overlapping span already been constructed? (Since escape lines are
    /// maximal free intervals, overlapping means identical.)
    pub fn is_used(&self, candidate: &Segment) -> bool {
        self.lines
            .iter()
            .any(|l| l.segment.overlaps_collinear(candidate))
    }

    pub fn push_point(&mut self, point: Point, parent: Option<usize>) -> usize {
        self.points.push(EscapePoint { point, parent });
        self.points.len() - 1
    }

    pub fn add_line(&mut self, segment: Segment, through: usize, trace: &mut Trace) -> usize {
        self.lines.push(EscapeLine { segment, through });
        trace.push(TraceEvent::LineAdded {
            net: self.id,
            line: segment,
            through: self.points[through].point,
        });
        self.lines.len() - 1
    }

    /// Does `segment` cross any perpendicular line of this network?
    /// Returns the crossing point and the id of the crossed line.
    pub fn find_crossing(&self, segment: &Segment) -> Option<(Point, usize)> {
        self.lines
            .iter()
            .enumerate()
            .find_map(|(id, l)| segment.crossing(&l.segment).map(|x| (x, id)))
    }

    /// Corner candidates from `x` (which lies on `line`) back to the root.
    pub fn chain(&self, line: usize, x: Point) -> Vec<Point> {
        let mut pts = vec![x];
        let mut node = Some(self.lines[line].through);
        while let Some(id) = node {
            pts.push(self.points[id].point);
            node = self.points[id].parent;
        }
        pts
    }
}

/// Which path improvements to run after a path was found.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Improvement {
    /// Only the collinear cleanup; the path may contain visible detours.
    None,
    /// Cleanup plus the segment-extension shortcut (paper Fig. 10 → 11).
    #[default]
    ExtensionOnly,
    /// `ExtensionOnly` plus perpendicular probing along every segment
    /// (paper Fig. 8 → 9). Removes staircases, costs more time.
    Full,
}

/// Tunable parameters of [`route_with`].
#[derive(Clone, Debug)]
pub struct RouterConfig {
    /// Maximum number of escape steps (both networks together) before giving up.
    pub max_steps: usize,
    /// Post-processing of the found path.
    pub improve: Improvement,
    /// Let Escape Process II also retreat from escape-line ends that lie on
    /// the bounding box, not only from ends stopped by a cover.
    ///
    /// The paper only retreats from covers. That keeps the number of probe
    /// lines small in open scenes, but a network whose lines reach the box
    /// on all sides gives up immediately, which makes the router miss more
    /// paths. Enabling this finds more paths at the price of many probe
    /// lines in open scenes (up to one per unit of the bounding box).
    pub boundary_retreat: bool,
    /// Let Escape Process II recurse: when neither the retreat position nor
    /// Process I at it leads anywhere, retreat along the freshly constructed
    /// probe line as well.
    ///
    /// The paper's wording ("try to find a Process I escape point on either
    /// of the two escape lines as outlined in the Escape Algorithm") admits
    /// this reading, and it is what lets the router thread long corridors
    /// such as the Hampton Court maze, which the flat reading cannot.
    /// Termination is unaffected (every level constructs a new line); the
    /// number of lines grows only slightly (about 7 % on random room scenes).
    /// Default: `true`.
    pub recursive_retreat: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        RouterConfig {
            max_steps: 10_000,
            improve: Improvement::ExtensionOnly,
            boundary_retreat: false,
            recursive_retreat: true,
        }
    }
}

/// Why a routing run ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// A path was found.
    Found,
    /// Both networks ran out of escape points. Either no path exists or the
    /// algorithm missed it (it is not complete).
    NoEscape,
    /// [`RouterConfig::max_steps`] was exhausted.
    StepLimit,
    /// `A` or `B` lies outside the bounds or on an obstacle.
    InvalidInput,
}

/// Result of [`route_with`]: the path (if any), why the run ended and the full trace.
#[derive(Clone, Debug)]
pub struct RouteResult {
    /// Corner list from `A` to `B`; consecutive corners differ in exactly one coordinate.
    pub path: Option<Vec<Point>>,
    /// Why the run ended.
    pub outcome: Outcome,
    /// Number of escape steps performed.
    pub steps: usize,
    /// Everything that happened, for visualization.
    pub trace: Trace,
}

pub(crate) enum StepResult {
    Continue,
    NoEscape,
    Intersection {
        point: Point,
        line_here: usize,
        line_other: usize,
    },
}

/// Routes from `a` to `b` with the default configuration.
///
/// Returns the corner list (`a` first, `b` last) or `None` if no path was
/// found. Note that `None` does not prove that no path exists: the algorithm
/// is fast but not complete.
pub fn route(obstacles: &ObstacleSet, a: Point, b: Point) -> Option<Vec<Point>> {
    route_with(obstacles, a, b, &RouterConfig::default()).path
}

/// Routes from `a` to `b` and returns the full [`RouteResult`] including the trace.
pub fn route_with(
    obstacles: &ObstacleSet,
    a: Point,
    b: Point,
    config: &RouterConfig,
) -> RouteResult {
    let mut trace = Trace::default();
    if !obstacles.is_free_point(a) || !obstacles.is_free_point(b) {
        return RouteResult {
            path: None,
            outcome: Outcome::InvalidInput,
            steps: 0,
            trace,
        };
    }
    if a == b {
        trace.push(TraceEvent::RawPath { corners: vec![a] });
        return RouteResult {
            path: Some(vec![a]),
            outcome: Outcome::Found,
            steps: 0,
            trace,
        };
    }

    let mut nets = [Network::new(NetId::A, a), Network::new(NetId::B, b)];
    let mut current = 0usize;
    let mut steps = 0usize;
    loop {
        if nets[0].no_escape && nets[1].no_escape {
            return RouteResult {
                path: None,
                outcome: Outcome::NoEscape,
                steps,
                trace,
            };
        }
        if nets[current].no_escape {
            current ^= 1;
            continue;
        }
        if steps >= config.max_steps {
            return RouteResult {
                path: None,
                outcome: Outcome::StepLimit,
                steps,
                trace,
            };
        }
        steps += 1;

        let (net, other) = split_pair(&mut nets, current);
        if let StepResult::Intersection {
            point,
            line_here,
            line_other,
        } = escape_step(obstacles, net, other, config, &mut trace)
        {
            let (la, lb) = if current == 0 {
                (line_here, line_other)
            } else {
                (line_other, line_here)
            };
            trace.push(TraceEvent::Intersection {
                point,
                line_a: nets[0].lines[la].segment,
                line_b: nets[1].lines[lb].segment,
            });
            let path = finish_path(obstacles, &nets, la, lb, point, config, &mut trace);
            return RouteResult {
                path: Some(path),
                outcome: Outcome::Found,
                steps,
                trace,
            };
        }
        current ^= 1;
    }
}

fn split_pair(nets: &mut [Network; 2], current: usize) -> (&mut Network, &Network) {
    let (first, second) = nets.split_at_mut(1);
    if current == 0 {
        (&mut first[0], &second[0])
    } else {
        (&mut second[0], &first[0])
    }
}

fn finish_path(
    obstacles: &ObstacleSet,
    nets: &[Network; 2],
    la: usize,
    lb: usize,
    x: Point,
    config: &RouterConfig,
    trace: &mut Trace,
) -> Vec<Point> {
    let mut path = reconstruct(&nets[0], la, &nets[1], lb, x);
    cleanup(&mut path);
    trace.push(TraceEvent::RawPath {
        corners: path.clone(),
    });
    debug_assert!(
        validate_path(
            obstacles,
            nets[0].points[0].point,
            nets[1].points[0].point,
            &path
        )
        .is_ok()
    );

    if matches!(
        config.improve,
        Improvement::ExtensionOnly | Improvement::Full
    ) {
        if improve_extension(obstacles, &mut path) {
            trace.push(TraceEvent::Improved {
                corners: path.clone(),
            });
        }
        path.reverse();
        let changed = improve_extension(obstacles, &mut path);
        path.reverse();
        if changed {
            trace.push(TraceEvent::Improved {
                corners: path.clone(),
            });
        }
    }
    if config.improve == Improvement::Full {
        if improve_probe(obstacles, &mut path) {
            trace.push(TraceEvent::Improved {
                corners: path.clone(),
            });
        }
        path.reverse();
        let changed = improve_probe(obstacles, &mut path);
        path.reverse();
        if changed {
            trace.push(TraceEvent::Improved {
                corners: path.clone(),
            });
        }
    }
    debug_assert!(
        validate_path(
            obstacles,
            nets[0].points[0].point,
            nets[1].points[0].point,
            &path
        )
        .is_ok()
    );
    path
}

/// One escape step of `net` against `other` (paper: "Escape Algorithm").
pub(crate) fn escape_step(
    obstacles: &ObstacleSet,
    net: &mut Network,
    other: &Network,
    config: &RouterConfig,
    trace: &mut Trace,
) -> StepResult {
    let (z_id, z) = net.object_point();

    // P1/P2: construct the escape line(s) through Z and test for intersections.
    let orientations: &[Orientation] = match net.flag {
        Flag::Both => &[Orientation::Horizontal, Orientation::Vertical],
        Flag::One(Orientation::Horizontal) => &[Orientation::Horizontal],
        Flag::One(Orientation::Vertical) => &[Orientation::Vertical],
    };
    for &o in orientations {
        let line = obstacles.escape_line(z, o);
        if net.is_used(&line) {
            continue;
        }
        let line_here = net.add_line(line, z_id, trace);
        if let Some((point, line_other)) = other.find_crossing(&line) {
            return StepResult::Intersection {
                point,
                line_here,
                line_other,
            };
        }
    }

    // P3: find the next escape point.
    if let Some((e, o)) = process_i(obstacles, net, z) {
        net.push_point(e, Some(z_id));
        net.flag = Flag::One(o);
        trace.push(TraceEvent::EscapePoint {
            net: net.id,
            point: e,
            process: Process::I,
        });
        return StepResult::Continue;
    }
    match process_ii(obstacles, net, other, z_id, z, config, trace) {
        ProcessOutcome::Escaped => StepResult::Continue,
        ProcessOutcome::Intersection {
            point,
            line_here,
            line_other,
        } => StepResult::Intersection {
            point,
            line_here,
            line_other,
        },
        ProcessOutcome::Failed => {
            net.no_escape = true;
            trace.push(TraceEvent::NoEscape { net: net.id });
            StepResult::NoEscape
        }
    }
}
