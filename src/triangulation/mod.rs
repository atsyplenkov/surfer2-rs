//! Static CDT and kinetic triangulation.
//!
//! Upstream: `BasicTriangulation.*`, `KineticTriangle.*`, `KineticTriangulation.*`.

pub(crate) mod around_vertex;
pub mod basic;
pub(crate) mod collapse;
pub(crate) mod construct;
pub mod kinetic;
pub(crate) mod refine;

pub use basic::{build_basic_triangulation, select_polygon_interior, BasicTriangulation};
pub(crate) use construct::initialize_kinetic;
pub use kinetic::{
    ccw, cw, ArcEnd, ArcRecord, KineticTriangle, KineticTriangulation, SkeletonFaceMeta, TriangleId,
};
// Wavefront types appear on public kinetic fields; re-export so `wavefront` can stay crate-private.
pub use crate::wavefront::{
    AngleType, EdgeId, InfiniteSpeedType, SupportingLine, VertexId, WavefrontEdge, WavefrontVertex,
};
