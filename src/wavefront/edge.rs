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
///
/// Uses absolute time from `pos_zero` + constant velocity, matching
/// `WavefrontEdge.cpp` (orientation of velocities, then projected 1D quotient).
pub fn compute_edge_collapse_f64(
    p0: Point2,
    v0: Point2,
    p1: Point2,
    v1: Point2,
    now: f64,
) -> EdgeCollapseSpec {
    // C++: orientation(v0, v1) of the two velocity vectors.
    let o = v0.x * v1.y - v0.y * v1.x;
    const EPS: f64 = 0.0;

    if o < EPS {
        // RIGHT_TURN or COLLINEAR
        if o < 0.0 {
            // moving apart (right turn of velocities)
            return EdgeCollapseSpec {
                type_: EdgeCollapseType::Past,
                time: now,
            };
        }
        // collinear velocities
        let dx = p1.x - p0.x;
        let dy = p1.y - p0.y;
        let sqdist = dx * dx + dy * dy;
        if sqdist == 0.0 {
            return EdgeCollapseSpec {
                type_: EdgeCollapseType::Always,
                time: now,
            };
        }
        return EdgeCollapseSpec {
            type_: EdgeCollapseType::Never,
            time: f64::INFINITY,
        };
    }

    // LEFT_TURN: endpoints approach along the edge.
    // time = d·d / ((v0-v1)·d) projected; use x unless vertical.
    let edge_vertical = (p1.x - p0.x).abs() == 0.0 && (v0.x - v1.x).abs() == 0.0;
    let (edge_delta, wfvs_delta) = if !edge_vertical {
        (p1.x - p0.x, v0.x - v1.x)
    } else {
        (p1.y - p0.y, v0.y - v1.y)
    };
    if wfvs_delta == 0.0 {
        return EdgeCollapseSpec {
            type_: EdgeCollapseType::Never,
            time: f64::INFINITY,
        };
    }
    let time = edge_delta / wfvs_delta;
    if time < now {
        return EdgeCollapseSpec {
            type_: EdgeCollapseType::Past,
            time: now,
        };
    }
    EdgeCollapseSpec {
        type_: EdgeCollapseType::Future,
        time,
    }
}
