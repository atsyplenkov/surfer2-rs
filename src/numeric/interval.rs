//! Outward-rounded floating-point intervals for filtered exact predicates.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Closed interval with outward `f64` rounding.
#[derive(Clone, Copy, PartialEq)]
pub struct Interval {
    lo: f64,
    hi: f64,
}

impl Interval {
    #[inline]
    pub fn new(lo: f64, hi: f64) -> Self {
        debug_assert!(lo <= hi || (lo.is_nan() || hi.is_nan()));
        Self { lo, hi }
    }

    #[inline]
    pub fn point(x: f64) -> Self {
        Self { lo: x, hi: x }
    }

    /// Exact point from an IEEE-754 value (dyadic rational).
    #[inline]
    pub fn from_f64_exact(x: f64) -> Self {
        Self::point(x)
    }

    #[inline]
    pub fn lo(self) -> f64 {
        self.lo
    }

    #[inline]
    pub fn hi(self) -> f64 {
        self.hi
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.lo.is_finite() && self.hi.is_finite()
    }

    #[inline]
    pub fn contains_zero(self) -> bool {
        self.lo <= 0.0 && self.hi >= 0.0
    }

    #[inline]
    pub fn definitely_positive(self) -> bool {
        self.lo > 0.0
    }

    #[inline]
    pub fn definitely_negative(self) -> bool {
        self.hi < 0.0
    }

    /// Sign when the interval excludes zero; `None` if indeterminate.
    #[inline]
    pub fn sign(self) -> Option<i8> {
        if self.definitely_positive() {
            Some(1)
        } else if self.definitely_negative() {
            Some(-1)
        } else if self.lo == 0.0 && self.hi == 0.0 {
            Some(0)
        } else {
            None
        }
    }

    /// Compare two intervals: `Some(-1|0|1)` when order is certain.
    pub fn partial_cmp_interval(self, other: Self) -> Option<std::cmp::Ordering> {
        if self.hi < other.lo {
            Some(std::cmp::Ordering::Less)
        } else if self.lo > other.hi {
            Some(std::cmp::Ordering::Greater)
        } else if self.lo == self.hi && other.lo == other.hi && self.lo == other.lo {
            Some(std::cmp::Ordering::Equal)
        } else {
            None
        }
    }

    #[inline]
    pub fn abs(self) -> Self {
        if self.lo >= 0.0 {
            self
        } else if self.hi <= 0.0 {
            Self::new(outward_neg(self.hi), outward_neg(self.lo)).swap_if_needed()
        } else {
            let m = (-self.lo).max(self.hi);
            Self::new(0.0, m)
        }
    }

    #[inline]
    pub fn sqrt(self) -> Self {
        // Domain clamp at 0 for geometric square roots of non-negative quantities.
        let lo = if self.lo <= 0.0 {
            0.0
        } else {
            outward_down(self.lo.sqrt())
        };
        let hi = if self.hi <= 0.0 {
            0.0
        } else {
            outward_up(self.hi.sqrt())
        };
        Self::new(lo, hi)
    }

    #[inline]
    fn swap_if_needed(self) -> Self {
        if self.lo <= self.hi {
            self
        } else {
            Self::new(self.hi, self.lo)
        }
    }

    pub fn hull(self, other: Self) -> Self {
        Self::new(self.lo.min(other.lo), self.hi.max(other.hi))
    }
}

impl fmt::Debug for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.lo, self.hi)
    }
}

impl Neg for Interval {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(outward_neg(self.hi), outward_neg(self.lo))
    }
}

impl Add for Interval {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(
            outward_down(self.lo + rhs.lo),
            outward_up(self.hi + rhs.hi),
        )
    }
}

impl Sub for Interval {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(
            outward_down(self.lo - rhs.hi),
            outward_up(self.hi - rhs.lo),
        )
    }
}

impl Mul for Interval {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let candidates = [
            self.lo * rhs.lo,
            self.lo * rhs.hi,
            self.hi * rhs.lo,
            self.hi * rhs.hi,
        ];
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for c in candidates {
            lo = lo.min(c);
            hi = hi.max(c);
        }
        Self::new(outward_down(lo), outward_up(hi))
    }
}

impl Div for Interval {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        // Caller must ensure rhs excludes zero for tight results.
        if rhs.contains_zero() {
            return Self::new(f64::NEG_INFINITY, f64::INFINITY);
        }
        let candidates = [
            self.lo / rhs.lo,
            self.lo / rhs.hi,
            self.hi / rhs.lo,
            self.hi / rhs.hi,
        ];
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for c in candidates {
            lo = lo.min(c);
            hi = hi.max(c);
        }
        Self::new(outward_down(lo), outward_up(hi))
    }
}

impl Add<f64> for Interval {
    type Output = Self;
    fn add(self, rhs: f64) -> Self {
        self + Interval::point(rhs)
    }
}

impl Mul<f64> for Interval {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        self * Interval::point(rhs)
    }
}

#[inline]
fn outward_up(x: f64) -> f64 {
    if x.is_finite() {
        x.next_up()
    } else {
        x
    }
}

#[inline]
fn outward_down(x: f64) -> f64 {
    if x.is_finite() {
        x.next_down()
    } else {
        x
    }
}

#[inline]
fn outward_neg(x: f64) -> f64 {
    // -x then outward away from zero relative to true -x.
    let y = -x;
    if y == 0.0 {
        0.0
    } else if x > 0.0 {
        // true -x is negative; outward is more negative → next_down
        outward_down(y)
    } else {
        outward_up(y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_outward() {
        let a = Interval::point(0.1);
        let b = Interval::point(0.2);
        let s = a + b;
        assert!(s.lo() <= 0.3 && s.hi() >= 0.3);
    }

    #[test]
    fn sign_definite() {
        assert_eq!(Interval::new(1.0, 2.0).sign(), Some(1));
        assert_eq!(Interval::new(-2.0, -1.0).sign(), Some(-1));
        assert_eq!(Interval::new(-1.0, 1.0).sign(), None);
    }
}
