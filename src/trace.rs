//! Recording of everything the router does, for visualization and debugging.

use crate::geometry::{Point, Segment};

/// Which of the two networks an event belongs to: the one rooted at the start
/// point `A` or the one rooted at the target `B`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NetId {
    /// The network rooted at the start point.
    A,
    /// The network rooted at the target point.
    B,
}

impl NetId {
    /// The other network.
    pub const fn other(self) -> NetId {
        match self {
            NetId::A => NetId::B,
            NetId::B => NetId::A,
        }
    }
}

/// Which escape process found an escape point.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Process {
    /// Escape Process I: slip around the end of a cover.
    I,
    /// Escape Process II: retreat along the escape line and branch off.
    II,
}

/// One step of the algorithm, in chronological order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TraceEvent {
    /// A network constructed a new escape line through `through`.
    LineAdded {
        /// Owning network.
        net: NetId,
        /// The constructed line.
        line: Segment,
        /// The escape point the line was drawn through.
        through: Point,
    },
    /// Escape Process II constructed a trial line through a retreat position.
    /// Trial lines are tested for intersections and searched with Process I,
    /// but they are not entered in the network unless the position becomes an
    /// escape point.
    ProbeLine {
        /// Owning network.
        net: NetId,
        /// The trial line.
        line: Segment,
        /// The retreat position it was drawn through.
        through: Point,
    },
    /// A network accepted a new escape point (which becomes its object point).
    EscapePoint {
        /// Owning network.
        net: NetId,
        /// The new escape point.
        point: Point,
        /// Which process found it.
        process: Process,
    },
    /// A network could not find another escape point and gave up.
    NoEscape {
        /// The network that gave up.
        net: NetId,
    },
    /// A line of one network crossed a perpendicular line of the other.
    Intersection {
        /// The crossing point.
        point: Point,
        /// The crossing line of network `A`.
        line_a: Segment,
        /// The crossing line of network `B`.
        line_b: Segment,
    },
    /// The path read off the two escape-point trees, after collinear cleanup
    /// but before any improvement.
    RawPath {
        /// Corner list from `A` to `B`.
        corners: Vec<Point>,
    },
    /// The path after one improvement pass.
    Improved {
        /// Corner list from `A` to `B`.
        corners: Vec<Point>,
    },
}

/// The chronological list of [`TraceEvent`]s of one routing run.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Trace {
    /// The events in the order they happened.
    pub events: Vec<TraceEvent>,
}

impl Trace {
    /// Appends an event.
    pub fn push(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    /// Number of recorded events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// True if nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Number of trial lines Process II constructed (not entered in the networks).
    pub fn probe_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, TraceEvent::ProbeLine { .. }))
            .count()
    }

    /// Number of escape lines entered by both networks together.
    pub fn line_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, TraceEvent::LineAdded { .. }))
            .count()
    }

    /// The last path event (`Improved` if present, else `RawPath`).
    pub fn final_path(&self) -> Option<&[Point]> {
        self.events.iter().rev().find_map(|e| match e {
            TraceEvent::RawPath { corners } | TraceEvent::Improved { corners } => {
                Some(corners.as_slice())
            }
            _ => None,
        })
    }
}
