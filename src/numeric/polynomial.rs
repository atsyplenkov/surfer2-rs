//! Linear and quadratic polynomials over kernel scalars.
//!
//! Upstream: `Polynomial_1` usage in `cgaltools.*` / `KineticTriangle::get_generic_collapse_time`.

use super::exact::{ExactError, ExactValue};
use super::interval::Interval;

/// Dense polynomial a0 + a1 t + a2 t^2 (degree ≤ 2) with f64 coefficients (fast kernel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Poly2 {
    pub c: [f64; 3],
}

impl Poly2 {
    pub fn constant(a0: f64) -> Self {
        Self { c: [a0, 0.0, 0.0] }
    }

    pub fn linear(a0: f64, a1: f64) -> Self {
        Self { c: [a0, a1, 0.0] }
    }

    pub fn quadratic(a0: f64, a1: f64, a2: f64) -> Self {
        Self { c: [a0, a1, a2] }
    }

    pub fn degree(self) -> u8 {
        if self.c[2] != 0.0 {
            2
        } else if self.c[1] != 0.0 {
            1
        } else {
            0
        }
    }

    pub fn eval(self, t: f64) -> f64 {
        self.c[0] + t * (self.c[1] + t * self.c[2])
    }

    pub fn derivative(self) -> Poly1 {
        Poly1 {
            c: [self.c[1], 2.0 * self.c[2]],
        }
    }
}

/// Linear polynomial a0 + a1 t.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Poly1 {
    pub c: [f64; 2],
}

impl Poly1 {
    pub fn new(a0: f64, a1: f64) -> Self {
        Self { c: [a0, a1] }
    }

    pub fn eval(self, t: f64) -> f64 {
        self.c[0] + t * self.c[1]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadraticRoots {
    pub count: u8,
    /// Ordered roots x0 ≤ x1 when count ≥ 1; repeated root when count==1 and is_square.
    pub x0: f64,
    pub x1: f64,
    pub is_square: bool,
}

/// Upstream `solve_quadratic`: radicand b²−4ac; ordered roots.
pub fn solve_quadratic_f64(a: f64, b: f64, c: f64) -> QuadraticRoots {
    // Expect degree 2: a != 0
    let rad = b * b - 4.0 * a * c;
    if rad < 0.0 {
        return QuadraticRoots {
            count: 0,
            x0: 0.0,
            x1: 0.0,
            is_square: false,
        };
    }
    let sqrt_rad = rad.sqrt();
    let two_a = 2.0 * a;
    let mut r0 = (-b - sqrt_rad) / two_a;
    let mut r1 = (-b + sqrt_rad) / two_a;
    if r0 > r1 {
        std::mem::swap(&mut r0, &mut r1);
    }
    if rad == 0.0 {
        QuadraticRoots {
            count: 1,
            x0: r0,
            x1: r0,
            is_square: true,
        }
    } else {
        QuadraticRoots {
            count: 2,
            x0: r0,
            x1: r1,
            is_square: false,
        }
    }
}

/// Exact quadratic coefficients.
#[derive(Clone, Debug)]
pub struct ExactPoly2 {
    pub c: [ExactValue; 3],
}

impl ExactPoly2 {
    pub fn eval(&self, t: &ExactValue) -> ExactValue {
        // a0 + t*(a1 + t*a2)
        self.c[0].add(&t.mul(&self.c[1].add(&t.mul(&self.c[2]))))
    }

    pub fn degree(&self) -> Result<u8, ExactError> {
        if self.c[2].sign()? != 0 {
            Ok(2)
        } else if self.c[1].sign()? != 0 {
            Ok(1)
        } else {
            Ok(0)
        }
    }
}

/// Determinant polynomial of three kinetic vertices with linear motions:
/// pos_i(t) = p_i + t * v_i.
///
/// Upstream `compute_determinant` on coordinate polynomials.
pub fn determinant_poly_f64(
    p0: [f64; 2],
    v0: [f64; 2],
    p1: [f64; 2],
    v1: [f64; 2],
    p2: [f64; 2],
    v2: [f64; 2],
) -> Poly2 {
    // det(t) = (x0 y1 + x1 y2 + x2 y0) - (y0 x1 + y1 x2 + y2 x0)
    // with xi = pi_x + t vi_x, yi = pi_y + t vi_y
    // Expand to quadratic.
    let (x0, y0) = (p0[0], p0[1]);
    let (x1, y1) = (p1[0], p1[1]);
    let (x2, y2) = (p2[0], p2[1]);
    let (u0, w0) = (v0[0], v0[1]);
    let (u1, w1) = (v1[0], v1[1]);
    let (u2, w2) = (v2[0], v2[1]);

    // Constant term: det of positions
    let c0 = (x0 * y1 + x1 * y2 + x2 * y0) - (y0 * x1 + y1 * x2 + y2 * x0);
    // t^1: each term replaces one factor with velocity
    let c1 = (u0 * y1 + x0 * w1 + u1 * y2 + x1 * w2 + u2 * y0 + x2 * w0)
        - (w0 * x1 + y0 * u1 + w1 * x2 + y1 * u2 + w2 * x0 + y2 * u0);
    // t^2: two velocities
    let c2 = (u0 * w1 + u1 * w2 + u2 * w0) - (w0 * u1 + w1 * u2 + w2 * u0);

    Poly2::quadratic(c0, c1, c2)
}

/// Same expansion as `determinant_poly_f64` with exact dyadic coefficients.
pub fn determinant_poly_exact(
    p0: [f64; 2],
    v0: [f64; 2],
    p1: [f64; 2],
    v1: [f64; 2],
    p2: [f64; 2],
    v2: [f64; 2],
) -> Result<ExactPoly2, ExactError> {
    let x = |p: [f64; 2], i: usize| ExactValue::from_f64(p[i]);
    let x0 = x(p0, 0)?;
    let y0 = x(p0, 1)?;
    let x1 = x(p1, 0)?;
    let y1 = x(p1, 1)?;
    let x2 = x(p2, 0)?;
    let y2 = x(p2, 1)?;
    let u0 = x(v0, 0)?;
    let w0 = x(v0, 1)?;
    let u1 = x(v1, 0)?;
    let w1 = x(v1, 1)?;
    let u2 = x(v2, 0)?;
    let w2 = x(v2, 1)?;
    let c0 = x0
        .mul(&y1)
        .add(&x1.mul(&y2))
        .add(&x2.mul(&y0))
        .sub(&y0.mul(&x1).add(&y1.mul(&x2)).add(&y2.mul(&x0)));
    let c1 = u0
        .mul(&y1)
        .add(&x0.mul(&w1))
        .add(&u1.mul(&y2))
        .add(&x1.mul(&w2))
        .add(&u2.mul(&y0))
        .add(&x2.mul(&w0))
        .sub(
            &w0.mul(&x1)
                .add(&y0.mul(&u1))
                .add(&w1.mul(&x2))
                .add(&y1.mul(&u2))
                .add(&w2.mul(&x0))
                .add(&y2.mul(&u0)),
        );
    let c2 = u0
        .mul(&w1)
        .add(&u1.mul(&w2))
        .add(&u2.mul(&w0))
        .sub(&w0.mul(&u1).add(&w1.mul(&u2)).add(&w2.mul(&u0)));
    Ok(ExactPoly2 { c: [c0, c1, c2] })
}

/// Interval enclosure of a Poly2 evaluation.
pub fn poly2_eval_interval(p: Poly2, t: Interval) -> Interval {
    let a0 = Interval::point(p.c[0]);
    let a1 = Interval::point(p.c[1]);
    let a2 = Interval::point(p.c[2]);
    a0 + t * (a1 + t * a2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_triangle_det_constant() {
        let p = determinant_poly_f64(
            [0.0, 0.0],
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 0.0],
            [0.0, 1.0],
            [0.0, 0.0],
        );
        assert_eq!(p.degree(), 0);
        assert!((p.c[0] - 1.0).abs() < 1e-15);
    }

    #[test]
    fn quadratic_roots_simple() {
        // t^2 - 1 = 0 → -1, 1
        let r = solve_quadratic_f64(1.0, 0.0, -1.0);
        assert_eq!(r.count, 2);
        assert!((r.x0 + 1.0).abs() < 1e-14);
        assert!((r.x1 - 1.0).abs() < 1e-14);
    }
}
