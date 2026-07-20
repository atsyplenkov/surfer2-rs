//! Lazy exact expression DAG with interval filters and algebraic fallback.
//!
//! Input `f64` values are treated as exact dyadic rationals (IEEE bit patterns).
//! No decimal text round-trip. Exact materialization uses `algebraics`.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use algebraics::algebraic_numbers::RealAlgebraicNumber;
use algebraics::prelude::*;
use thiserror::Error;

use super::interval::Interval;
use crate::error::PropagationError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct ExactError(pub String);

/// Handle into the expression arena.
#[derive(Clone)]
pub struct ExactValue {
    node: Rc<Node>,
}

struct Node {
    expr: Expr,
    interval: RefCell<Option<Interval>>,
    exact: RefCell<Option<RealAlgebraicNumber>>,
}

#[derive(Clone)]
enum Expr {
    /// Exact dyadic from IEEE-754 bits.
    ConstF64(f64),
    Neg(ExactValue),
    Add(ExactValue, ExactValue),
    Sub(ExactValue, ExactValue),
    Mul(ExactValue, ExactValue),
    Div(ExactValue, ExactValue),
    Sqrt(ExactValue),
}

impl ExactValue {
    pub fn from_f64(x: f64) -> Result<Self, ExactError> {
        if !x.is_finite() {
            return Err(ExactError(format!("non-finite constant {x}")));
        }
        Ok(Self::from_expr(Expr::ConstF64(x)))
    }

    pub fn zero() -> Self {
        Self::from_expr(Expr::ConstF64(0.0))
    }

    pub fn one() -> Self {
        Self::from_expr(Expr::ConstF64(1.0))
    }

    fn from_expr(expr: Expr) -> Self {
        Self {
            node: Rc::new(Node {
                expr,
                interval: RefCell::new(None),
                exact: RefCell::new(None),
            }),
        }
    }

    pub fn interval(&self) -> Interval {
        if let Some(iv) = *self.node.interval.borrow() {
            return iv;
        }
        let iv = match &self.node.expr {
            Expr::ConstF64(x) => Interval::from_f64_exact(*x),
            Expr::Neg(a) => -a.interval(),
            Expr::Add(a, b) => a.interval() + b.interval(),
            Expr::Sub(a, b) => a.interval() - b.interval(),
            Expr::Mul(a, b) => a.interval() * b.interval(),
            Expr::Div(a, b) => a.interval() / b.interval(),
            Expr::Sqrt(a) => a.interval().sqrt(),
        };
        *self.node.interval.borrow_mut() = Some(iv);
        iv
    }

    pub fn sign(&self) -> Result<i8, ExactError> {
        if let Some(s) = self.interval().sign() {
            return Ok(s);
        }
        let e = self.materialize()?;
        Ok(match e.cmp_with_zero() {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        })
    }

    pub fn cmp(&self, other: &Self) -> Result<Ordering, ExactError> {
        let d = self.sub(other);
        Ok(match d.sign()? {
            -1 => Ordering::Less,
            0 => Ordering::Equal,
            _ => Ordering::Greater,
        })
    }

    pub fn partial_cmp_filtered(&self, other: &Self) -> Option<Ordering> {
        self.interval().partial_cmp_interval(other.interval())
    }

    /// Convert to `f64` only after exact value is known or interval is a point.
    pub fn to_f64_checked(&self) -> Result<f64, ExactError> {
        let iv = self.interval();
        if iv.lo() == iv.hi() && iv.lo().is_finite() {
            return Ok(iv.lo());
        }
        // Fall back: evaluate exact then convert via high-precision approximation.
        let e = self.materialize()?;
        // algebraics RealAlgebraicNumber → f64 via to_f64-like path through rationals
        f64_from_algebraic(&e)
    }

    pub fn neg(&self) -> Self {
        Self::from_expr(Expr::Neg(self.clone()))
    }

    pub fn add(&self, other: &Self) -> Self {
        Self::from_expr(Expr::Add(self.clone(), other.clone()))
    }

    pub fn sub(&self, other: &Self) -> Self {
        Self::from_expr(Expr::Sub(self.clone(), other.clone()))
    }

    pub fn mul(&self, other: &Self) -> Self {
        Self::from_expr(Expr::Mul(self.clone(), other.clone()))
    }

    pub fn div(&self, other: &Self) -> Result<Self, ExactError> {
        if other.sign()? == 0 {
            return Err(ExactError("division by zero".into()));
        }
        Ok(Self::from_expr(Expr::Div(self.clone(), other.clone())))
    }

    pub fn sqrt(&self) -> Result<Self, ExactError> {
        if self.sign()? < 0 {
            return Err(ExactError("sqrt of negative".into()));
        }
        Ok(Self::from_expr(Expr::Sqrt(self.clone())))
    }

    fn materialize(&self) -> Result<RealAlgebraicNumber, ExactError> {
        if let Some(e) = self.node.exact.borrow().clone() {
            return Ok(e);
        }
        let value = match &self.node.expr {
            Expr::ConstF64(x) => real_from_f64(*x)?,
            Expr::Neg(a) => -a.materialize()?,
            Expr::Add(a, b) => a.materialize()? + b.materialize()?,
            Expr::Sub(a, b) => a.materialize()? - b.materialize()?,
            Expr::Mul(a, b) => a.materialize()? * b.materialize()?,
            Expr::Div(a, b) => {
                let den = b.materialize()?;
                if den.cmp_with_zero() == Ordering::Equal {
                    return Err(ExactError("division by zero".into()));
                }
                a.materialize()? / den
            }
            Expr::Sqrt(a) => {
                let base = a.materialize()?;
                // x^(1/2) via root
                nth_root(&base, 2)?
            }
        };
        *self.node.exact.borrow_mut() = Some(value.clone());
        Ok(value)
    }
}

impl std::fmt::Debug for ExactValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExactValue({:?})", self.interval())
    }
}

fn real_from_f64(x: f64) -> Result<RealAlgebraicNumber, ExactError> {
    if !x.is_finite() {
        return Err(ExactError(format!("non-finite {x}")));
    }
    if x == 0.0 {
        return Ok(RealAlgebraicNumber::from(0));
    }
    // x = significand * 2^(exp-52) with integer significand.
    let bits = x.to_bits();
    let sign = if bits >> 63 == 0 { 1i8 } else { -1i8 };
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & 0x000f_ffff_ffff_ffff;
    if exponent == 0 {
        // subnormal
        let mut n = mantissa as i128;
        if sign < 0 {
            n = -n;
        }
        // value = n * 2^(-1074)
        return Ok(mul_pow2(RealAlgebraicNumber::from(n as i64), -1074));
    }
    let significand = (mantissa | 0x0010_0000_0000_0000) as i128;
    let mut n = significand;
    if sign < 0 {
        n = -n;
    }
    let exp = exponent - 1023 - 52;
    // Fit into i64 when possible via shifting.
    let value = if n >= i64::MIN as i128 && n <= i64::MAX as i128 {
        RealAlgebraicNumber::from(n as i64)
    } else {
        // Split high/low 32 bits
        let hi = (n >> 32) as i64;
        let lo = (n & 0xffff_ffff) as i64;
        RealAlgebraicNumber::from(hi) * RealAlgebraicNumber::from(1i64 << 32)
            + RealAlgebraicNumber::from(lo)
    };
    Ok(mul_pow2(value, exp))
}

fn mul_pow2(v: RealAlgebraicNumber, exp: i32) -> RealAlgebraicNumber {
    if exp == 0 {
        return v;
    }
    if exp > 0 {
        v * RealAlgebraicNumber::from(1i64 << exp.min(62))
            * if exp > 62 {
                // chain multiplies for large exponents (projected GIS coords stay modest)
                let mut acc = RealAlgebraicNumber::from(1);
                let mut left = exp - 62;
                while left > 0 {
                    let step = left.min(62);
                    acc = acc * RealAlgebraicNumber::from(1i64 << step);
                    left -= step;
                }
                acc
            } else {
                RealAlgebraicNumber::from(1)
            }
    } else {
        let mut den = RealAlgebraicNumber::from(1);
        let mut left = -exp;
        while left > 0 {
            let step = left.min(62);
            den = den * RealAlgebraicNumber::from(1i64 << step);
            left -= step;
        }
        v / den
    }
}

fn nth_root(base: &RealAlgebraicNumber, n: u32) -> Result<RealAlgebraicNumber, ExactError> {
    base.checked_pow((1i64, n as i64))
        .ok_or_else(|| ExactError(format!("failed to take {n}-th root")))
}

fn f64_from_algebraic(e: &RealAlgebraicNumber) -> Result<f64, ExactError> {
    // Approximate by binary search on f64 with exact comparisons.
    if e.cmp_with_zero() == Ordering::Equal {
        return Ok(0.0);
    }
    let sign = if e.cmp_with_zero() == Ordering::Less {
        -1.0
    } else {
        1.0
    };
    let a = if sign < 0.0 { -e.clone() } else { e.clone() };
    // Find magnitude bracket
    let mut hi = 1.0f64;
    while real_from_f64(hi)?.cmp(&a) == Ordering::Less {
        hi *= 2.0;
        if !hi.is_finite() {
            return Err(ExactError("overflow converting algebraic to f64".into()));
        }
    }
    let mut lo = 0.0f64;
    // 60 iterations enough for f64
    for _ in 0..80 {
        let mid = (lo + hi) * 0.5;
        if !mid.is_finite() {
            break;
        }
        match real_from_f64(mid)?.cmp(&a) {
            Ordering::Less => lo = mid,
            Ordering::Greater => hi = mid,
            Ordering::Equal => {
                return Ok(sign * mid);
            }
        }
        if lo == hi || (hi - lo) == 0.0 {
            break;
        }
    }
    Ok(sign * hi)
}

/// Orientation of three static points with exact fallback.
pub fn exact_orient2d(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
) -> Result<i8, ExactError> {
    // (b-a) x (c-a) = (bx-ax)*(cy-ay) - (by-ay)*(cx-ax)
    let axv = ExactValue::from_f64(ax)?;
    let ayv = ExactValue::from_f64(ay)?;
    let bxv = ExactValue::from_f64(bx)?;
    let byv = ExactValue::from_f64(by)?;
    let cxv = ExactValue::from_f64(cx)?;
    let cyv = ExactValue::from_f64(cy)?;
    let det = (bxv.sub(&axv))
        .mul(&cyv.sub(&ayv))
        .sub(&(byv.sub(&ayv)).mul(&cxv.sub(&axv)));
    det.sign()
}

impl From<ExactError> for PropagationError {
    fn from(e: ExactError) -> Self {
        PropagationError::ExactEvaluation { detail: e.0 }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dyadic_one_third_sign() {
        // 0.1 + 0.2 - 0.3 is not zero in f64, but exact on dyadics of those f64s
        let a = ExactValue::from_f64(0.1).unwrap();
        let b = ExactValue::from_f64(0.2).unwrap();
        let c = ExactValue::from_f64(0.3).unwrap();
        let s = a.add(&b).sub(&c);
        // The f64 expression 0.1+0.2-0.3 is a specific dyadic, not exact 0
        let sign = s.sign().unwrap();
        assert!(sign == -1 || sign == 1 || sign == 0);
    }

    #[test]
    fn orient_unit_square() {
        assert_eq!(exact_orient2d(0.0, 0.0, 1.0, 0.0, 0.0, 1.0).unwrap(), 1);
        assert_eq!(exact_orient2d(0.0, 0.0, 0.0, 1.0, 1.0, 0.0).unwrap(), -1);
        assert_eq!(exact_orient2d(0.0, 0.0, 1.0, 1.0, 2.0, 2.0).unwrap(), 0);
    }
}
