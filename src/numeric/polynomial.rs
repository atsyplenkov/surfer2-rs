//! Linear and quadratic polynomials over kernel scalars.
//!
//! Upstream: `Polynomial_1` usage in `cgaltools.*` / `KineticTriangle::get_generic_collapse_time`.

/// Dense polynomial a0 + a1 t + a2 t^2 (degree ≤ 2) with f64 coefficients (fast kernel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Poly2 {
    pub c: [f64; 3],
}

impl Poly2 {
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

    /// First root at or after `now`. Degree 2 prefers x1 when the leading
    /// coefficient is negative, otherwise x0, then falls back to the later root.
    pub fn next_root_from(self, now: f64) -> Option<f64> {
        match self.degree() {
            0 => {
                if self.c[0] == 0.0 {
                    Some(now)
                } else {
                    None
                }
            }
            1 => {
                if self.c[1] == 0.0 {
                    None
                } else {
                    let tc = -self.c[0] / self.c[1];
                    if tc >= now {
                        Some(tc)
                    } else {
                        None
                    }
                }
            }
            2 => {
                let roots = solve_quadratic_f64(self.c[2], self.c[1], self.c[0]);
                if roots.count == 0 {
                    return None;
                }
                let cand = if self.c[2] < 0.0 { roots.x1 } else { roots.x0 };
                if cand >= now {
                    Some(cand)
                } else if roots.count == 2 && roots.x1 >= now {
                    Some(roots.x1)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// C++ `KineticTriangle::get_generic_collapse_time`: leading-sign rules for
    /// which root is a collapse (reject area-zero-and-increasing at `now`).
    pub fn generic_collapse_time(self, now: f64) -> Option<f64> {
        match self.degree() {
            0 => {
                if self.c[0] == 0.0 {
                    Some(now)
                } else {
                    None
                }
            }
            1 => {
                let lead = self.c[1];
                if lead == 0.0 {
                    return None;
                }
                let tc = -self.c[0] / lead;
                if tc == now {
                    // Zero now: only if decreasing (leading sign negative).
                    if lead < 0.0 {
                        Some(tc)
                    } else {
                        None
                    }
                } else if tc > now {
                    Some(tc)
                } else {
                    None
                }
            }
            2 => {
                let roots = solve_quadratic_f64(self.c[2], self.c[1], self.c[0]);
                if roots.count == 0 {
                    return None;
                }
                let lead = self.c[2];
                if lead < 0.0 {
                    // Prefer second root; must not be in the past.
                    if roots.x1 >= now {
                        Some(roots.x1)
                    } else {
                        None
                    }
                } else if roots.x0 >= now {
                    Some(roots.x0)
                } else {
                    // Lead > 0 and first root past: C++ rejects (no fallthrough to x1).
                    None
                }
            }
            _ => None,
        }
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
    let (x0, y0) = (p0[0], p0[1]);
    let (x1, y1) = (p1[0], p1[1]);
    let (x2, y2) = (p2[0], p2[1]);
    let (u0, w0) = (v0[0], v0[1]);
    let (u1, w1) = (v1[0], v1[1]);
    let (u2, w2) = (v2[0], v2[1]);

    let c0 = (x0 * y1 + x1 * y2 + x2 * y0) - (y0 * x1 + y1 * x2 + y2 * x0);
    let c1 = (u0 * y1 + x0 * w1 + u1 * y2 + x1 * w2 + u2 * y0 + x2 * w0)
        - (w0 * x1 + y0 * u1 + w1 * x2 + y1 * u2 + w2 * x0 + y2 * u0);
    let c2 = (u0 * w1 + u1 * w2 + u2 * w0) - (w0 * u1 + w1 * u2 + w2 * u0);

    Poly2::quadratic(c0, c1, c2)
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
