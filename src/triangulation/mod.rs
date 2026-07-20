//! Static CDT and kinetic triangulation.
//!
//! Upstream: `BasicTriangulation.*`, `KineticTriangle.*`, `KineticTriangulation.*`.

pub mod around_vertex;
pub mod basic;
pub mod kinetic;

pub use basic::{build_basic_triangulation, select_polygon_interior, BasicTriangulation};
pub use kinetic::{
    compute_collapse_fast, cw, ccw, ArcRecord, KineticTriangle, KineticTriangulation, SkeletonFaceMeta,
    TriangleId,
};
