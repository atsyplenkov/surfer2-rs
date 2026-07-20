//! Collapse specification and priority order.
//!
//! Upstream: `surf/src/CollapseSpec.h` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.
//!
//! **Enum discriminant order IS the priority order** (lower = earlier).
//! `FACE_HAS_INFINITELY_FAST_VERTEX_WEIGHTED` sits after `SPLIT_OR_FLIP_REFINE`,
//! not next to the opposing infinite-speed case.

use std::cmp::Ordering;

/// Upstream `EdgeCollapseType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum EdgeCollapseType {
    Undefined = 1,
    Past,
    Future,
    Always,
    Never,
}

/// Upstream `CollapseType` — source order is priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i16)]
pub enum CollapseType {
    Undefined = 1,
    FaceHasInfinitelyFastVertexOpposing = 2,
    TriangleCollapse = 3,
    ConstraintCollapse = 4,
    SpokeCollapse = 5,
    SplitOrFlipRefine = 6,
    FaceHasInfinitelyFastVertexWeighted = 7,
    VertexMovesOverSpoke = 8,
    CcwVertexLeavesCh = 9,
    InvalidEvent = 10,
    Never = 11,
}

impl CollapseType {
    pub const ALL_COUNTED: [CollapseType; 9] = [
        CollapseType::FaceHasInfinitelyFastVertexOpposing,
        CollapseType::TriangleCollapse,
        CollapseType::ConstraintCollapse,
        CollapseType::SpokeCollapse,
        CollapseType::SplitOrFlipRefine,
        CollapseType::FaceHasInfinitelyFastVertexWeighted,
        CollapseType::VertexMovesOverSpoke,
        CollapseType::CcwVertexLeavesCh,
        CollapseType::InvalidEvent,
    ];

    #[inline]
    pub fn requires_relevant_edge(self) -> bool {
        matches!(
            self,
            CollapseType::FaceHasInfinitelyFastVertexWeighted
                | CollapseType::ConstraintCollapse
                | CollapseType::SpokeCollapse
                | CollapseType::SplitOrFlipRefine
                | CollapseType::VertexMovesOverSpoke
                | CollapseType::CcwVertexLeavesCh
        )
    }

    #[inline]
    pub fn requires_secondary_key(self) -> bool {
        matches!(
            self,
            CollapseType::FaceHasInfinitelyFastVertexWeighted
                | CollapseType::VertexMovesOverSpoke
        )
    }
}

impl PartialOrd for CollapseType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CollapseType {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as i16).cmp(&(*other as i16))
    }
}

/// Fast-kernel collapse key (f64 time). Robust mode uses a parallel exact key.
#[derive(Debug, Clone, Copy)]
pub struct CollapseSpec {
    pub component: i32,
    pub type_: CollapseType,
    pub time: f64,
    pub relevant_edge: i8,
    pub secondary_key: f64,
}

impl CollapseSpec {
    pub fn never(component: i32) -> Self {
        Self {
            component,
            type_: CollapseType::Never,
            time: f64::INFINITY,
            relevant_edge: -1,
            secondary_key: 0.0,
        }
    }

    pub fn undefined(component: i32) -> Self {
        Self {
            component,
            type_: CollapseType::Undefined,
            time: 0.0,
            relevant_edge: -1,
            secondary_key: 0.0,
        }
    }

    pub fn new(component: i32, type_: CollapseType, time: f64) -> Self {
        debug_assert!(!type_.requires_relevant_edge());
        Self {
            component,
            type_,
            time,
            relevant_edge: -1,
            secondary_key: 0.0,
        }
    }

    pub fn with_edge(component: i32, type_: CollapseType, time: f64, edge: i8) -> Self {
        debug_assert!(type_.requires_relevant_edge());
        debug_assert!(!type_.requires_secondary_key());
        debug_assert!((0..3).contains(&edge));
        Self {
            component,
            type_,
            time,
            relevant_edge: edge,
            secondary_key: 0.0,
        }
    }

    pub fn with_edge_secondary(
        component: i32,
        type_: CollapseType,
        time: f64,
        edge: i8,
        secondary_key: f64,
    ) -> Self {
        debug_assert!(type_.requires_relevant_edge());
        debug_assert!(type_.requires_secondary_key());
        debug_assert!((0..3).contains(&edge));
        Self {
            component,
            type_,
            time,
            relevant_edge: edge,
            secondary_key,
        }
    }

    /// Upstream compare: component ↑, time ↑, type ↑, secondary_key ↓.
    pub fn cmp_priority(&self, other: &Self) -> Ordering {
        debug_assert!(self.type_ != CollapseType::Undefined);
        debug_assert!(other.type_ != CollapseType::Undefined);

        if self.type_ == CollapseType::Never {
            return if other.type_ == CollapseType::Never {
                Ordering::Equal
            } else {
                Ordering::Greater
            };
        }
        if other.type_ == CollapseType::Never {
            return Ordering::Less;
        }

        match self.component.cmp(&other.component) {
            Ordering::Equal => {}
            o => return o,
        }

        match cmp_f64_exact(self.time, other.time) {
            Ordering::Equal => {}
            o => return o,
        }

        match self.type_.cmp(&other.type_) {
            Ordering::Equal => {}
            o => return o,
        }

        if self.type_.requires_secondary_key() {
            // Higher secondary_key is more important → reverse order.
            return cmp_f64_exact(other.secondary_key, self.secondary_key);
        }

        Ordering::Equal
    }

    pub fn allows_refinement_to(&self, other: &Self) -> bool {
        if self.time != other.time {
            return false;
        }
        if self.type_ == CollapseType::SplitOrFlipRefine
            && matches!(
                other.type_,
                CollapseType::VertexMovesOverSpoke | CollapseType::SpokeCollapse
            )
        {
            return self.relevant_edge != other.relevant_edge;
        }
        false
    }
}

impl PartialEq for CollapseSpec {
    fn eq(&self, other: &Self) -> bool {
        self.cmp_priority(other) == Ordering::Equal
    }
}

impl Eq for CollapseSpec {}

impl PartialOrd for CollapseSpec {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CollapseSpec {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_priority(other)
    }
}

/// Total order on f64 matching exact NT equality for finite values; NaN is largest.
fn cmp_f64_exact(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_is_largest() {
        let n = CollapseSpec::never(0);
        let t = CollapseSpec::new(0, CollapseType::TriangleCollapse, 1.0);
        assert!(t < n);
        assert_eq!(n.cmp(&CollapseSpec::never(0)), Ordering::Equal);
    }

    #[test]
    fn component_before_time() {
        let a = CollapseSpec::new(0, CollapseType::TriangleCollapse, 5.0);
        let b = CollapseSpec::new(1, CollapseType::TriangleCollapse, 0.0);
        assert!(a < b);
    }

    #[test]
    fn type_order_matches_upstream() {
        assert!(CollapseType::FaceHasInfinitelyFastVertexOpposing < CollapseType::TriangleCollapse);
        assert!(CollapseType::SplitOrFlipRefine < CollapseType::FaceHasInfinitelyFastVertexWeighted);
        assert!(CollapseType::VertexMovesOverSpoke < CollapseType::CcwVertexLeavesCh);
    }

    #[test]
    fn secondary_key_descending() {
        let a = CollapseSpec::with_edge_secondary(
            0,
            CollapseType::VertexMovesOverSpoke,
            1.0,
            0,
            10.0,
        );
        let b = CollapseSpec::with_edge_secondary(
            0,
            CollapseType::VertexMovesOverSpoke,
            1.0,
            0,
            3.0,
        );
        // Higher secondary key first
        assert!(a < b);
    }

    #[test]
    fn equal_events_remain_equal() {
        let a = CollapseSpec::new(0, CollapseType::TriangleCollapse, 1.0);
        let b = CollapseSpec::new(0, CollapseType::TriangleCollapse, 1.0);
        assert_eq!(a.cmp(&b), Ordering::Equal);
    }
}
