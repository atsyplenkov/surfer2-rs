//! Wavefront edges, vertices, and supporting lines.
//!
//! Upstream: `WavefrontEdge.*`, `WavefrontVertex.*`, `WavefrontSupportingLine.h`.

pub mod edge;
pub mod supporting_line;
pub mod vertex;

pub use edge::{compute_edge_collapse_f64, EdgeCollapseSpec, EdgeId, WavefrontEdge};
pub use supporting_line::{
    intersect_lines, intersection_velocity, wavefront_vertex_motion, SupportingLine,
};
pub use vertex::{
    angle_type_at, infinite_speed_type, AngleType, InfiniteSpeedType, VertexId, WavefrontVertex,
};
