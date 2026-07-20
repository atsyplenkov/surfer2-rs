//! Propagation statistics.
//!
//! Upstream counters around `CollapseType::NEVER` event_type_counter.

use crate::event::CollapseType;

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub events_processed: u64,
    pub triangle_collapses: u64,
    pub constraint_collapses: u64,
    pub spoke_collapses: u64,
    pub split_or_flip_refine: u64,
    pub vertex_moves_over_spoke: u64,
    pub inf_fast_opposing: u64,
    pub inf_fast_weighted: u64,
    pub ccw_leaves_ch: u64,
    pub flips: u64,
    pub exact_fallbacks: u64,
}

impl Stats {
    pub fn count_event(&mut self, t: CollapseType) {
        match t {
            CollapseType::TriangleCollapse => self.triangle_collapses += 1,
            CollapseType::ConstraintCollapse => self.constraint_collapses += 1,
            CollapseType::SpokeCollapse => self.spoke_collapses += 1,
            CollapseType::SplitOrFlipRefine => self.split_or_flip_refine += 1,
            CollapseType::VertexMovesOverSpoke => {
                self.vertex_moves_over_spoke += 1;
                self.flips += 1;
            }
            CollapseType::FaceHasInfinitelyFastVertexOpposing => self.inf_fast_opposing += 1,
            CollapseType::FaceHasInfinitelyFastVertexWeighted => self.inf_fast_weighted += 1,
            CollapseType::CcwVertexLeavesCh => self.ccw_leaves_ch += 1,
            _ => {}
        }
    }
}
