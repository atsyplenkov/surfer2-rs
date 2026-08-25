//! Concrete 2D geometry types shared across the crate.
//!
//! Coordinates remain in the caller's projected frame as IEEE-754 `f64`.
//! No origin shift or decimal round-trip is performed.

use crate::error::InputError;

/// Finite 2D point with projected coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn from_array(xy: [f64; 2]) -> Self {
        Self { x: xy[0], y: xy[1] }
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    #[inline]
    pub fn distance_to(self, other: Point2) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx.hypot(dy)
    }

    /// Exact bit pattern for fixture round-trips (not decimal).
    #[inline]
    pub fn to_bits(self) -> [u64; 2] {
        [self.x.to_bits(), self.y.to_bits()]
    }

    #[inline]
    pub fn from_bits(bits: [u64; 2]) -> Self {
        Self {
            x: f64::from_bits(bits[0]),
            y: f64::from_bits(bits[1]),
        }
    }
}

impl From<[f64; 2]> for Point2 {
    fn from(xy: [f64; 2]) -> Self {
        Self::from_array(xy)
    }
}

/// Oriented segment used by public skeleton output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment2 {
    pub start: Point2,
    pub end: Point2,
}

impl Segment2 {
    #[inline]
    pub fn new(start: Point2, end: Point2) -> Self {
        Self { start, end }
    }

    #[inline]
    pub fn length(self) -> f64 {
        self.start.distance_to(self.end)
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.start.is_finite() && self.end.is_finite()
    }
}

/// Infinite ray used for unbounded skeleton arcs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray2 {
    pub origin: Point2,
    /// Direction need not be unit length; must be finite and non-zero.
    pub direction: Point2,
}

/// Robust orientation predicate on three static points (adaptive filter).
///
/// Returns a signed value whose sign is the orientation; magnitude is not a
/// geometric area guarantee under the robust crate's adaptive expansion.
#[inline]
pub fn orient2d(a: Point2, b: Point2, c: Point2) -> f64 {
    robust::orient2d(
        robust::Coord { x: a.x, y: a.y },
        robust::Coord { x: b.x, y: b.y },
        robust::Coord { x: c.x, y: c.y },
    )
}

#[inline]
pub fn orientation_sign(a: Point2, b: Point2, c: Point2) -> i8 {
    let o = orient2d(a, b, c);
    if o > 0.0 {
        1
    } else if o < 0.0 {
        -1
    } else {
        0
    }
}

/// Proper segment intersection test for open segments (shared endpoints allowed).
pub fn segments_properly_intersect(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> bool {
    let o1 = orientation_sign(a0, a1, b0);
    let o2 = orientation_sign(a0, a1, b1);
    let o3 = orientation_sign(b0, b1, a0);
    let o4 = orientation_sign(b0, b1, a1);
    if o1 == 0 || o2 == 0 || o3 == 0 || o4 == 0 {
        // Collinear / endpoint touching: not a proper crossing for PSLG validation.
        return false;
    }
    o1 != o2 && o3 != o4
}

/// Validate a coordinate is finite.
#[inline]
pub fn require_finite_point(p: Point2, index: usize) -> Result<(), InputError> {
    if p.is_finite() {
        Ok(())
    } else {
        Err(InputError::NonFiniteCoordinate { index })
    }
}

/// Axis-aligned bounding-box diagonal of `pts`.
///
/// Callers pass the input footprint (original PSLG vertices at t=0), not
/// skeleton stop positions or unbounded-hull extras. Non-finite points are
/// ignored. Returns 0 when no finite points remain. Multipliers (metre
/// thresholds, gap, runaway length, endpoint-union ε) live at the call site.
pub(crate) fn scene_extent(pts: impl Iterator<Item = Point2>) -> f64 {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for p in pts {
        if !p.is_finite() {
            continue;
        }
        any = true;
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    if !any {
        0.0
    } else {
        (max_x - min_x).hypot(max_y - min_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_roundtrip() {
        let p = Point2::new(1.0 / 3.0, -0.0);
        assert_eq!(Point2::from_bits(p.to_bits()), p);
    }

    #[test]
    fn orient_ccw() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(0.0, 1.0);
        assert!(orient2d(a, b, c) > 0.0);
    }
}
