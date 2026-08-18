//! Headless straight-skeleton engine: Rust port of cgalab/surfer2
//! (revision c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa).
//!
//! License: GPL-3.0-or-later. See `LICENSE` and `COPYRIGHT`.
//!
//! This crate translates Surfer2's kinetic triangulation / wavefront
//! propagation semantics. GUI, CLI, GraphML/OBJ serializers, Boost, and CGAL
//! are out of scope.



pub mod dcel;
pub mod error;
pub mod event;
pub mod geometry;
pub mod input;
pub mod numeric;
pub mod offset;
pub mod propagator;
pub mod stats;
pub mod triangulation;
pub mod wavefront;

pub use dcel::{
    ArcGeometry, Face, FaceId, HalfEdge, HalfEdgeId, Node, NodeId, Skeleton, SkeletonSegment,
};
pub use error::{InputError, PropagationError, SkeletonError, TriangulationError};
pub use geometry::{Point2, Ray2, Segment2};
pub use input::{InputEdge, Polygon, Pslg};
pub use offset::{offset, OffsetPolyline};
pub use propagator::Propagator;
pub use stats::Stats;

/// Which planar component(s) to propagate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentMode {
    /// Single component index (0 = left of first input edge, 1 = right, …).
    Index(usize),
    /// All components on separate time axes (upstream `-1`).
    AllSequential,
    /// All components sharing one time axis (upstream `-2`).
    AllSimultaneous,
}

impl Default for ComponentMode {
    fn default() -> Self {
        Self::AllSequential
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub components: ComponentMode,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            components: ComponentMode::AllSequential,
        }
    }
}

/// One-shot straight skeleton of a PSLG.
pub fn straight_skeleton(input: &Pslg, options: &Options) -> Result<Skeleton, SkeletonError> {
    let prop = Propagator::from_pslg(input, options)?;
    prop.into_skeleton()
}

/// Interior straight skeleton of a polygon with holes.
pub fn interior_straight_skeleton(polygon: &Polygon) -> Result<Skeleton, SkeletonError> {
    let prop = Propagator::from_polygon(polygon)?;
    prop.into_skeleton()
}
