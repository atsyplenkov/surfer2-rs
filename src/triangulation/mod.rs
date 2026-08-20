//! Static CDT and kinetic triangulation.
//!
//! Upstream: `BasicTriangulation.*`, `KineticTriangle.*`, `KineticTriangulation.*`.

pub mod around_vertex;
pub mod basic;
pub mod collapse;
pub mod kinetic;
pub mod refine;

pub use basic::{build_basic_triangulation, select_polygon_interior, BasicTriangulation};
pub use collapse::{compute_collapse, compute_collapse_fast};
pub use kinetic::{
    ccw, cw, ArcEnd, ArcRecord, KineticTriangle, KineticTriangulation, SkeletonFaceMeta, TriangleId,
};
