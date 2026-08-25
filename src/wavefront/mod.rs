//! Wavefront edges, vertices, and supporting lines.
//!
//! Upstream: `WavefrontEdge.*`, `WavefrontVertex.*`, `WavefrontSupportingLine.h`.

pub mod edge;
pub(crate) mod split_leftover;
pub mod supporting_line;
pub mod vertex;

pub use edge::{EdgeId, WavefrontEdge};
pub use supporting_line::SupportingLine;
pub use vertex::{AngleType, InfiniteSpeedType, VertexId, WavefrontVertex};
