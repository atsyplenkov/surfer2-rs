//! Wavefront propagation driver.
//!
//! Upstream: `surf/src/SkeletonStructure.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use crate::dcel::{build_dcel, Skeleton};
use crate::error::{SkeletonError, TriangulationError};
use crate::event::{CollapseSpec, CollapseType, EventQueue};
use crate::geometry::Point2;
use crate::input::{Polygon, Pslg};
use crate::stats::Stats;
use crate::triangulation::basic::{build_basic_triangulation, select_polygon_interior};
use crate::triangulation::kinetic::{
    compute_collapse_fast, KineticTriangulation, SkeletonFaceMeta, TriangleId,
};
use crate::wavefront::edge::WavefrontEdge;
use crate::wavefront::supporting_line::{intersection_velocity, SupportingLine};
use crate::wavefront::vertex::{InfiniteSpeedType, WavefrontVertex};
use crate::{ComponentMode, KernelMode, Options};

/// Incremental propagator with step/advance_to/finish.
pub struct Propagator {
    pub kt: KineticTriangulation,
    pub queue: EventQueue,
    pub time: f64,
    pub kernel: KernelMode,
    pub stats: Stats,
    pub finalized: bool,
    /// When true, run topology invariants after every event (debug/tests).
    pub check_invariants_each_step: bool,
    simultaneous_streak: u32,
    last_event_time: f64,
}

impl Propagator {
    pub fn from_pslg(input: &Pslg, options: &Options) -> Result<Self, SkeletonError> {
        let basic = build_basic_triangulation(input)?;
        let restrict = match options.components {
            ComponentMode::Index(i) => i as i32,
            ComponentMode::AllSequential => -1,
            ComponentMode::AllSimultaneous => -2,
        };
        if let ComponentMode::Index(i) = options.components {
            let max = basic.max_component.unwrap_or(0) as usize;
            if i > max {
                return Err(TriangulationError::ComponentOutOfRange {
                    index: i,
                    count: max + 1,
                }
                .into());
            }
        }
        let mut kt = initialize_kinetic(&basic, input, restrict, None)?;
        let mut queue = EventQueue::with_capacity(kt.next_tidx as usize);
        seed_queue(&mut kt, &mut queue, 0.0)?;
        Ok(Self {
            kt,
            queue,
            time: 0.0,
            kernel: options.kernel,
            stats: Stats::default(),
            finalized: false,
            check_invariants_each_step: cfg!(debug_assertions),
            simultaneous_streak: 0,
            last_event_time: f64::NEG_INFINITY,
        })
    }

    pub fn from_polygon(polygon: &Polygon, kernel: KernelMode) -> Result<Self, SkeletonError> {
        let pslg = polygon.as_pslg()?;
        let basic = build_basic_triangulation(&pslg)?;
        let interior = select_polygon_interior(&basic)?;
        // Component of interior faces — restrict to that component id (usually 0 or 1).
        let comp = basic.components[interior[0]].unwrap_or(0);
        let mut kt = initialize_kinetic(&basic, &pslg, comp as i32, Some(&interior))?;
        let mut queue = EventQueue::with_capacity(kt.next_tidx as usize);
        seed_queue(&mut kt, &mut queue, 0.0)?;
        Ok(Self {
            kt,
            queue,
            time: 0.0,
            kernel,
            stats: Stats::default(),
            finalized: false,
            check_invariants_each_step: false,
            simultaneous_streak: 0,
            last_event_time: f64::NEG_INFINITY,
        })
            check_invariants_each_step: cfg!(debug_assertions),
    }

    pub fn current_time(&self) -> f64 {
        self.time
    }

    pub fn next_event_time(&self) -> Result<Option<f64>, SkeletonError> {
        if self.finalized {
            return Ok(None);
        }
        match self.queue.peak()? {
            Some(s) if s.type_ != CollapseType::Never => Ok(Some(s.time)),
            _ => Ok(None),
        }
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Process a single event. Returns false when finished.
    pub fn step(&mut self) -> Result<bool, SkeletonError> {
        if self.finalized {
            return Ok(false);
        }
        let (tidx, spec) = match (self.queue.peak_tidx()?, self.queue.peak()?) {
            (Some(t), Some(s)) if s.type_ != CollapseType::Never => (t, *s),
            _ => {
                self.finish()?;
                return Ok(false);
            }
        };

        // Watchdog for non-advancing fast kernel
        if spec.time <= self.last_event_time {
            self.simultaneous_streak += 1;
            if self.kernel == KernelMode::Fast && self.simultaneous_streak > 10_000 {
                return Err(crate::error::PropagationError::NonAdvancingLoop {
                    component: spec.component as usize,
                    time: spec.time,
                }
                .into());
            }
        } else {
            self.simultaneous_streak = 0;
            self.last_event_time = spec.time;
        }

        self.time = spec.time;
        self.stats.events_processed += 1;
        self.stats.count_event(spec.type_);

        // Absolute safety bound (independent of time advance).
        if self.stats.events_processed > 5_000_000 {
            return Err(crate::error::PropagationError::NonAdvancingLoop {
                component: spec.component as usize,
                time: spec.time,
            }
            .into());
        }
        self.handle_event(tidx, spec)?;

        let kt = &self.kt;
        self.queue
            .process_pending_updates(|tidx| recompute_collapse(kt, tidx, self.time))?;

        // M3/M4: optional topology invariants after every event.
        if self.check_invariants_each_step {
            self.kt.check_invariants(self.time)?;
        }

        if self.queue.is_empty() || self.queue.peak_is_never()? {
            self.finish()?;
            return Ok(false);
        }
        Ok(true)
    }

    pub fn advance_to(&mut self, t: f64) -> Result<(), SkeletonError> {
        while let Some(nt) = self.next_event_time()? {
            if nt > t {
                break;
            }
            if !self.step()? {
                break;
            }
        }
        self.time = self.time.max(t);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), SkeletonError> {
        if self.finalized {
            return Ok(());
        }
        // Stop remaining free vertices as rays
        let ids: Vec<_> = self.kt.vertices.keys().collect();
        for vid in ids {
            let stopped = self.kt.vertices[vid].is_stopped();
            let is_inf = self.kt.infinite_vertex == Some(vid);
            if !stopped && !is_inf {
                let v = &self.kt.vertices[vid];
                let start = v.position_at(v.time_start);
                let vel = v.velocity;
                let faces = [
                    v.incident_edges[0]
                        .and_then(|e| self.kt.edges.get(e).map(|ed| ed.skeleton_face))
                        .unwrap_or(0),
                    v.incident_edges[1]
                        .and_then(|e| self.kt.edges.get(e).map(|ed| ed.skeleton_face))
                        .unwrap_or(0),
                ];
                self.kt.arcs.push(crate::triangulation::kinetic::ArcRecord {
                    start,
                    end: None,
                    start_time: v.time_start,
                    end_time: None,
                    left_face: faces[0],
                    right_face: faces[1],
                    component: 0,
                    from_input_edge: false,
                });
                let _ = vel;
                if let Some(vv) = self.kt.vertices.get_mut(vid) {
                    vv.time_stop = Some(f64::INFINITY);
                }
            }
        }
        self.finalized = true;
        Ok(())
    }

    pub fn into_skeleton(mut self) -> Result<Skeleton, SkeletonError> {
        if !self.finalized {
            while self.step()? {}
            self.finish()?;
        }
        build_dcel(&self.kt)
    }

    fn handle_event(&mut self, tidx: u32, spec: CollapseSpec) -> Result<(), SkeletonError> {
        let tid = self
            .kt
            .triangle_by_tidx(tidx)
            .ok_or_else(|| crate::error::PropagationError::InconsistentEvent {
                triangle: tidx as u64,
                time: spec.time,
            })?;

        match spec.type_ {
            CollapseType::TriangleCollapse => self.handle_triangle_collapse(tid, spec)?,
            CollapseType::ConstraintCollapse => self.handle_constraint_collapse(tid, spec)?,
            CollapseType::SpokeCollapse => self.handle_spoke_collapse(tid, spec)?,
            CollapseType::SplitOrFlipRefine => self.handle_split_or_flip(tid, spec)?,
            CollapseType::VertexMovesOverSpoke => self.handle_flip(tid, spec)?,
            CollapseType::FaceHasInfinitelyFastVertexOpposing => {
                self.handle_inf_opposing(tid, spec)?
            }
            CollapseType::FaceHasInfinitelyFastVertexWeighted => {
                self.handle_inf_weighted(tid, spec)?
            }
            CollapseType::CcwVertexLeavesCh => self.handle_ccw_leaves(tid, spec)?,
            CollapseType::InvalidEvent | CollapseType::Undefined | CollapseType::Never => {
                return Err(crate::error::PropagationError::InconsistentEvent {
                    triangle: tidx as u64,
                    time: spec.time,
                }
                .into());
            }
        }
        Ok(())
    }

    fn handle_triangle_collapse(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.set_dying();
        }
        let time = spec.time;
        // Stop all three vertices first.
        let verts: [_; 3] = {
            let t = &self.kt.triangles[tid];
            [
                t.vertices[0].unwrap(),
                t.vertices[1].unwrap(),
                t.vertices[2].unwrap(),
            ]
        };
        for &v in &verts {
            self.kt.stop_vertex(v, time)?;
        }

        // Per side: constraint dies, or spoke → part2 on neighbor.
        for i in 0..3 {
            let (is_const, nb, eid) = {
                let t = &self.kt.triangles[tid];
                (
                    t.wavefronts[i].is_some(),
                    t.neighbors[i],
                    t.wavefronts[i],
                )
            };
            if is_const {
                if let Some(e) = eid {
                    if let Some(edge) = self.kt.edges.get_mut(e) {
                        edge.dead = true;
                    }
                }
            } else if let Some(nid) = nb {
                // Disconnect and run spoke part2 on neighbor.
                let idx_in_n = (0..3)
                    .find(|&s| self.kt.triangles[nid].neighbors[s] == Some(tid))
                    .ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
                        detail: "triangle collapse: neighbor missing backref".into(),
                    })?;
                if let Some(tr) = self.kt.triangles.get_mut(tid) {
                    tr.neighbors[i] = None;
                }
                if let Some(n) = self.kt.triangles.get_mut(nid) {
                    n.neighbors[idx_in_n] = None;
                    n.invalidate_collapse();
                }
                self.spoke_collapse_part2(nid, idx_in_n, time)?;
            }
            let _ = (ccw(i), cw(i));
        }

        self.queue.needs_dropping(self.kt.triangles[tid].tidx)?;
        if let Some(t) = self.kt.triangles.get_mut(tid) {
            t.set_dead();
        }
        Ok(())
    }

    fn handle_constraint_collapse(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};
        let edge_idx = spec.relevant_edge as usize;
        let (va, vb) = {
            let t = &self.kt.triangles[tid];
            (
                t.vertices[ccw(edge_idx)].unwrap(),
                t.vertices[cw(edge_idx)].unwrap(),
            )
        };
        self.kt.stop_vertex(va, spec.time)?;
        self.kt.stop_vertex(vb, spec.time)?;
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.set_dying();
        }
        move_constraints_to_neighbor(&mut self.kt, tid, edge_idx)?;
        self.constraint_collapse_part2(tid, edge_idx, spec.time)?;
        Ok(())
    }

    fn handle_spoke_collapse(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};
        let edge_idx = spec.relevant_edge as usize;
        let (va, vb, nb) = {
            let t = &self.kt.triangles[tid];
            (
                t.vertices[ccw(edge_idx)].unwrap(),
                t.vertices[cw(edge_idx)].unwrap(),
                t.neighbors[edge_idx],
            )
        };
        self.kt.stop_vertex(va, spec.time)?;
        self.kt.stop_vertex(vb, spec.time)?;

        let Some(nid) = nb else {
            // No neighbor: treat as constraint-style collapse on this triangle alone.
            if let Some(tr) = self.kt.triangles.get_mut(tid) {
                tr.set_dying();
            }
            return self.constraint_collapse_part2(tid, edge_idx, spec.time);
        };

        let idx_in_n = (0..3)
            .find(|&s| self.kt.triangles[nid].neighbors[s] == Some(tid))
            .ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
                detail: "spoke collapse: neighbor missing backref".into(),
            })?;

        // Disconnect across the spoke.
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.neighbors[edge_idx] = None;
        }
        if let Some(n) = self.kt.triangles.get_mut(nid) {
            n.neighbors[idx_in_n] = None;
            n.invalidate_collapse();
        }

        self.spoke_collapse_part2(tid, edge_idx, spec.time)?;
        self.spoke_collapse_part2(nid, idx_in_n, spec.time)?;
        Ok(())
    }

    /// Upstream `do_spoke_collapse_part2`.
    fn spoke_collapse_part2(
        &mut self,
        tid: TriangleId,
        edge_idx: usize,
        time: f64,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        if !self.kt.triangles[tid].live() && !self.kt.triangles[tid].is_dying {
            // already dead
            if self.kt.triangles[tid].is_dead {
                return Ok(());
            }
        }

        let (v_opp, va, vb) = {
            let t = &self.kt.triangles[tid];
            (
                t.vertices[edge_idx].unwrap(),
                t.vertices[ccw(edge_idx)].unwrap(),
                t.vertices[cw(edge_idx)].unwrap(),
            )
        };

        // If opposite vertex also meets collapse point, whole triangle dies.
        let posa = self.kt.vertices[va]
            .pos_stop
            .unwrap_or_else(|| self.kt.vertices[va].position_at(time));
        let opp_pos = self.kt.vertices[v_opp].position_at(time);
        let opp_meets = !self.kt.vertices[v_opp].is_stopped()
            && (opp_pos.x - posa.x).abs() <= 1e-12
            && (opp_pos.y - posa.y).abs() <= 1e-12;

        if opp_meets {
            if let Some(tr) = self.kt.triangles.get_mut(tid) {
                tr.set_dying();
            }
            self.kt.stop_vertex(v_opp, time)?;
            for i in 1..=2 {
                let edge = (edge_idx + i) % 3;
                let nb = self.kt.triangles[tid].neighbors[edge];
                if let Some(nid) = nb {
                    let idx_in_n = (0..3)
                        .find(|&s| self.kt.triangles[nid].neighbors[s] == Some(tid))
                        .ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
                            detail: "spoke part2 complete: backref".into(),
                        })?;
                    if let Some(tr) = self.kt.triangles.get_mut(tid) {
                        tr.neighbors[edge] = None;
                    }
                    if let Some(n) = self.kt.triangles.get_mut(nid) {
                        n.neighbors[idx_in_n] = None;
                        n.invalidate_collapse();
                    }
                    self.spoke_collapse_part2(nid, idx_in_n, time)?;
                } else if let Some(eid) = self.kt.triangles[tid].wavefronts[edge] {
                    if let Some(e) = self.kt.edges.get_mut(eid) {
                        e.dead = true;
                    }
                }
            }
            self.queue.needs_dropping(self.kt.triangles[tid].tidx)?;
            if let Some(t) = self.kt.triangles.get_mut(tid) {
                t.set_dead();
            }
            return Ok(());
        }

        // Partial: same as constraint collapse part2 on this triangle.
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            if !tr.is_dying {
                tr.set_dying();
            }
        }
        move_constraints_to_neighbor(&mut self.kt, tid, edge_idx)?;
        self.constraint_collapse_part2(tid, edge_idx, time)?;
        let _ = vb;
        Ok(())
    }

    /// Upstream `do_constraint_collapse_part2`: spawn successor, rewire fans, drop t.
    fn constraint_collapse_part2(
        &mut self,
        tid: TriangleId,
        edge_idx: usize,
        time: f64,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        let (va, vb, na_id, nb_id, collapsing_eid, left_sl, right_sl) = {
            let t = &self.kt.triangles[tid];
            let va = t.vertices[ccw(edge_idx)].unwrap();
            let vb = t.vertices[cw(edge_idx)].unwrap();
            let collapsing_eid = t.wavefronts[edge_idx];
            let wf_left = t.wavefronts[cw(edge_idx)];
            let wf_right = t.wavefronts[ccw(edge_idx)];
            let left_sl = wf_left.map(|e| self.kt.edges[e].supporting_line);
            let right_sl = wf_right.map(|e| self.kt.edges[e].supporting_line);
            (
                va,
                vb,
                t.neighbors[cw(edge_idx)],
                t.neighbors[ccw(edge_idx)],
                collapsing_eid,
                left_sl,
                right_sl,
            )
        };

        // Ensure stopped.
        if !self.kt.vertices[va].is_stopped() {
            self.kt.stop_vertex(va, time)?;
        }
        if !self.kt.vertices[vb].is_stopped() {
            self.kt.stop_vertex(vb, time)?;
        }
        let pos = self.kt.vertices[va]
            .pos_stop
            .unwrap_or_else(|| self.kt.vertices[va].position_at(time));

        let mut velocity = Point2::new(0.0, 0.0);
        let sl_pair = match (left_sl, right_sl) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => {
                let mut lines = Vec::new();
                for vid in [va, vb] {
                    for s in 0..2 {
                        if let Some(eid) = self.kt.vertices[vid].incident_edges[s] {
                            if Some(eid) != collapsing_eid && !self.kt.edges[eid].dead {
                                lines.push(self.kt.edges[eid].supporting_line);
                            }
                        }
                    }
                }
                if lines.len() >= 2 {
                    Some((lines[0], lines[1]))
                } else {
                    None
                }
            }
        };
        if let Some((a, b)) = sl_pair {
            if let Some((_, vel)) = intersection_velocity(a, b) {
                velocity = vel;
            }
        }

        let mut nv =
            WavefrontVertex::new_kinetic(pos, velocity, time, InfiniteSpeedType::None);
        for vid in [va, vb] {
            for s in 0..2 {
                if let Some(eid) = self.kt.vertices[vid].incident_edges[s] {
                    if Some(eid) != collapsing_eid && !self.kt.edges[eid].dead {
                        if nv.incident_edges[0].is_none() {
                            nv.incident_edges[0] = Some(eid);
                        } else if nv.incident_edges[1].is_none() {
                            nv.incident_edges[1] = Some(eid);
                        }
                    }
                }
            }
        }
        let nvid = self.kt.vertices.insert(nv);

        // Replace va/vb with nvid in all other live triangles.
        let live_ids: Vec<_> = self.kt.triangles.keys().collect();
        for oid in live_ids {
            if oid == tid {
                continue;
            }
            let t = match self.kt.triangles.get(oid) {
                Some(t) if t.live() || t.is_dying => t,
                _ => continue,
            };
            if t.is_dead {
                continue;
            }
            let mut changed = false;
            let mut verts = t.vertices;
            for i in 0..3 {
                if verts[i] == Some(va) || verts[i] == Some(vb) {
                    verts[i] = Some(nvid);
                    changed = true;
                }
            }
            if changed {
                if let Some(tm) = self.kt.triangles.get_mut(oid) {
                    tm.vertices = verts;
                    tm.invalidate_collapse();
                }
                for s in 0..3 {
                    if let Some(eid) = self.kt.triangles[oid].wavefronts[s] {
                        if let Some(e) = self.kt.edges.get_mut(eid) {
                            for ep in &mut e.endpoints {
                                if *ep == va || *ep == vb {
                                    *ep = nvid;
                                }
                            }
                        }
                    }
                }
                if self.kt.triangles[oid].live() {
                    self.queue.needs_update(self.kt.triangles[oid].tidx)?;
                }
            }
        }

        // Link neighbors across the dying triangle (excluding collapsed side).
        if let (Some(na), Some(nb)) = (na_id, nb_id) {
            let idx_a = (0..3).find(|&s| {
                self.kt.triangles[na].neighbors[s] == Some(tid)
                    || (self.kt.triangles[na].live()
                        && self.kt.triangles[na].neighbors[s] == Some(tid))
            });
            // Re-find after possible rewires
            let idx_a = (0..3).find(|&s| self.kt.triangles[na].neighbors[s] == Some(tid));
            let idx_b = (0..3).find(|&s| self.kt.triangles[nb].neighbors[s] == Some(tid));
            if let (Some(ia), Some(ib)) = (idx_a, idx_b) {
                self.kt.set_neighbor(na, ia, Some(nb), ib);
            }
            let _ = idx_a;
        } else if let Some(na) = na_id {
            if let Some(ia) = (0..3).find(|&s| self.kt.triangles[na].neighbors[s] == Some(tid)) {
                if let Some(tr) = self.kt.triangles.get_mut(na) {
                    tr.neighbors[ia] = None;
                    tr.invalidate_collapse();
                }
                if self.kt.triangles[na].live() {
                    self.queue.needs_update(self.kt.triangles[na].tidx)?;
                }
            }
        } else if let Some(nb) = nb_id {
            if let Some(ib) = (0..3).find(|&s| self.kt.triangles[nb].neighbors[s] == Some(tid)) {
                if let Some(tr) = self.kt.triangles.get_mut(nb) {
                    tr.neighbors[ib] = None;
                    tr.invalidate_collapse();
                }
                if self.kt.triangles[nb].live() {
                    self.queue.needs_update(self.kt.triangles[nb].tidx)?;
                }
            }
        }

        if let Some(eid) = collapsing_eid {
            if let Some(e) = self.kt.edges.get_mut(eid) {
                e.dead = true;
            }
        }

        self.queue.needs_dropping(self.kt.triangles[tid].tidx)?;
        if let Some(t) = self.kt.triangles.get_mut(tid) {
            t.set_dead();
        }
        // Clear neighbor back-pointers still pointing at tid
        let neighbors: Vec<_> = self.kt.triangles[tid].neighbors.iter().copied().collect();
        for nb in neighbors.into_iter().flatten() {
            if let Some(n) = self.kt.triangles.get_mut(nb) {
                for s2 in 0..3 {
                    if n.neighbors[s2] == Some(tid) {
                        n.neighbors[s2] = None;
                        n.invalidate_collapse();
                    }
                }
                if n.live() {
                    self.queue.needs_update(n.tidx)?;
                }
            }
        }
        Ok(())
    }

    fn handle_split_or_flip(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        let edge_idx = spec.relevant_edge as usize;
        let time = spec.time;
        let (v, va, vb) = {
            let t = &self.kt.triangles[tid];
            (
                t.vertices[edge_idx].unwrap(),
                t.vertices[ccw(edge_idx)].unwrap(),
                t.vertices[cw(edge_idx)].unwrap(),
            )
        };
        let pos = self.kt.vertices[v].position_at(time);
        let posa = self.kt.vertices[va].position_at(time);
        let posb = self.kt.vertices[vb].position_at(time);
        let d_va = pos.distance_to(posa);
        let d_vb = pos.distance_to(posb);

        if d_va == 0.0 {
            let refined = CollapseSpec::with_edge(
                spec.component,
                crate::event::CollapseType::SpokeCollapse,
                time,
                cw(edge_idx) as i8,
            );
            if let Some(t) = self.kt.triangles.get_mut(tid) {
                t.collapse_cache = Some(refined);
            }
            self.queue.needs_update(self.kt.triangles[tid].tidx)?;
            // Force immediate recompute by seeding
            self.queue.seed(self.kt.triangles[tid].tidx, refined);
            return Ok(());
        }
        if d_vb == 0.0 {
            let refined = CollapseSpec::with_edge(
                spec.component,
                crate::event::CollapseType::SpokeCollapse,
                time,
                ccw(edge_idx) as i8,
            );
            if let Some(t) = self.kt.triangles.get_mut(tid) {
                t.collapse_cache = Some(refined);
            }
            self.queue.seed(self.kt.triangles[tid].tidx, refined);
            return Ok(());
        }

        let abx = posb.x - posa.x;
        let aby = posb.y - posa.y;
        let apx = pos.x - posa.x;
        let apy = pos.y - posa.y;
        let ab2 = abx * abx + aby * aby;
        let tparam = if ab2 > 0.0 {
            (apx * abx + apy * aby) / ab2
        } else {
            0.5
        };
        if !(0.0..=1.0).contains(&tparam) {
            let flip_edge = if d_va > d_vb {
                cw(edge_idx)
            } else {
                ccw(edge_idx)
            };
            let longest = d_va.max(d_vb);
            let refined = CollapseSpec::with_edge_secondary(
                spec.component,
                crate::event::CollapseType::VertexMovesOverSpoke,
                time,
                flip_edge as i8,
                longest * longest,
            );
            if let Some(t) = self.kt.triangles.get_mut(tid) {
                t.collapse_cache = Some(refined);
            }
            self.queue.seed(self.kt.triangles[tid].tidx, refined);
            return Ok(());
        }

        self.handle_split_event(tid, edge_idx, time)
    }

    fn handle_split_event(
        &mut self,
        tid: TriangleId,
        edge_idx: usize,
        time: f64,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        let (v, eid, na_id, nb_id) = {
            let t = &self.kt.triangles[tid];
            (
                t.vertices[edge_idx].unwrap(),
                t.wavefronts[edge_idx].unwrap(),
                t.neighbors[cw(edge_idx)],
                t.neighbors[ccw(edge_idx)],
            )
        };
        self.kt.stop_vertex(v, time)?;
        let pos = self.kt.vertices[v].pos_stop.unwrap();
        let base_sl = self.kt.edges[eid].supporting_line;
        let face = self.kt.edges[eid].skeleton_face;
        let source = self.kt.edges[eid].source_input_edge;
        let ep0 = self.kt.edges[eid].endpoints[0];
        let ep1 = self.kt.edges[eid].endpoints[1];

        let nea = self.kt.edges.insert(WavefrontEdge::new(
            [ep0, v],
            base_sl,
            face,
            source,
            false,
        ));
        let neb = self.kt.edges.insert(WavefrontEdge::new(
            [v, ep1],
            base_sl,
            face,
            source,
            false,
        ));
        if let Some(e) = self.kt.edges.get_mut(eid) {
            e.dead = true;
        }

        let inc = self.kt.vertices[v].incident_edges;
        let mut nva =
            WavefrontVertex::new_kinetic(pos, Point2::new(0.0, 0.0), time, InfiniteSpeedType::None);
        let mut nvb =
            WavefrontVertex::new_kinetic(pos, Point2::new(0.0, 0.0), time, InfiniteSpeedType::None);
        nva.incident_edges = [Some(nea), inc[1]];
        nvb.incident_edges = [inc[0], Some(neb)];
        if let (Some(ea), Some(eb)) = (nva.incident_edges[0], nva.incident_edges[1]) {
            if let Some((_, vel)) = intersection_velocity(
                self.kt.edges[ea].supporting_line,
                self.kt.edges[eb].supporting_line,
            ) {
                nva.velocity = vel;
            }
        }
        if let (Some(ea), Some(eb)) = (nvb.incident_edges[0], nvb.incident_edges[1]) {
            if let Some((_, vel)) = intersection_velocity(
                self.kt.edges[ea].supporting_line,
                self.kt.edges[eb].supporting_line,
            ) {
                nvb.velocity = vel;
            }
        }
        let nva_id = self.kt.vertices.insert(nva);
        let nvb_id = self.kt.vertices.insert(nvb);
        if let Some(e) = self.kt.edges.get_mut(nea) {
            e.endpoints[1] = nva_id;
        }
        if let Some(e) = self.kt.edges.get_mut(neb) {
            e.endpoints[0] = nvb_id;
        }

        if let Some(na) = na_id {
            if let Some(i) = (0..3).find(|&s| self.kt.triangles[na].neighbors[s] == Some(tid)) {
                for s in 0..3 {
                    if self.kt.triangles[na].vertices[s] == Some(v) {
                        self.kt.set_vertex(na, s, nva_id);
                    }
                }
                self.kt.set_wavefront(na, i, nea);
                self.queue.needs_update(self.kt.triangles[na].tidx)?;
            }
        }
        if let Some(nb) = nb_id {
            if let Some(i) = (0..3).find(|&s| self.kt.triangles[nb].neighbors[s] == Some(tid)) {
                for s in 0..3 {
                    if self.kt.triangles[nb].vertices[s] == Some(v) {
                        self.kt.set_vertex(nb, s, nvb_id);
                    }
                }
                self.kt.set_wavefront(nb, i, neb);
                self.queue.needs_update(self.kt.triangles[nb].tidx)?;
            }
        }

        let live: Vec<_> = self.kt.triangles.keys().collect();
        for oid in live {
            if oid == tid || !self.kt.triangles[oid].live() {
                continue;
            }
            for s in 0..3 {
                if self.kt.triangles[oid].vertices[s] == Some(v) {
                    self.kt.set_vertex(oid, s, nva_id);
                    self.queue.needs_update(self.kt.triangles[oid].tidx)?;
                }
            }
        }

        self.kill_triangle(tid)?;
        Ok(())
    }

    fn handle_flip(&mut self, tid: TriangleId, spec: CollapseSpec) -> Result<(), SkeletonError> {
        let side = spec.relevant_edge as usize;
        let neighbor = self.kt.triangles[tid].neighbors[side];
        if neighbor.is_none() {
            // Nothing to flip — degrade to kill if degenerate
            self.kill_triangle(tid)?;
            return Ok(());
        }
        let ntid = neighbor.unwrap();
        // Raw flip of edge `side` between tid and ntid
        raw_flip(&mut self.kt, tid, side, ntid)?;
        self.queue.needs_update(self.kt.triangles[tid].tidx)?;
        self.queue.needs_update(self.kt.triangles[ntid].tidx)?;
        self.mark_adjacent_update(tid)?;
        self.mark_adjacent_update(ntid)?;
        Ok(())
    }

    fn handle_inf_opposing(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        let verts: Vec<_> = self.kt.triangles[tid]
            .vertices
            .iter()
            .flatten()
            .copied()
            .collect();
        for v in verts {
            self.kt.stop_vertex(v, spec.time)?;
        }
        self.kill_triangle(tid)?;
        Ok(())
    }

    fn handle_inf_weighted(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        self.handle_inf_opposing(tid, spec)
    }

    fn handle_ccw_leaves(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        self.handle_flip(tid, spec)
    }

    fn kill_triangle(&mut self, tid: TriangleId) -> Result<(), SkeletonError> {
        let tidx = {
            let t = self.kt.triangles.get_mut(tid).ok_or_else(|| {
                crate::error::PropagationError::TopologyInvariant {
                    detail: "kill missing triangle".into(),
                }
            })?;
            t.set_dying();
            t.tidx
        };
        // Clear neighbor back-pointers
        let neighbors: Vec<_> = self.kt.triangles[tid].neighbors.iter().copied().collect();
        for (s, nb) in neighbors.iter().enumerate() {
            if let Some(nid) = nb {
                if let Some(n) = self.kt.triangles.get_mut(*nid) {
                    for s2 in 0..3 {
                        if n.neighbors[s2] == Some(tid) {
                            n.neighbors[s2] = None;
                            // leave as hole — wavefront should be set by handlers in full port
                            n.invalidate_collapse();
                        }
                    }
                    self.queue.needs_update(n.tidx)?;
                }
            }
            let _ = s;
        }
        self.queue.needs_dropping(tidx)?;
        if let Some(t) = self.kt.triangles.get_mut(tid) {
            t.set_dead();
        }
        Ok(())
    }

    fn mark_adjacent_update(&mut self, tid: TriangleId) -> Result<(), SkeletonError> {
        let t = &self.kt.triangles[tid];
        for s in 0..3 {
            if let Some(nb) = t.neighbors[s] {
                if let Some(n) = self.kt.triangles.get(nb) {
                    if n.live() {
                        self.queue.needs_update(n.tidx)?;
                    }
                }
            }
        }
        if t.live() {
            self.queue.needs_update(t.tidx)?;
        }
        Ok(())
    }
}

fn recompute_collapse(
    kt: &KineticTriangulation,
    tidx: u32,
    now: f64,
) -> Result<CollapseSpec, crate::error::PropagationError> {
    let tid = kt.triangle_by_tidx(tidx).ok_or_else(|| {
        crate::error::PropagationError::InconsistentEvent {
            triangle: tidx as u64,
            time: now,
        }
    })?;
    let t = kt.triangles.get(tid).ok_or_else(|| {
        crate::error::PropagationError::InconsistentEvent {
            triangle: tidx as u64,
            time: now,
        }
    })?;
    if !t.live() {
        return Ok(CollapseSpec::never(t.component));
    }
    compute_collapse_fast(kt, t, now)
}

fn seed_queue(
    kt: &mut KineticTriangulation,
    queue: &mut EventQueue,
    now: f64,
) -> Result<(), SkeletonError> {
    let tidxs: Vec<(u32, TriangleId)> = kt
        .triangles
        .iter()
        .filter(|(_, t)| t.live())
        .map(|(id, t)| (t.tidx, id))
        .collect();
    for (tidx, tid) in tidxs {
        let spec = compute_collapse_fast(kt, &kt.triangles[tid], now)?;
        queue.seed(tidx, spec);
    }
    Ok(())
}

fn initialize_kinetic(
    basic: &crate::triangulation::basic::BasicTriangulation,
    input: &Pslg,
    restrict: i32,
    interior_filter: Option<&[usize]>,
) -> Result<KineticTriangulation, SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let mut kt = KineticTriangulation::new();
    kt.restrict_component = restrict;

    // One kinetic vertex per input vertex (bevels added later for degree-1).
    let mut input_vertex_ids = Vec::with_capacity(input.vertex_count());
    for (vi, p) in input.vertices().iter().enumerate() {
        let mut v = WavefrontVertex::new_kinetic(*p, Point2::new(0.0, 0.0), 0.0, InfiniteSpeedType::None);
        v.is_initial = true;
        v.source_input_vertex = Some(vi as u32);
        input_vertex_ids.push(kt.vertices.insert(v));
    }

    // Create triangles first (vertices only).
    let mut tri_map: Vec<Option<TriangleId>> = vec![None; basic.triangles.len()];
    for (ti, tri) in basic.triangles.iter().enumerate() {
        if let Some(filter) = interior_filter {
            if !filter.contains(&ti) {
                continue;
            }
        } else {
            let comp = basic.components[ti];
            if restrict >= 0 && comp != Some(restrict as u32) {
                continue;
            }
        }
        let component = if restrict == -2 {
            0
        } else {
            basic.components[ti].unwrap_or(0) as i32
        };
        let tid = kt.alloc_triangle(component);
        for i in 0..3 {
            kt.set_vertex(tid, i, input_vertex_ids[tri[i]]);
        }
        tri_map[ti] = Some(tid);
    }

    // Neighbors across unconstrained sides.
    for (ti, _) in basic.triangles.iter().enumerate() {
        let Some(tid) = tri_map[ti] else { continue };
        for s in 0..3 {
            if basic.constraints[ti][s].is_some() {
                continue;
            }
            if let Some(nb) = basic.neighbors[ti][s] {
                if let Some(ntid) = tri_map[nb] {
                    let mut side_b = 0;
                    for s2 in 0..3 {
                        if basic.neighbors[nb][s2] == Some(ti) {
                            side_b = s2;
                            break;
                        }
                    }
                    kt.set_neighbor(tid, s, Some(ntid), side_b);
                }
            }
        }
    }

    // Wavefront edges: one per constrained triangle side, oriented so the left
    // normal points into the triangle (toward the opposite vertex).
    let tids: Vec<_> = tri_map.iter().flatten().copied().collect();
    for &tid in &tids {
        for s in 0..3 {
            // Skip if already has neighbor (unconstrained) or wavefront.
            {
                let t = &kt.triangles[tid];
                if t.neighbors[s].is_some() || t.wavefronts[s].is_some() {
                    continue;
                }
            }
            // Determine if this side is constrained in the basic map
            let basic_ti = tri_map.iter().position(|x| *x == Some(tid));
            let source_edge = basic_ti.and_then(|ti| basic.constraints[ti][s]);
            let weight = source_edge
                .map(|ei| input.edges()[ei as usize].weight)
                .unwrap_or(1.0);

            let (va, vb, vc) = {
                let t = &kt.triangles[tid];
                (
                    t.vertices[ccw(s)].unwrap(),
                    t.vertices[cw(s)].unwrap(),
                    t.vertices[s].unwrap(),
                )
            };
            let mut pa = kt.vertices[va].pos_zero;
            let mut pb = kt.vertices[vb].pos_zero;
            let pc = kt.vertices[vc].pos_zero;
            // Ensure left-of pa→pb contains pc
            let cross = (pb.x - pa.x) * (pc.y - pa.y) - (pb.y - pa.y) * (pc.x - pa.x);
            let (ea, eb) = if cross >= 0.0 {
                (va, vb)
            } else {
                std::mem::swap(&mut pa, &mut pb);
                (vb, va)
            };
            let sl = SupportingLine::from_segment(pa, pb, weight).ok_or_else(|| {
                crate::error::PropagationError::TopologyInvariant {
                    detail: "bad supporting line".into(),
                }
            })?;
            let face = kt.faces.len() as u32;
            kt.faces.push(SkeletonFaceMeta {
                source_input_edge: source_edge,
                is_beveling: source_edge.is_none(),
                component: kt.triangles[tid].component,
            });
            let eid = kt.edges.insert(WavefrontEdge::new(
                [ea, eb],
                sl,
                face,
                source_edge,
                source_edge.is_none(),
            ));
            link_vertex_edge(&mut kt, ea, eid);
            link_vertex_edge(&mut kt, eb, eid);
            kt.set_wavefront(tid, s, eid);
        }
    }

    // Compute velocities: for each initial vertex, intersect its two incident
    // supporting lines (polygon case). Degree≠2 uses mean of incident normals.
    let vids: Vec<_> = input_vertex_ids.clone();
    for vid in vids {
        let edges: Vec<_> = {
            let v = &kt.vertices[vid];
            v.incident_edges.iter().flatten().copied().collect()
        };
        if edges.len() >= 2 {
            let sl0 = kt.edges[edges[0]].supporting_line;
            let sl1 = kt.edges[edges[1]].supporting_line;
            if let Some((pos, vel)) = intersection_velocity(sl0, sl1) {
                if let Some(v) = kt.vertices.get_mut(vid) {
                    // Keep geometric input position as pos_zero for skeleton start;
                    // velocity from offset-1 intersection.
                    let _ = pos;
                    v.velocity = vel;
                }
                continue;
            }
        }
        if let Some(eid) = edges.first() {
            let n = kt.edges[*eid].supporting_line.normal;
            if let Some(v) = kt.vertices.get_mut(vid) {
                v.velocity = n;
            }
        }
    }

    Ok(kt)
}

fn estimate_vertex_velocity(
    _input: &Pslg,
    _incident: &[(usize, bool)],
    _p: Point2,
) -> Point2 {
    Point2::new(0.0, 0.0)
}

fn edge_sl(input: &Pslg, ei: usize) -> Option<SupportingLine> {
    let e = &input.edges()[ei];
    let u = input.vertices()[e.vertices[0]];
    let v = input.vertices()[e.vertices[1]];
    SupportingLine::from_segment(u, v, e.weight)
}

/// Upstream `move_constraints_to_neighbor`: transfer non-collapsing constraints
/// from dying triangle `tid` (collapsing side `edge_idx`) onto neighbors.
fn move_constraints_to_neighbor(
    kt: &mut KineticTriangulation,
    tid: TriangleId,
    edge_idx: usize,
) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    // for i in {1,2}: if constrained(edge_idx+i), move to neighbor(edge_idx+3-i)
    for i in 1..=2usize {
        let src_side = (edge_idx + i) % 3;
        let nb_side_of_t = (edge_idx + 3 - i) % 3;
        let (wf, nb) = {
            let t = &kt.triangles[tid];
            (t.wavefronts[src_side], t.neighbors[nb_side_of_t])
        };
        let Some(eid) = wf else { continue };
        let Some(nid) = nb else { continue };

        // index of tid in neighbor
        let nidx = {
            let n = &kt.triangles[nid];
            (0..3)
                .find(|&s| n.neighbors[s] == Some(tid))
                .ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
                    detail: "move_constraint: neighbor missing backref".into(),
                })?
        };

        // n must not already be constrained on nidx
        if kt.triangles[nid].wavefronts[nidx].is_some() {
            return Err(crate::error::PropagationError::TopologyInvariant {
                detail: "move_constraint onto constrained side".into(),
            }
            .into());
        }

        // Transfer wavefront: set on neighbor, clear on source, fix endpoints to n's side verts
        {
            let n = &kt.triangles[nid];
            let v0 = n.vertices[ccw(nidx)].unwrap();
            let v1 = n.vertices[cw(nidx)].unwrap();
            if let Some(e) = kt.edges.get_mut(eid) {
                e.endpoints = [v0, v1];
                e.incident_triangle = Some(nid);
            }
        }
        {
            let n = kt.triangles.get_mut(nid).unwrap();
            n.wavefronts[nidx] = Some(eid);
            n.neighbors[nidx] = None;
            n.invalidate_collapse();
        }
        {
            let t = kt.triangles.get_mut(tid).unwrap();
            t.wavefronts[src_side] = None;
            t.invalidate_collapse();
        }
    }
    Ok(())
}

fn link_vertex_edge(kt: &mut KineticTriangulation, vid: crate::wavefront::vertex::VertexId, eid: crate::wavefront::edge::EdgeId) {
    if let Some(v) = kt.vertices.get_mut(vid) {
        if v.incident_edges[0].is_none() {
            v.incident_edges[0] = Some(eid);
        } else if v.incident_edges[1].is_none() {
            v.incident_edges[1] = Some(eid);
        }
    }
}

/// Flip edge `side` of `tid` shared with neighbor — upstream `do_raw_flip_inner`.
fn raw_flip(
    kt: &mut KineticTriangulation,
    tid: TriangleId,
    side: usize,
    ntid: TriangleId,
) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let nidx = {
        let n = kt.triangles.get(ntid).ok_or_else(|| {
            crate::error::PropagationError::TopologyInvariant {
                detail: "flip missing neighbor".into(),
            }
        })?;
        let mut found = None;
        for s in 0..3 {
            if n.neighbors[s] == Some(tid) {
                found = Some(s);
                break;
            }
        }
        found.ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
            detail: "flip: neighbor does not point back".into(),
        })?
    };

    let (v, o) = {
        let t = &kt.triangles[tid];
        let n = &kt.triangles[ntid];
        (t.vertices[side].unwrap(), n.vertices[nidx].unwrap())
    };

    // vertices[ccw(idx)] = o; n->vertices[ccw(nidx)] = v
    kt.set_vertex(tid, ccw(side), o);
    kt.set_vertex(ntid, ccw(nidx), v);

    let (t_nb_cw, t_wf_cw, n_nb_cw, n_wf_cw) = {
        let t = &kt.triangles[tid];
        let n = &kt.triangles[ntid];
        (
            t.neighbors[cw(side)],
            t.wavefronts[cw(side)],
            n.neighbors[cw(nidx)],
            n.wavefronts[cw(nidx)],
        )
    };

    {
        let t = kt.triangles.get_mut(tid).unwrap();
        t.neighbors[side] = n_nb_cw;
        t.wavefronts[side] = n_wf_cw;
        t.invalidate_collapse();
    }
    {
        let n = kt.triangles.get_mut(ntid).unwrap();
        n.neighbors[nidx] = t_nb_cw;
        n.wavefronts[nidx] = t_wf_cw;
        n.invalidate_collapse();
    }

    // pair t and n
    {
        let n = kt.triangles.get_mut(ntid).unwrap();
        n.neighbors[cw(nidx)] = Some(tid);
        n.wavefronts[cw(nidx)] = None;
        n.invalidate_collapse();
    }
    {
        let t = kt.triangles.get_mut(tid).unwrap();
        t.neighbors[cw(side)] = Some(ntid);
        t.wavefronts[cw(side)] = None;
        t.invalidate_collapse();
    }

    // outer back-pointers
    {
        let wf = kt.triangles[tid].wavefronts[side];
        let nb = kt.triangles[tid].neighbors[side];
        if let Some(eid) = wf {
            if let Some(e) = kt.edges.get_mut(eid) {
                e.incident_triangle = Some(tid);
            }
        } else if let Some(other_id) = nb {
            if let Some(other) = kt.triangles.get_mut(other_id) {
                for s in 0..3 {
                    if other.neighbors[s] == Some(ntid) {
                        other.neighbors[s] = Some(tid);
                        other.invalidate_collapse();
                        break;
                    }
                }
            }
        }
    }
    {
        let wf = kt.triangles[ntid].wavefronts[nidx];
        let nb = kt.triangles[ntid].neighbors[nidx];
        if let Some(eid) = wf {
            if let Some(e) = kt.edges.get_mut(eid) {
                e.incident_triangle = Some(ntid);
            }
        } else if let Some(other_id) = nb {
            if let Some(other) = kt.triangles.get_mut(other_id) {
                for s in 0..3 {
                    if other.neighbors[s] == Some(tid) {
                        other.neighbors[s] = Some(ntid);
                        other.invalidate_collapse();
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
