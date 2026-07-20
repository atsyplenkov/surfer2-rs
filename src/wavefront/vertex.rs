//! Kinetic wavefront vertices.
//!
//! Upstream: `surf/src/WavefrontVertex.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use slotmap::new_key_type;

use super::edge::EdgeId;
use crate::geometry::Point2;

new_key_type! { pub struct VertexId; }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfiniteSpeedType {
    None,
    Opposing,
    Weighted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleType {
    Acute,
    Right,
    Obtuse,
    Straight,
    Reflex,
}

#[derive(Debug, Clone)]
pub struct WavefrontVertex {
    pub pos_zero: Point2,
    pub velocity: Point2,
    pub infinite_speed: InfiniteSpeedType,
    /// Incident wavefront edges [left, right] in wavefront orientation.
    pub incident_edges: [Option<EdgeId>; 2],
    pub time_start: f64,
    pub time_stop: Option<f64>,
    pub pos_stop: Option<Point2>,
    /// Ancestry links for DCEL emission: next_vertex[side].
    pub next_vertex: [Option<VertexId>; 2],
    pub prev_vertex: [Option<VertexId>; 2],
    pub angle: AngleType,
    pub is_initial: bool,
    pub is_degenerate: bool,
    pub dead: bool,
    /// Source input vertex index when this is an initial kinetic vertex.
    pub source_input_vertex: Option<u32>,
}

impl WavefrontVertex {
    pub fn new_kinetic(
        pos_zero: Point2,
        velocity: Point2,
        time_start: f64,
        infinite_speed: InfiniteSpeedType,
    ) -> Self {
        Self {
            pos_zero,
            velocity,
            infinite_speed,
            incident_edges: [None, None],
            time_start,
            time_stop: None,
            pos_stop: None,
            next_vertex: [None, None],
            prev_vertex: [None, None],
            angle: AngleType::Acute,
            is_initial: false,
            is_degenerate: false,
            dead: false,
            source_input_vertex: None,
        }
    }

    pub fn position_at(&self, t: f64) -> Point2 {
        if let (Some(ts), Some(p)) = (self.time_stop, self.pos_stop) {
            if t >= ts {
                return p;
            }
        }
        Point2::new(
            self.pos_zero.x + t * self.velocity.x,
            self.pos_zero.y + t * self.velocity.y,
        )
    }

    pub fn stop(&mut self, t: f64, p: Point2) {
        self.time_stop = Some(t);
        self.pos_stop = Some(p);
    }

    pub fn is_stopped(&self) -> bool {
        self.time_stop.is_some()
    }

    #[inline]
    pub fn is_convex_or_straight(&self) -> bool {
        self.angle != AngleType::Reflex
    }

    #[inline]
    pub fn is_reflex_or_straight(&self) -> bool {
        matches!(self.angle, AngleType::Reflex | AngleType::Straight)
    }
}

/// Classify infinite speed at a straight sector between two supporting normals.
pub fn infinite_speed_type(
    normal_a: Point2,
    normal_b: Point2,
    weight_a: f64,
    weight_b: f64,
    angle: AngleType,
) -> InfiniteSpeedType {
    if angle != AngleType::Straight {
        return InfiniteSpeedType::None;
    }
    // Opposing if normals point roughly opposite
    let dot = normal_a.x * normal_b.x + normal_a.y * normal_b.y;
    if dot < 0.0 {
        InfiniteSpeedType::Opposing
    } else if (weight_a - weight_b).abs() > 0.0 {
        InfiniteSpeedType::Weighted
    } else {
        InfiniteSpeedType::None
    }
}

/// Angle type from three points a-b-c (at b), wavefront left-interior orientation.
pub fn angle_type_at(a: Point2, b: Point2, c: Point2) -> AngleType {
    let ux = a.x - b.x;
    let uy = a.y - b.y;
    let vx = c.x - b.x;
    let vy = c.y - b.y;
    let cross = ux * vy - uy * vx;
    let dot = ux * vx + uy * vy;
    let lu = ux.hypot(uy);
    let lv = vx.hypot(vy);
    if lu == 0.0 || lv == 0.0 {
        return AngleType::Straight;
    }
    // collinear?
    let scale = lu * lv;
    if cross.abs() <= 1e-14 * scale {
        // straight or 180
        if dot < 0.0 {
            return AngleType::Straight;
        }
        return AngleType::Reflex; // folded back
    }
    // Interior to the left of wavefront chain a→b→c means cross > 0 for CCW polygon left.
    // Reflex if interior angle > π, i.e. cross has the opposite sign of the polygon orientation.
    // For standard left-of-edge propagation (CCW rings), reflex when cross < 0.
    if cross < 0.0 {
        AngleType::Reflex
    } else if dot < 0.0 {
        AngleType::Obtuse
    } else if dot == 0.0 {
        AngleType::Right
    } else {
        AngleType::Acute
    }
}
