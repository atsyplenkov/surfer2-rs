//! Wavefront edge records.
//!
//! Upstream: `surf/src/WavefrontEdge.*` (rev c42f9a7).

use slotmap::new_key_type;

use super::supporting_line::SupportingLine;
use super::vertex::VertexId;
use crate::event::EdgeCollapseType;
use crate::geometry::Point2;
use crate::triangulation::kinetic::TriangleId;

new_key_type! { pub struct EdgeId; }

#[derive(Debug, Clone)]
pub struct WavefrontEdge {
    pub endpoints: [VertexId; 2],
    pub supporting_line: SupportingLine,
    pub incident_triangle: Option<TriangleId>,
    /// Skeleton face owning this edge's left side (input-edge provenance).
    pub skeleton_face: u32,
    pub source_input_edge: Option<u32>,
    pub is_initial: bool,
    pub is_beveling: bool,
    pub dead: bool,
}

impl WavefrontEdge {
    pub fn new(
        endpoints: [VertexId; 2],
        supporting_line: SupportingLine,
        skeleton_face: u32,
        source_input_edge: Option<u32>,
        is_beveling: bool,
    ) -> Self {
        Self {
            endpoints,
            supporting_line,
            incident_triangle: None,
            skeleton_face,
            source_input_edge,
            is_initial: !is_beveling,
            is_beveling,
            dead: false,
        }
    }
}

/// Edge collapse classification (fast kernel).
#[derive(Debug, Clone, Copy)]
pub struct EdgeCollapseSpec {
    pub type_: EdgeCollapseType,
    pub time: f64,
}

/// Upstream `WavefrontEdge::compute_collapse` (f64).
pub fn compute_edge_collapse_f64(
    p0: Point2,
    v0: Point2,
    p1: Point2,
    v1: Point2,
    now: f64,
) -> EdgeCollapseSpec {
    // Relative motion of endpoint 1 wrt 0 projected along the edge.
    let e = Point2::new(p1.x - p0.x, p1.y - p0.y);
    let rel_v = Point2::new(v1.x - v0.x, v1.y - v0.y);
    // Orientation of relative velocity vs edge: cross(e, rel_v)
    let cross = e.x * rel_v.y - e.y * rel_v.x;
    let e2 = e.x * e.x + e.y * e.y;
    if e2 == 0.0 {
        return EdgeCollapseSpec {
            type_: EdgeCollapseType::Always,
            time: now,
        };
    }
    // Approaching if endpoints move toward each other along edge.
    // Use 1D projection: length change rate = dot(rel_v, e_hat) * |e| sign
    let len = e2.sqrt();
    let closing = -(rel_v.x * e.x + rel_v.y * e.y) / len; // positive → shrinking

    // Also check perpendicular relative motion for collinearity of future
    if cross.abs() <= 1e-30 * (len * (rel_v.x.hypot(rel_v.y) + 1.0)) {
        // Parallel motion of edge endpoints in plane sense of orientation
        let dist = len;
        if dist == 0.0 {
            return EdgeCollapseSpec {
                type_: EdgeCollapseType::Always,
                time: now,
            };
        }
        if closing > 0.0 {
            // Should not happen if truly parallel orientation-zero and positive length —
            // fall through to FUTURE if closing.
        } else if closing < 0.0 {
            return EdgeCollapseSpec {
                type_: EdgeCollapseType::Past,
                time: now,
            };
        } else {
            return EdgeCollapseSpec {
                type_: EdgeCollapseType::Never,
                time: f64::INFINITY,
            };
        }
    }

    if closing <= 0.0 {
        return EdgeCollapseSpec {
            type_: EdgeCollapseType::Past,
            time: now,
        };
    }
    let t = now + len / closing;
    EdgeCollapseSpec {
        type_: EdgeCollapseType::Future,
        time: t,
    }
}
