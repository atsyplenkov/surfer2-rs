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
use crate::triangulation::around_vertex::{most_around, step_around};
use crate::triangulation::basic::{build_basic_triangulation, select_polygon_interior};
use crate::triangulation::kinetic::{
    compute_collapse_fast, KineticTriangulation, SkeletonFaceMeta, TriangleId,
};
use crate::wavefront::edge::WavefrontEdge;
use crate::wavefront::supporting_line::{intersection_velocity, SupportingLine};
use crate::wavefront::vertex::{InfiniteSpeedType, VertexId, WavefrontVertex};
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
            check_invariants_each_step: cfg!(debug_assertions),
            simultaneous_streak: 0,
            last_event_time: f64::NEG_INFINITY,
        })
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
        let tid = self.kt.triangle_by_tidx(tidx).ok_or_else(|| {
            crate::error::PropagationError::InconsistentEvent {
                triangle: tidx as u64,
                time: spec.time,
            }
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
                (t.wavefronts[i].is_some(), t.neighbors[i], t.wavefronts[i])
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
        // Snapshot supporting lines BEFORE move_constraints clears wavefronts on t.
        let (va, vb, left_sl, right_sl) = {
            let t = &self.kt.triangles[tid];
            let va = t.vertices[ccw(edge_idx)].unwrap();
            let vb = t.vertices[cw(edge_idx)].unwrap();
            let left_sl = t.wavefronts[cw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
            let right_sl = t.wavefronts[ccw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
            (va, vb, left_sl, right_sl)
        };
        self.kt.stop_vertex(va, spec.time)?;
        self.kt.stop_vertex(vb, spec.time)?;
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.set_dying();
        }
        let receivers = move_constraints_to_neighbor(&mut self.kt, tid, edge_idx)?;
        for ntid in receivers {
            if self.kt.triangles[ntid].live() {
                self.queue.needs_update(self.kt.triangles[ntid].tidx)?;
            }
        }
        self.constraint_collapse_part2(tid, edge_idx, spec.time, left_sl, right_sl)?;
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
            let (left_sl, right_sl) = {
                use crate::triangulation::kinetic::{ccw, cw};
                let t = &self.kt.triangles[tid];
                let left_sl = t.wavefronts[cw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
                let right_sl =
                    t.wavefronts[ccw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
                (left_sl, right_sl)
            };
            return self.constraint_collapse_part2(tid, edge_idx, spec.time, left_sl, right_sl);
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
        let (left_sl, right_sl) = {
            let t = &self.kt.triangles[tid];
            let left_sl = t.wavefronts[cw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
            let right_sl = t.wavefronts[ccw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
            (left_sl, right_sl)
        };
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            if !tr.is_dying {
                tr.set_dying();
            }
        }
        let receivers = move_constraints_to_neighbor(&mut self.kt, tid, edge_idx)?;
        for ntid in receivers {
            if self.kt.triangles[ntid].live() {
                self.queue.needs_update(self.kt.triangles[ntid].tidx)?;
            }
        }
        self.constraint_collapse_part2(tid, edge_idx, time, left_sl, right_sl)?;
        let _ = vb;
        Ok(())
    }

    /// Upstream `do_constraint_collapse_part2`: spawn successor, rewire fans, drop t.
    fn constraint_collapse_part2(
        &mut self,
        tid: TriangleId,
        edge_idx: usize,
        time: f64,
        left_sl: Option<SupportingLine>,
        right_sl: Option<SupportingLine>,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        let (va, vb, na_id, nb_id, collapsing_eid) = {
            let t = &self.kt.triangles[tid];
            let va = t.vertices[ccw(edge_idx)].unwrap();
            let vb = t.vertices[cw(edge_idx)].unwrap();
            let collapsing_eid = t.wavefronts[edge_idx];
            (
                va,
                vb,
                t.neighbors[cw(edge_idx)],
                t.neighbors[ccw(edge_idx)],
                collapsing_eid,
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

        // C++: ea = va.incident_wavefront_edge(0), eb = vb.incident_wavefront_edge(1)
        // remaining edges after the collapsed one dies. Prefer those; fall back to
        // triangle-side SLs snapshotted before move_constraints.
        let ea_id = {
            let v = &self.kt.vertices[va];
            [v.incident_edges[0], v.incident_edges[1]]
                .into_iter()
                .flatten()
                .find(|&eid| Some(eid) != collapsing_eid && !self.kt.edges[eid].dead)
        };
        let eb_id = {
            let v = &self.kt.vertices[vb];
            // Prefer slot 1 then 0 (C++ incident(1) for vb).
            [v.incident_edges[1], v.incident_edges[0]]
                .into_iter()
                .flatten()
                .find(|&eid| Some(eid) != collapsing_eid && !self.kt.edges[eid].dead)
        };
        let mut velocity = Point2::new(0.0, 0.0);
        let sl_pair = match (ea_id, eb_id) {
            (Some(a), Some(b)) if a != b => Some((
                self.kt.edges[a].supporting_line,
                self.kt.edges[b].supporting_line,
            )),
            _ => match (left_sl, right_sl) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => None,
            },
        };
        if let Some((a, b)) = sl_pair {
            if let Some((_, vel)) = intersection_velocity(a, b) {
                velocity = vel;
            }
        }

        // C++ make_vertex: pos_zero is extrapolated to t=0 so p(t)=pos_zero+t*vel.
        let pos_zero = Point2::new(pos.x - time * velocity.x, pos.y - time * velocity.y);
        let ang = match (ea_id, eb_id) {
            (Some(a), Some(b)) => vertex_angle_from_lines(
                self.kt.edges[a].supporting_line,
                self.kt.edges[b].supporting_line,
            ),
            _ => crate::wavefront::vertex::AngleType::Acute,
        };
        let mut nv =
            WavefrontVertex::new_kinetic(pos_zero, velocity, time, InfiniteSpeedType::None);
        nv.angle = ang;
        // Ordered: incident[0]=edge into nvid (eb from vb side), incident[1]=edge out (ea).
        // C++ make_vertex(pos,time,ea,eb): a=ea attaches as edge 1 of new?
        // Actually new vertex is between ea and eb: wavefronts[0]=ea, wavefronts[1]=eb
        // Looking at make_vertex(a,b): incident a and b.
        nv.incident_edges[0] = ea_id; // edge a
        nv.incident_edges[1] = eb_id; // edge b
        let nvid = self.kt.vertices.insert(nv);
        if let Some(eid) = ea_id {
            if let Some(e) = self.kt.edges.get_mut(eid) {
                for ep in &mut e.endpoints {
                    if *ep == va || *ep == vb {
                        *ep = nvid;
                    }
                }
            }
        }
        if let Some(eid) = eb_id {
            if let Some(e) = self.kt.edges.get_mut(eid) {
                for ep in &mut e.endpoints {
                    if *ep == va || *ep == vb {
                        *ep = nvid;
                    }
                }
            }
        }
        if let Some(vv) = self.kt.vertices.get_mut(va) {
            vv.next_vertex[0] = Some(nvid);
        }
        if let Some(vv) = self.kt.vertices.get_mut(vb) {
            vv.next_vertex[1] = Some(nvid);
        }
        if let Some(nv) = self.kt.vertices.get_mut(nvid) {
            nv.prev_vertex[0] = Some(va);
            nv.prev_vertex[1] = Some(vb);
        }

        self.rewrite_star_side(tid, ccw(edge_idx), false, nvid)?;
        self.rewrite_star_side(tid, cw(edge_idx), true, nvid)?;

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
                t.wavefronts[edge_idx].ok_or_else(|| {
                    crate::error::PropagationError::TopologyInvariant {
                        detail: "split: missing constraint".into(),
                    }
                })?,
                t.neighbors[cw(edge_idx)],
                t.neighbors[ccw(edge_idx)],
            )
        };
        let eb = self.kt.vertices[v].incident_edges[0];
        let ea = self.kt.vertices[v].incident_edges[1];
        self.kt.stop_vertex(v, time)?;
        let pos = self.kt.vertices[v]
            .pos_stop
            .unwrap_or_else(|| self.kt.vertices[v].position_at(time));
        let base_sl = self.kt.edges[eid].supporting_line;
        let face = self.kt.edges[eid].skeleton_face;
        let source = self.kt.edges[eid].source_input_edge;
        let bevel = self.kt.edges[eid].is_beveling;
        let ep0 = self.kt.edges[eid].endpoints[0];
        let ep1 = self.kt.edges[eid].endpoints[1];

        if let Some(e) = self.kt.edges.get_mut(eid) {
            e.dead = true;
        }
        // C++ WavefrontEdge::split: nea=(ep0, null), neb=(null, ep1)
        let nea = self
            .kt
            .edges
            .insert(WavefrontEdge::new([ep0, v], base_sl, face, source, bevel));
        let neb = self
            .kt
            .edges
            .insert(WavefrontEdge::new([v, ep1], base_sl, face, source, bevel));

        let nva_id = self.make_split_successor(pos, time, Some(nea), ea)?;
        let nvb_id = self.make_split_successor(pos, time, eb, Some(neb))?;

        if let Some(e0) = self.kt.vertices.get_mut(ep0) {
            e0.incident_edges[1] = Some(nea);
        }
        if let Some(e) = self.kt.edges.get_mut(nea) {
            e.endpoints[1] = nva_id;
        }
        if let Some(e) = self.kt.edges.get_mut(neb) {
            e.endpoints[0] = nvb_id;
        }
        if let Some(e1) = self.kt.vertices.get_mut(ep1) {
            e1.incident_edges[0] = Some(neb);
        }

        if let Some(vv) = self.kt.vertices.get_mut(v) {
            vv.next_vertex[0] = Some(nvb_id);
            vv.next_vertex[1] = Some(nva_id);
        }
        if let Some(nva) = self.kt.vertices.get_mut(nva_id) {
            nva.prev_vertex[0] = Some(nvb_id);
        }
        if let Some(nvb) = self.kt.vertices.get_mut(nvb_id) {
            nvb.prev_vertex[1] = Some(nva_id);
        }

        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.set_dying();
        }

        self.rewrite_star_side(tid, edge_idx, true, nva_id)?;
        self.rewrite_star_side(tid, edge_idx, false, nvb_id)?;

        if let Some(na) = na_id {
            if let Some(i) = (0..3).find(|&s| self.kt.triangles[na].neighbors[s] == Some(tid)) {
                self.kt.set_wavefront(na, i, nea);
                if self.kt.triangles[na].live() {
                    self.queue.needs_update(self.kt.triangles[na].tidx)?;
                }
            }
        }
        if let Some(nb) = nb_id {
            if let Some(i) = (0..3).find(|&s| self.kt.triangles[nb].neighbors[s] == Some(tid)) {
                self.kt.set_wavefront(nb, i, neb);
                if self.kt.triangles[nb].live() {
                    self.queue.needs_update(self.kt.triangles[nb].tidx)?;
                }
            }
        }

        self.queue.needs_dropping(self.kt.triangles[tid].tidx)?;
        if let Some(t) = self.kt.triangles.get_mut(tid) {
            t.set_dead();
        }
        Ok(())
    }

    fn make_split_successor(
        &mut self,
        pos: Point2,
        time: f64,
        a: Option<crate::wavefront::edge::EdgeId>,
        b: Option<crate::wavefront::edge::EdgeId>,
    ) -> Result<VertexId, SkeletonError> {
        let mut vel = Point2::new(0.0, 0.0);
        let mut pos_zero = pos;
        let mut ang = crate::wavefront::vertex::AngleType::Acute;
        if let (Some(ea), Some(eb)) = (a, b) {
            let sla = self.kt.edges[ea].supporting_line;
            let slb = self.kt.edges[eb].supporting_line;
            ang = vertex_angle_from_lines(sla, slb);
            if let Some((p0, v)) = intersection_velocity(sla, slb) {
                pos_zero = p0;
                vel = v;
            }
        }
        let mut nv = WavefrontVertex::new_kinetic(pos_zero, vel, time, InfiniteSpeedType::None);
        nv.angle = ang;
        nv.incident_edges = [a, b];
        Ok(self.kt.vertices.insert(nv))
    }

    fn rewrite_star_side(
        &mut self,
        start: TriangleId,
        v_idx: usize,
        use_cw: bool,
        new_v: VertexId,
    ) -> Result<(), SkeletonError> {
        let mut cur = step_around(&self.kt, start, v_idx, use_cw);
        let mut guard = 0;
        while let Some((id, idx)) = cur {
            guard += 1;
            if guard > 10_000 {
                return Err(crate::error::PropagationError::TopologyInvariant {
                    detail: "split: star walk did not terminate".into(),
                }
                .into());
            }
            self.kt.set_vertex(id, idx, new_v);
            if self.kt.triangles[id].live() {
                self.queue.needs_update(self.kt.triangles[id].tidx)?;
            }
            cur = step_around(&self.kt, id, idx, use_cw);
        }
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
        use crate::triangulation::kinetic::cw;
        // C++: flip the CW CH spoke, not relevant_edge (the infinite-vertex side).
        let side = cw(spec.relevant_edge as usize);
        let Some(ntid) = self.kt.triangles[tid].neighbors[side] else {
            return Ok(());
        };
        raw_flip(&mut self.kt, tid, side, ntid)?;
        self.queue.needs_update(self.kt.triangles[tid].tidx)?;
        self.queue.needs_update(self.kt.triangles[ntid].tidx)?;
        self.mark_adjacent_update(tid)?;
        self.mark_adjacent_update(ntid)?;
        Ok(())
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
    let t =
        kt.triangles
            .get(tid)
            .ok_or_else(|| crate::error::PropagationError::InconsistentEvent {
                triangle: tidx as u64,
                time: now,
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
        let mut v =
            WavefrontVertex::new_kinetic(*p, Point2::new(0.0, 0.0), 0.0, InfiniteSpeedType::None);
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
            // C++: vertices[ccw]=edge.v0 has wavefronts[1]=edge;
            //      vertices[cw]=edge.v1 has wavefronts[0]=edge.
            if let Some(v) = kt.vertices.get_mut(ea) {
                v.incident_edges[1] = Some(eid);
            }
            if let Some(v) = kt.vertices.get_mut(eb) {
                v.incident_edges[0] = Some(eid);
            }
            kt.set_wavefront(tid, s, eid);
        }
    }

    create_kinetic_vertices(&mut kt)?;
    create_bevels(&mut kt, input)?;
    refine_triangulation_initial(&mut kt)?;

    Ok(kt)
}

/// C++ `create_kinetic_vertices`: one kinetic vertex per wavefront sector,
/// not one per input point (interior and exterior must not share a vertex).
fn create_kinetic_vertices(kt: &mut KineticTriangulation) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let tids: Vec<_> = kt.triangles.keys().collect();
    for tid in tids {
        if !kt.triangles.get(tid).map(|t| t.live()).unwrap_or(false) {
            continue;
        }
        for i in 0..3 {
            let l = match kt.triangles[tid].wavefronts[ccw(i)] {
                Some(e) => e,
                None => continue,
            };
            let start_v = match kt.triangles[tid].vertices[i] {
                Some(v) => v,
                None => continue,
            };
            if kt.vertices[start_v].source_input_vertex.is_none() {
                continue;
            }

            let mut most = (tid, i);
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > 10_000 {
                    return Err(crate::error::PropagationError::TopologyInvariant {
                        detail: "kinetic vertex fan did not terminate".into(),
                    }
                    .into());
                }
                match step_around(kt, most.0, most.1, true) {
                    Some(n) => most = n,
                    None => break,
                }
            }
            let Some(r) = kt.triangles[most.0].wavefronts[cw(most.1)] else {
                // Degree-1: bevels later.
                continue;
            };
            if l == r {
                continue;
            }

            let pos = kt.vertices[start_v].pos_zero;
            let sl_l = kt.edges[l].supporting_line;
            let sl_r = kt.edges[r].supporting_line;
            let vel = intersection_velocity(sl_l, sl_r)
                .map(|(_, v)| v)
                .unwrap_or(Point2::new(0.0, 0.0));
            let ang = vertex_angle_from_lines(sl_l, sl_r);
            let mut nv = WavefrontVertex::new_kinetic(pos, vel, 0.0, InfiniteSpeedType::None);
            nv.angle = ang;
            nv.is_initial = true;
            nv.incident_edges = [Some(l), Some(r)];
            let nvid = kt.vertices.insert(nv);

            let mut walk = Some((tid, i));
            let mut wguard = 0;
            while let Some((id, idx)) = walk {
                wguard += 1;
                if wguard > 10_000 {
                    break;
                }
                kt.set_vertex(id, idx, nvid);
                walk = step_around(kt, id, idx, true);
            }
        }
    }
    Ok(())
}

fn create_bevels(kt: &mut KineticTriangulation, input: &Pslg) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let mut deg = vec![0u32; input.vertex_count()];
    for e in input.edges() {
        deg[e.vertices[0]] += 1;
        deg[e.vertices[1]] += 1;
    }

    let starts: Vec<(TriangleId, usize)> = kt
        .triangles
        .iter()
        .filter(|(_, t)| t.live())
        .flat_map(|(tid, t)| (0..3).map(move |i| (tid, i, t.vertices[i])))
        .filter_map(|(tid, i, vid)| {
            let vid = vid?;
            let src = kt.vertices[vid].source_input_vertex?;
            if deg[src as usize] == 1 {
                Some((tid, i))
            } else {
                None
            }
        })
        .collect();

    for (tid, i) in starts {
        let Some(vid) = kt.triangles.get(tid).and_then(|t| t.vertices[i]) else {
            continue;
        };
        if kt.vertices[vid].source_input_vertex.is_none() {
            continue;
        }
        create_bevels_at_vertex(kt, tid, i)?;
    }
    let _ = (ccw(0), cw(0));
    Ok(())
}

fn create_bevels_at_vertex(
    kt: &mut KineticTriangulation,
    tid: TriangleId,
    i: usize,
) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let (most_ccw, most_cw) = (
        most_around(kt, tid, i, false),
        most_around(kt, tid, i, true),
    );
    let l = kt.triangles[most_ccw.0].wavefronts[ccw(most_ccw.1)];
    let r = kt.triangles[most_cw.0].wavefronts[cw(most_cw.1)];
    let (Some(l), Some(r)) = (l, r) else {
        return Ok(());
    };
    // ponytail: C++ aborts on unequal weights / reflex bevels
    if (kt.edges[l].supporting_line.weight - kt.edges[r].supporting_line.weight).abs() > 0.0 {
        return Ok(());
    }

    let pos = kt.vertices[kt.triangles[tid].vertices[i].unwrap()].pos_zero;
    let dl = kt.edges[l].supporting_line.direction;
    // C++: opposite(l).perpendicular(p) → dir = rot90ccw(-D) = (Dy, -Dx)
    let bevel_dir = Point2::new(dl.y, -dl.x);
    let sl = SupportingLine::from_point_dir(pos, bevel_dir, kt.edges[l].supporting_line.weight)
        .ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
            detail: "bevel supporting line".into(),
        })?;
    let face = kt.faces.len() as u32;
    kt.faces.push(SkeletonFaceMeta {
        source_input_edge: None,
        is_beveling: true,
        component: kt.triangles[tid].component,
    });
    let mut bevel = WavefrontEdge::new(
        [kt.triangles[tid].vertices[i].unwrap(); 2],
        sl,
        face,
        None,
        true,
    );
    bevel.is_initial = true;
    let bevel_id = kt.edges.insert(bevel);

    let v0 = make_initial_kv(kt, pos, Some(l), Some(bevel_id));
    let v1 = make_initial_kv(kt, pos, Some(bevel_id), Some(r));
    if let Some(a) = kt.vertices.get_mut(v1) {
        a.prev_vertex[0] = Some(v0);
    }
    if let Some(b) = kt.vertices.get_mut(v0) {
        b.prev_vertex[1] = Some(v1);
    }

    // Stamp most-ccw side with v0, split once, rest with v1.
    let mut t_it = most_ccw;
    let mut new_v = v0;
    let mut current_edge = bevel_id;
    let mut last = false;
    let mut guard = 0;
    while kt.triangles.get(t_it.0).is_some() {
        guard += 1;
        if guard > 10_000 {
            break;
        }
        kt.set_vertex(t_it.0, t_it.1, new_v);
        let next_cw = step_around(kt, t_it.0, t_it.1, true);
        let split_this = if last {
            false
        } else if next_cw.is_none() {
            true
        } else {
            // Finite ccw corner: split if that point is left of bevel direction from pos.
            let ccw_vid = kt.triangles[t_it.0].vertices[ccw(t_it.1)];
            match ccw_vid {
                Some(cv) => {
                    let p = kt.vertices[cv].pos_zero;
                    let cross = bevel_dir.x * (p.y - pos.y) - bevel_dir.y * (p.x - pos.x);
                    cross > 0.0
                }
                None => true,
            }
        };
        if split_this && !last {
            let new_t = split_vertex_bevel(kt, t_it.0, t_it.1, current_edge, v1)?;
            last = true;
            new_v = v1;
            current_edge = r;
            t_it = (new_t, 0);
        } else if let Some(n) = next_cw {
            t_it = n;
        } else {
            break;
        }
    }
    Ok(())
}

fn make_initial_kv(
    kt: &mut KineticTriangulation,
    pos: Point2,
    a: Option<crate::wavefront::edge::EdgeId>,
    b: Option<crate::wavefront::edge::EdgeId>,
) -> VertexId {
    let (vel, ang) = match (a, b) {
        (Some(ea), Some(eb)) => {
            let sla = kt.edges[ea].supporting_line;
            let slb = kt.edges[eb].supporting_line;
            let vel = intersection_velocity(sla, slb)
                .map(|(_, v)| v)
                .unwrap_or(Point2::new(0.0, 0.0));
            (vel, vertex_angle_from_lines(sla, slb))
        }
        _ => (
            Point2::new(0.0, 0.0),
            crate::wavefront::vertex::AngleType::Reflex,
        ),
    };
    let mut nv = WavefrontVertex::new_kinetic(pos, vel, 0.0, InfiniteSpeedType::None);
    nv.angle = ang;
    nv.is_initial = true;
    nv.incident_edges = [a, b];
    kt.vertices.insert(nv)
}

fn split_vertex_bevel(
    kt: &mut KineticTriangulation,
    tid: TriangleId,
    i: usize,
    new_e: crate::wavefront::edge::EdgeId,
    new_v: VertexId,
) -> Result<TriangleId, SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let component = kt.triangles[tid].component;
    let new_t = kt.alloc_triangle(component);
    let (wf_cw, nb_cw, v_ccw, v_i) = {
        let t = &kt.triangles[tid];
        (
            t.wavefronts[cw(i)],
            t.neighbors[cw(i)],
            t.vertices[ccw(i)],
            t.vertices[i],
        )
    };
    {
        let n = kt.triangles.get_mut(new_t).unwrap();
        n.wavefronts[0] = None;
        n.wavefronts[1] = Some(new_e);
        n.wavefronts[2] = wf_cw;
        n.neighbors[0] = Some(tid);
        n.neighbors[1] = None;
        n.neighbors[2] = nb_cw;
    }
    kt.set_vertex(new_t, 0, new_v);
    if let Some(v) = v_ccw {
        kt.set_vertex(new_t, 1, v);
    }
    if let Some(v) = v_i {
        kt.set_vertex(new_t, 2, v);
    }
    {
        let t = kt.triangles.get_mut(tid).unwrap();
        t.wavefronts[cw(i)] = None;
        t.neighbors[cw(i)] = Some(new_t);
        t.invalidate_collapse();
    }
    if let Some(nb) = nb_cw {
        if let Some(s) = (0..3).find(|&s| kt.triangles[nb].neighbors[s] == Some(tid)) {
            kt.triangles[nb].neighbors[s] = Some(new_t);
            kt.triangles[nb].invalidate_collapse();
        }
    }
    if let Some(e) = kt.edges.get_mut(new_e) {
        if e.incident_triangle.is_none() {
            e.incident_triangle = Some(new_t);
        }
    }
    if let Some(eid) = wf_cw {
        if let Some(e) = kt.edges.get_mut(eid) {
            e.incident_triangle = Some(new_t);
        }
    }
    Ok(new_t)
}

fn vertex_angle_from_lines(
    a: SupportingLine,
    b: SupportingLine,
) -> crate::wavefront::vertex::AngleType {
    use crate::wavefront::vertex::AngleType;
    // C++ WavefrontVertex: orientation(a.direction, b.direction).
    // LEFT_TURN → convex, COLLINEAR → straight, RIGHT_TURN → reflex.
    let d0 = a.direction;
    let d1 = b.direction;
    let cross = d0.x * d1.y - d0.y * d1.x;
    let scale = d0.x.hypot(d0.y) * d1.x.hypot(d1.y);
    if scale == 0.0 || cross.abs() <= 1e-14 * scale {
        return AngleType::Straight;
    }
    if cross > 0.0 {
        let dot = d0.x * d1.x + d0.y * d1.y;
        if dot < 0.0 {
            AngleType::Obtuse
        } else {
            AngleType::Acute
        }
    } else {
        AngleType::Reflex
    }
}

fn refine_triangulation_initial(kt: &mut KineticTriangulation) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};
    // One pass + drain: flip triangles with a single reflex unconstrained corner.
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 10_000 {
            break;
        }
        let mut flipped = false;
        let tids: Vec<_> = kt.triangles.keys().collect();
        for tid in tids {
            let t = match kt.triangles.get(tid) {
                Some(t) if t.live() && !t.is_unbounded => t,
                _ => continue,
            };
            let mut reflex_idx = None;
            let mut n_reflex = 0;
            for i in 0..3 {
                if let Some(vid) = t.vertices[i] {
                    if let Some(v) = kt.vertices.get(vid) {
                        if !v.is_convex_or_straight() {
                            n_reflex += 1;
                            reflex_idx = Some(i);
                        }
                    }
                }
            }
            if n_reflex != 1 {
                continue;
            }
            let ri = reflex_idx.unwrap();
            if t.wavefronts[ri].is_some() {
                continue; // constrained opposite reflex
            }
            let Some(nid) = t.neighbors[ri] else { continue };
            if kt
                .triangles
                .get(nid)
                .map(|n| n.is_unbounded || !n.live())
                .unwrap_or(true)
            {
                continue;
            }
            // Flip spoke opposite the reflex vertex; keep only if topology stays valid.
            if raw_flip(kt, tid, ri, nid).is_err() {
                continue;
            }
            let ok = kt
                .triangles
                .get(tid)
                .map(|t| t.check_side_xor().is_ok())
                .unwrap_or(false)
                && kt
                    .triangles
                    .get(nid)
                    .map(|t| t.check_side_xor().is_ok())
                    .unwrap_or(false);
            if !ok {
                // Best-effort: flip back
                let _ = raw_flip(kt, tid, ri, nid);
                continue;
            }
            flipped = true;
        }
        if !flipped {
            break;
        }
    }
    let _ = (ccw(0), cw(0));
    Ok(())
}

fn estimate_vertex_velocity(_input: &Pslg, _incident: &[(usize, bool)], _p: Point2) -> Point2 {
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
) -> Result<Vec<TriangleId>, SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let mut receivers = Vec::new();
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
            // C++ move_constraint_from keeps the neighbor pointer to dying t
            // so the later star walk can still step across this side.
            n.invalidate_collapse();
        }
        {
            let t = kt.triangles.get_mut(tid).unwrap();
            t.wavefronts[src_side] = None;
            t.invalidate_collapse();
        }
        receivers.push(nid);
    }
    Ok(receivers)
}

fn link_vertex_edge(
    kt: &mut KineticTriangulation,
    vid: crate::wavefront::vertex::VertexId,
    eid: crate::wavefront::edge::EdgeId,
) {
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
