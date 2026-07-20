//! Certified numeric layer: intervals, exact DAG values, polynomials.
//!
//! Upstream: `surf/src/cgaltools.*`, algebraic pieces of `WavefrontVertex.*`.
//!
//! - [`Fast`][crate::KernelMode::Fast]: `f64` arithmetic with adaptive
//!   orientation/incircle predicates where applicable.
//! - [`Robust`][crate::KernelMode::Robust]: outward `f64` intervals with lazy
//!   exact materialization via `algebraics`.

mod exact;
mod interval;
mod polynomial;

pub use exact::{ExactError, ExactValue};
pub use interval::Interval;
pub use polynomial::{determinant_poly_f64, solve_quadratic_f64, Poly1, Poly2, QuadraticRoots};
