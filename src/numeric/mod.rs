//! f64 polynomials for collapse-time computation.
//!
//! Upstream: `surf/src/cgaltools.*` / `compute_determinant`.

mod polynomial;

pub use polynomial::{determinant_poly_f64, solve_quadratic_f64, Poly2, QuadraticRoots};
