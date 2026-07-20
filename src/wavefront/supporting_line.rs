//! Weighted supporting line of a wavefront edge.
//!
//! Upstream: `surf/src/WavefrontSupportingLine.h` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use crate::geometry::Point2;

/// Line through two input points with multiplicative weight speed.
///
/// Normal points left of directed edge (u → v) and has length `weight`
/// (unit normal × weight). Offset at time `t` is translation by `t * normal`.
#[derive(Debug, Clone, Copy)]
pub struct SupportingLine {
    /// Point on the line at t=0.
    pub point: Point2,
    /// Direction u → v (not necessarily unit).
    pub direction: Point2,
    pub weight: f64,
    /// Unit normal (left of direction).
    pub normal_unit: Point2,
    /// `normal_unit * weight` — velocity of the supporting line.
    pub normal: Point2,
}

impl SupportingLine {
    pub fn from_segment(u: Point2, v: Point2, weight: f64) -> Option<Self> {
        let direction = Point2::new(v.x - u.x, v.y - u.y);
        let len = direction.x.hypot(direction.y);
        if !(len > 0.0 && weight.is_finite() && weight > 0.0) {
            return None;
        }
        // Left normal: (-dy, dx) / len
        let normal_unit = Point2::new(-direction.y / len, direction.x / len);
        let normal = Point2::new(normal_unit.x * weight, normal_unit.y * weight);
        Some(Self {
            point: u,
            direction,
            weight,
            normal_unit,
            normal,
        })
    }

    /// Line origin at time t (point translated by t*normal); direction unchanged.
    pub fn point_at(self, t: f64) -> Point2 {
        Point2::new(
            self.point.x + t * self.normal.x,
            self.point.y + t * self.normal.y,
        )
    }

    /// Evaluate signed distance orientation of point vs line at time 0.
    pub fn orientation_point(self, p: Point2) -> f64 {
        // cross(direction, p - point)
        let dx = p.x - self.point.x;
        let dy = p.y - self.point.y;
        self.direction.x * dy - self.direction.y * dx
    }
}

/// Intersect two lines given as point+direction. Returns None if parallel.
pub fn intersect_lines(p0: Point2, d0: Point2, p1: Point2, d1: Point2) -> Option<Point2> {
    // p0 + s d0 = p1 + u d1
    // s d0x - u d1x = p1x - p0x
    // s d0y - u d1y = p1y - p0y
    let det = d0.x * (-d1.y) - d0.y * (-d1.x);
    // det = -d0.x d1.y + d0.y d1.x = d0.y d1.x - d0.x d1.y
    let det = d0.x * d1.y - d0.y * d1.x;
    if det == 0.0 {
        return None;
    }
    let rhsx = p1.x - p0.x;
    let rhsy = p1.y - p0.y;
    let s = (rhsx * d1.y - rhsy * d1.x) / det;
    Some(Point2::new(p0.x + s * d0.x, p0.y + s * d0.y))
}

/// Velocity of the intersection of two supporting lines (offset to t=1 minus t=0).
pub fn intersection_velocity(a: SupportingLine, b: SupportingLine) -> Option<(Point2, Point2)> {
    let p0 = intersect_lines(a.point, a.direction, b.point, b.direction)?;
    let a1 = a.point_at(1.0);
    let b1 = b.point_at(1.0);
    let p1 = intersect_lines(a1, a.direction, b1, b.direction)?;
    let vel = Point2::new(p1.x - p0.x, p1.y - p0.y);
    Some((p0, vel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_square_edge_normal() {
        let s = SupportingLine::from_segment(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), 1.0)
            .unwrap();
        // Left of bottom edge is +y
        assert!((s.normal_unit.x).abs() < 1e-15);
        assert!((s.normal_unit.y - 1.0).abs() < 1e-15);
    }

    #[test]
    fn perpendicular_intersection() {
        let a = SupportingLine::from_segment(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), 1.0)
            .unwrap();
        let b = SupportingLine::from_segment(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0), 1.0)
            .unwrap();
        let (p, v) = intersection_velocity(a, b).unwrap();
        assert!((p.x).abs() < 1e-12 && (p.y).abs() < 1e-12);
        // Both normals: a→(0,1), b→(-1,0); intersection moves NW
        assert!(v.x < 0.0 && v.y > 0.0);
    }
}
