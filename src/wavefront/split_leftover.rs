//! Leftover-face collapse after a split.
//!
//! C++ fires a second same-time event that stops the leftover constraint
//! endpoint. The override applies only to the two-constraint leftover;
//! three-constraint leftovers TriangleCollapse later, and one-constraint
//! input is not this policy.

use crate::event::CollapseType;
use crate::geometry::Point2;
use crate::wavefront::supporting_line::{intersect_lines, SupportingLine};

/// |sin θ| below this makes the 2×2 supporting-line intersection
/// condition ≥ 1e4. Angle test: wavefront weights do not change it.
const LINE_SIN_DEGENERATE: f64 = 1e-4;
/// Below this sine the lines do not define a usable intersection
/// (treated as missing). Also an angle test, not a speed cap.
const LINE_SIN_MISSING: f64 = 1e-6;

/// How a just-born split successor moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuccessorMotion {
    /// Well-conditioned intersection, including exact same-direction parallels
    /// that share a common normal.
    Finite,
    /// Supporting lines do not meet (opposing parallel / missing intersection).
    NonIntersecting,
    /// Intersection is non-finite or the supporting-line pair is poorly
    /// conditioned. Not a speed cap.
    Degenerate,
}

/// Whether the leftover face should keep its natural collapse or die now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftoverAction {
    LeaveNatural,
    CollapseNow,
    /// Override is defined only for two-constraint leftovers.
    Unsupported,
}

/// Scheduling plan before any `compute_collapse` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftoverSchedule {
    LeaveNatural,
    CollapseNow,
    /// Two-constraint degenerate: compute the natural collapse, then decide.
    ComputeNatural,
    Unsupported,
}

/// Classify successor motion from the two supporting lines of the child.
///
/// Geometry only: weights and the engine `1e6` velocity cap are not
/// consulted. Well-conditioned pairs (`|sin θ| ≥ 1e-4`) are `Finite`.
/// A finite (even huge) intersection of a poorly-conditioned pair is
/// `Degenerate`. No intersection, or a non-finite one, follows the
/// exact-parallel rule: same-direction `Finite`, opposing `NonIntersecting`.
pub fn classify_successor_motion(a: SupportingLine, b: SupportingLine) -> SuccessorMotion {
    let la = a.direction.x.hypot(a.direction.y);
    let lb = b.direction.x.hypot(b.direction.y);
    if !(la > 0.0 && lb > 0.0) {
        return SuccessorMotion::Degenerate;
    }
    let det = a.direction.x * b.direction.y - a.direction.y * b.direction.x;
    let sin_abs = det.abs() / (la * lb);
    if !sin_abs.is_finite() {
        return SuccessorMotion::Degenerate;
    }
    if sin_abs >= LINE_SIN_DEGENERATE {
        return SuccessorMotion::Finite;
    }
    let dot = a.direction.x * b.direction.x + a.direction.y * b.direction.y;
    match uncapped_intersection_velocity(a, b) {
        Some(vel) if vel.x == 0.0 && vel.y == 0.0 => SuccessorMotion::NonIntersecting,
        Some(_) if sin_abs < LINE_SIN_MISSING && dot <= 0.0 => SuccessorMotion::NonIntersecting,
        Some(_) => SuccessorMotion::Degenerate,
        None if dot > 0.0 => SuccessorMotion::Finite,
        None => SuccessorMotion::NonIntersecting,
    }
}

/// Intersection velocity with no speed cap. `None` if the lines do not
/// meet or the offset-1 intersection is non-finite.
fn uncapped_intersection_velocity(a: SupportingLine, b: SupportingLine) -> Option<Point2> {
    let p0 = intersect_lines(a.point, a.direction, b.point, b.direction)?;
    let p1 = intersect_lines(a.point_at(1.0), a.direction, b.point_at(1.0), b.direction)?;
    let vel = Point2::new(p1.x - p0.x, p1.y - p0.y);
    if vel.x.is_finite() && vel.y.is_finite() {
        Some(vel)
    } else {
        None
    }
}

/// Complete first-stage decision: no collapse, collapse now, or compute natural.
pub fn leftover_schedule(constraint_count: usize, motion: SuccessorMotion) -> LeftoverSchedule {
    match constraint_count {
        3 => LeftoverSchedule::LeaveNatural,
        2 => match motion {
            SuccessorMotion::Finite => LeftoverSchedule::LeaveNatural,
            SuccessorMotion::NonIntersecting => LeftoverSchedule::CollapseNow,
            SuccessorMotion::Degenerate => LeftoverSchedule::ComputeNatural,
        },
        _ => LeftoverSchedule::Unsupported,
    }
}

/// Decide whether to cache a same-time ConstraintCollapse on the leftover face.
pub fn leftover_action(
    constraint_count: usize,
    motion: SuccessorMotion,
    natural: CollapseType,
) -> LeftoverAction {
    match leftover_schedule(constraint_count, motion) {
        LeftoverSchedule::LeaveNatural => LeftoverAction::LeaveNatural,
        LeftoverSchedule::CollapseNow => LeftoverAction::CollapseNow,
        LeftoverSchedule::Unsupported => LeftoverAction::Unsupported,
        LeftoverSchedule::ComputeNatural if natural == CollapseType::Never => {
            LeftoverAction::CollapseNow
        }
        LeftoverSchedule::ComputeNatural => LeftoverAction::LeaveNatural,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point2;

    fn line(u: [f64; 2], v: [f64; 2], weight: f64) -> SupportingLine {
        SupportingLine::from_segment(Point2::from_array(u), Point2::from_array(v), weight).unwrap()
    }

    #[test]
    fn perpendicular_is_finite() {
        let a = line([0.0, 0.0], [1.0, 0.0], 1.0);
        let b = line([0.0, 0.0], [0.0, 1.0], 1.0);
        assert_eq!(classify_successor_motion(a, b), SuccessorMotion::Finite);
    }

    #[test]
    fn weighted_but_well_conditioned_is_finite() {
        let a = line([0.0, 0.0], [1.0, 0.0], 1e5);
        let b = line([0.0, 0.0], [0.0, 1.0], 1e5);
        assert_eq!(classify_successor_motion(a, b), SuccessorMotion::Finite);
    }

    #[test]
    fn weight_above_velocity_cap_stays_finite() {
        let a = line([0.0, 0.0], [1.0, 0.0], 1e7);
        let b = line([0.0, 0.0], [0.0, 1.0], 1e7);
        assert_eq!(classify_successor_motion(a, b), SuccessorMotion::Finite);
    }

    #[test]
    fn same_direction_parallel_is_finite() {
        let a = line([0.0, 0.0], [1.0, 0.0], 1.0);
        let b = line([0.0, 2.0], [1.0, 2.0], 1.0);
        assert_eq!(classify_successor_motion(a, b), SuccessorMotion::Finite);
    }

    #[test]
    fn opposing_parallel_is_non_intersecting() {
        let a = line([0.0, 0.0], [1.0, 0.0], 1.0);
        let b = line([1.0, 1.0], [0.0, 1.0], 1.0);
        assert_eq!(
            classify_successor_motion(a, b),
            SuccessorMotion::NonIntersecting
        );
    }

    #[test]
    fn nearly_parallel_is_degenerate() {
        let a = line([0.0, 0.0], [1.0, 0.0], 1.0);
        let b = line([0.0, 1.0], [1.0, 1.0 + 1e-5], 1.0);
        assert_eq!(classify_successor_motion(a, b), SuccessorMotion::Degenerate);
    }

    #[test]
    fn opposing_poorly_conditioned_is_weight_independent() {
        // |sin θ| ≈ 5e-5, opposing, finite intersection. Weight must not
        // flip Degenerate (unit) to NonIntersecting (via the 1e6 cap).
        for w in [1.0, 100.0] {
            let a = line([0.0, 0.0], [1.0, 0.0], w);
            let b = line([1.0, 1.0], [0.0, 1.0 + 5e-5], w);
            assert_eq!(
                classify_successor_motion(a, b),
                SuccessorMotion::Degenerate,
                "weight={w}"
            );
        }
        for w in [1.0, 100.0] {
            let a = line([0.0, 0.0], [1.0, 0.0], w);
            let b = line([1.0, 1.0], [0.0, 1.0 + 1e-7], w);
            assert_eq!(
                classify_successor_motion(a, b),
                SuccessorMotion::NonIntersecting,
                "missing-intersection weight={w}"
            );
        }
    }

    #[test]
    fn leftover_schedule_and_action_table() {
        use CollapseType::*;
        use LeftoverAction::*;
        use LeftoverSchedule as S;
        use SuccessorMotion::*;
        let cases = [
            (3, Finite, TriangleCollapse, S::LeaveNatural, LeaveNatural),
            (3, Degenerate, Never, S::LeaveNatural, LeaveNatural),
            (2, Degenerate, Never, S::ComputeNatural, CollapseNow),
            (
                2,
                NonIntersecting,
                ConstraintCollapse,
                S::CollapseNow,
                CollapseNow,
            ),
            (2, NonIntersecting, Never, S::CollapseNow, CollapseNow),
            (2, Finite, ConstraintCollapse, S::LeaveNatural, LeaveNatural),
            (2, Finite, Never, S::LeaveNatural, LeaveNatural),
            (
                2,
                Degenerate,
                ConstraintCollapse,
                S::ComputeNatural,
                LeaveNatural,
            ),
            (1, Degenerate, Never, S::Unsupported, Unsupported),
            (1, NonIntersecting, Never, S::Unsupported, Unsupported),
            (0, Finite, Never, S::Unsupported, Unsupported),
        ];
        for (n, motion, natural, want_sched, want_act) in cases {
            assert_eq!(
                leftover_schedule(n, motion),
                want_sched,
                "schedule nconst={n} motion={motion:?}"
            );
            assert_eq!(
                leftover_action(n, motion, natural),
                want_act,
                "action nconst={n} motion={motion:?} natural={natural:?}"
            );
        }
    }
}
