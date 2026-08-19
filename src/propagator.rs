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
    compute_collapse, KineticTriangulation, SkeletonFaceMeta, TriangleId,
};
use crate::wavefront::edge::WavefrontEdge;
use crate::wavefront::split_leftover::{
    classify_successor_motion, leftover_action, leftover_schedule, LeftoverAction,
    LeftoverSchedule, SuccessorMotion,
};
use crate::wavefront::supporting_line::{wavefront_vertex_motion, SupportingLine};
use crate::wavefront::vertex::{infinite_speed_type, InfiniteSpeedType, VertexId, WavefrontVertex};
use crate::{ComponentMode, Options};

/// f64 stand-in for C++ `pos_stop` equality at UTM scale.
const JUNCTION_M: f64 = 5e-3;

/// Incremental propagator with step/advance_to/finish.
pub struct Propagator {
    pub kt: KineticTriangulation,
    pub queue: EventQueue,
    pub time: f64,
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
            stats: Stats::default(),
            finalized: false,
            check_invariants_each_step: cfg!(debug_assertions),
            simultaneous_streak: 0,
            last_event_time: f64::NEG_INFINITY,
        })
    }

    pub fn from_polygon(polygon: &Polygon) -> Result<Self, SkeletonError> {
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
            if self.simultaneous_streak > 10_000 {
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
        self.seal_stops(spec.time);

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
        let mut used = std::collections::HashSet::new();
        for t in self.kt.triangles.values() {
            for v in t.vertices.into_iter().flatten() {
                used.insert(v);
            }
        }
        // Stop remaining free vertices as rays
        let ids: Vec<_> = self.kt.vertices.keys().collect();
        for vid in ids {
            let stopped = self.kt.vertices[vid].is_stopped();
            let is_inf = self.kt.infinite_vertex == Some(vid);
            let dummy = self.kt.vertices[vid].source_input_vertex.is_some() && !used.contains(&vid);
            if dummy {
                if let Some(vv) = self.kt.vertices.get_mut(vid) {
                    vv.time_stop = Some(f64::INFINITY);
                }
                continue;
            }
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
                    vertex: Some(vid),
                });
                let _ = vel;
                if let Some(vv) = self.kt.vertices.get_mut(vid) {
                    vv.time_stop = Some(f64::INFINITY);
                }
            }
        }
        self.fill_missing_next();
        debug_assert!(
            self.unlinked_stops() == 0,
            "C++ stop invariant: {} vertices stopped with a missing next_vertex",
            self.unlinked_stops()
        );
        self.finalized = true;
        Ok(())
    }

    fn unlinked_stops(&self) -> usize {
        self.kt
            .vertices
            .iter()
            .filter(|(id, v)| {
                self.kt.infinite_vertex != Some(*id)
                    && v.time_stop.is_some_and(|t| t.is_finite())
                    && v.pos_stop.is_some()
                    && (v.next_vertex[0].is_none() || v.next_vertex[1].is_none())
            })
            .count()
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
        debug_assert!(
            self.kt.check_refinement.is_empty(),
            "C++ handle_event: check_refinement empty at event start"
        );

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
        // C++ drains after every event. Until bevel need_flip and the
        // infinitely-fast weighted handler exist, only collapse/split on
        // unweighted PSLGs is safe: refine on weighted leftover drops locked
        // random-2 131→128, and flip/CH refine loops bevels.
        let refine_after = !self.kt_has_nonunit_weight()
            && matches!(
                spec.type_,
                CollapseType::TriangleCollapse
                    | CollapseType::ConstraintCollapse
                    | CollapseType::SpokeCollapse
                    | CollapseType::SplitOrFlipRefine
            );
        if refine_after {
            process_check_refinement_queue(&mut self.kt, Some(&mut self.queue), spec.time)?;
        } else {
            while self.kt.check_refinement_pop().is_some() {}
        }
        Ok(())
    }

    fn kt_has_nonunit_weight(&self) -> bool {
        self.kt
            .edges
            .values()
            .any(|e| (e.supporting_line.weight - 1.0).abs() > 1e-15)
    }

    /// C++ `KineticTriangulation::modified`.
    fn modified(&mut self, tid: TriangleId, front: bool) -> Result<(), SkeletonError> {
        mark_modified(&mut self.kt, Some(&mut self.queue), tid, front)
    }

    fn handle_triangle_collapse(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        let time = spec.time;
        let verts = {
            let t = &self.kt.triangles[tid];
            [
                t.vertices[0].unwrap(),
                t.vertices[1].unwrap(),
                t.vertices[2].unwrap(),
            ]
        };
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.set_dying();
        }
        for &v in &verts {
            self.kt.stop_vertex(v, time)?;
        }

        // Per side: constraint dies (C++ head-to-head next), or spoke → part2.
        for i in 0..3 {
            let (is_const, nb, eid) = {
                let t = &self.kt.triangles[tid];
                (t.wavefronts[i].is_some(), t.neighbors[i], t.wavefronts[i])
            };
            if is_const {
                let a = verts[cw(i)];
                let b = verts[ccw(i)];
                let pa = self.kt.vertices[a]
                    .pos_stop
                    .unwrap_or_else(|| self.kt.vertices[a].position_at(time));
                let pb = self.kt.vertices[b]
                    .pos_stop
                    .unwrap_or_else(|| self.kt.vertices[b].position_at(time));
                if pa.distance_to(pb) <= JUNCTION_M {
                    self.set_next_vertex(a, 0, b, false);
                } else {
                    self.spawn_child_to_neighbor(a, time);
                    self.spawn_child_to_neighbor(b, time);
                }
                if let Some(e) = eid {
                    if let Some(edge) = self.kt.edges.get_mut(e) {
                        edge.dead = true;
                    }
                }
            } else if let Some(nid) = nb {
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
        self.stop_collapse_pair(va, vb, spec.time)?;
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
        self.stop_collapse_pair(va, vb, spec.time)?;

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
                    let (a, b) = {
                        let t = &self.kt.triangles[tid];
                        (
                            t.vertices[ccw(edge)].unwrap(),
                            t.vertices[cw(edge)].unwrap(),
                        )
                    };
                    self.set_next_vertex(a, 1, b, false);
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

        self.stop_collapse_pair(va, vb, time)?;
        let pos = self.kt.vertices[va]
            .pos_stop
            .unwrap_or_else(|| self.kt.vertices[va].position_at(time));

        // C++: ea = va.incident(0), eb = vb.incident(1). Fall back to the
        // other slot if that one is the collapsing edge.
        let remaining = |vid: VertexId, prefer: usize| {
            let inc = self.kt.vertices[vid].incident_edges;
            [inc[prefer], inc[1 - prefer]]
                .into_iter()
                .flatten()
                .find(|&eid| Some(eid) != collapsing_eid && !self.kt.edges[eid].dead)
        };
        let ea_id = remaining(va, 0);
        let eb_id = remaining(vb, 1);
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
        let ang = match sl_pair {
            Some((a, b)) => vertex_angle_from_lines(a, b),
            _ => crate::wavefront::vertex::AngleType::Acute,
        };
        let inf = match sl_pair {
            Some((a, b)) => infinite_speed_type(a.normal, b.normal, a.weight, b.weight, ang),
            None => InfiniteSpeedType::None,
        };
        let (pos_zero, velocity) = if inf != InfiniteSpeedType::None {
            (pos, Point2::new(0.0, 0.0))
        } else {
            match sl_pair {
                Some((a, b)) => wavefront_vertex_motion(a, b, pos, time),
                None => (pos, Point2::new(0.0, 0.0)),
            }
        };
        let mut nv = WavefrontVertex::new_kinetic(pos_zero, velocity, time, inf);
        nv.pos_start = pos;
        nv.angle = ang;
        nv.incident_edges = [ea_id, eb_id];
        let nvid = self.kt.vertices.insert(nv);
        // Far ends before the fan walk (C++: v is ea.vertex(1) and eb.vertex(0)).
        let ea_far = ea_id.map(|e| self.kt.edges[e].endpoints[0]);
        let eb_far = eb_id.map(|e| self.kt.edges[e].endpoints[1]);
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
        // set_vertex may have written nvid into the wrong endpoint slot.
        if let (Some(eid), Some(far)) = (ea_id, ea_far) {
            if let Some(e) = self.kt.edges.get_mut(eid) {
                e.endpoints = [far, nvid];
            }
        }
        if let (Some(eid), Some(far)) = (eb_id, eb_far) {
            if let Some(e) = self.kt.edges.get_mut(eid) {
                e.endpoints = [nvid, far];
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
        if self.kt.triangles[tid].is_unbounded
            || self.kt.triangles[tid]
                .vertices
                .iter()
                .any(|v| *v == self.kt.infinite_vertex)
        {
            return self.handle_ccw_leaves(tid, spec);
        }
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

        let (nva_id, nva_motion) = self.make_split_successor(pos, time, Some(nea), ea)?;
        let (nvb_id, nvb_motion) = self.make_split_successor(pos, time, eb, Some(neb))?;

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
                self.schedule_leftover_collapse(na, i, nva_id, nva_motion, pos, time)?;
                if self.kt.triangles[na].live() {
                    self.queue.needs_update(self.kt.triangles[na].tidx)?;
                }
            }
        }
        if let Some(nb) = nb_id {
            if let Some(i) = (0..3).find(|&s| self.kt.triangles[nb].neighbors[s] == Some(tid)) {
                self.kt.set_wavefront(nb, i, neb);
                self.schedule_leftover_collapse(nb, i, nvb_id, nvb_motion, pos, time)?;
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
    ) -> Result<(VertexId, SuccessorMotion), SkeletonError> {
        let mut vel = Point2::new(0.0, 0.0);
        let mut pos_zero = pos;
        let mut ang = crate::wavefront::vertex::AngleType::Acute;
        let mut inf = InfiniteSpeedType::None;
        let mut motion = SuccessorMotion::Finite;
        if let (Some(ea), Some(eb)) = (a, b) {
            let sla = self.kt.edges[ea].supporting_line;
            let slb = self.kt.edges[eb].supporting_line;
            ang = vertex_angle_from_lines(sla, slb);
            inf = infinite_speed_type(sla.normal, slb.normal, sla.weight, slb.weight, ang);
            if inf == InfiniteSpeedType::None {
                let (p0, v) = wavefront_vertex_motion(sla, slb, pos, time);
                pos_zero = p0;
                vel = v;
                motion = classify_successor_motion(sla, slb);
            }
        }
        let mut nv = WavefrontVertex::new_kinetic(pos_zero, vel, time, inf);
        nv.pos_start = pos;
        nv.angle = ang;
        nv.incident_edges = [a, b];
        Ok((self.kt.vertices.insert(nv), motion))
    }

    /// C++'s second same-time event after a split: the leftover face dies now.
    fn schedule_leftover_collapse(
        &mut self,
        tid: TriangleId,
        side: usize,
        child: VertexId,
        motion: SuccessorMotion,
        pos: Point2,
        time: f64,
    ) -> Result<(), SkeletonError> {
        let (nconst, component) = {
            let tr = self.kt.triangles.get(tid).ok_or_else(|| {
                crate::error::PropagationError::TopologyInvariant {
                    detail: format!("leftover collapse: missing face {tid:?}"),
                }
            })?;
            if self.kt.vertices.get(child).is_none() {
                return Err(crate::error::PropagationError::TopologyInvariant {
                    detail: format!("leftover collapse: missing split child {child:?}"),
                }
                .into());
            }
            if !tr.live() {
                return Ok(());
            }
            let nconst = tr.wavefronts.iter().filter(|w| w.is_some()).count();
            (nconst, tr.component)
        };
        match leftover_schedule(nconst, motion) {
            LeftoverSchedule::LeaveNatural | LeftoverSchedule::Unsupported => {
                if nconst == 3 && motion == SuccessorMotion::NonIntersecting {
                    // Face stays natural (3-const CollapseNow ate the
                    // far endpoint's outgoing ridge). Still emit the
                    // leftover spoke C++ walks off the stuck child.
                    self.emit_leftover_spoke(tid, side, child, time)?;
                }
                return Ok(());
            }
            LeftoverSchedule::CollapseNow => {}
            LeftoverSchedule::ComputeNatural => {
                let tr = self.kt.triangles.get(tid).ok_or_else(|| {
                    crate::error::PropagationError::TopologyInvariant {
                        detail: format!("leftover collapse: missing face {tid:?}"),
                    }
                })?;
                let natural = compute_collapse(&self.kt, tr, time)?;
                if leftover_action(nconst, motion, natural.type_) != LeftoverAction::CollapseNow {
                    return Ok(());
                }
            }
        }
        let v = self.kt.vertices.get_mut(child).ok_or_else(|| {
            crate::error::PropagationError::TopologyInvariant {
                detail: format!("leftover collapse: missing split child {child:?}"),
            }
        })?;
        v.pos_zero = pos;
        v.velocity = Point2::new(0.0, 0.0);
        let tr = self.kt.triangles.get_mut(tid).ok_or_else(|| {
            crate::error::PropagationError::TopologyInvariant {
                detail: format!("leftover collapse: missing face {tid:?} after decide"),
            }
        })?;
        tr.collapse_cache = Some(CollapseSpec::with_edge(
            component,
            CollapseType::ConstraintCollapse,
            time,
            side as i8,
        ));
        Ok(())
    }

    /// DCEL spoke from a stuck 3-const leftover child to the far leftover
    /// endpoint. Does not stop the far vertex or cache a collapse.
    fn emit_leftover_spoke(
        &mut self,
        tid: TriangleId,
        side: usize,
        child: VertexId,
        time: f64,
    ) -> Result<(), SkeletonError> {
        let Some(eid) = self.kt.triangles.get(tid).and_then(|t| t.wavefronts[side]) else {
            return Ok(());
        };
        let e = &self.kt.edges[eid];
        let far = if e.endpoints[0] == child {
            e.endpoints[1]
        } else if e.endpoints[1] == child {
            e.endpoints[0]
        } else {
            return Ok(());
        };
        let start = self.kt.vertices[child].pos_start;
        let end = self.kt.vertices[far].position_at(time);
        let d = start.distance_to(end);
        if !(d > JUNCTION_M) {
            return Ok(());
        }
        let mut dummy = WavefrontVertex::new_kinetic(
            start,
            Point2::new(0.0, 0.0),
            time,
            InfiniteSpeedType::None,
        );
        dummy.pos_start = start;
        let dummy_id = self.kt.vertices.insert(dummy);
        self.kt.stop_vertex_at(dummy_id, time, end)?;
        // Zero-length cap so fill_missing_next does not copy next[1] onto
        // next[0] (that would union this stop with the far vertex's start).
        let mut cap =
            WavefrontVertex::new_kinetic(end, Point2::new(0.0, 0.0), time, InfiniteSpeedType::None);
        cap.pos_start = end;
        let cap_id = self.kt.vertices.insert(cap);
        self.kt.stop_vertex_at(cap_id, time, end)?;
        if let Some(d) = self.kt.vertices.get_mut(dummy_id) {
            d.prev_vertex[0] = Some(child);
            d.next_vertex = [Some(cap_id), Some(cap_id)];
        }
        if let Some(c) = self.kt.vertices.get_mut(child) {
            if c.prev_vertex[1].is_none() {
                c.prev_vertex[1] = Some(dummy_id);
            }
        }
        if let Some(k) = self.kt.vertices.get_mut(cap_id) {
            k.prev_vertex = [Some(dummy_id), Some(dummy_id)];
            k.next_vertex = [Some(cap_id), Some(cap_id)];
        }
        let _ = far;
        Ok(())
    }

    fn scene_gap(&self) -> f64 {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for iv in self.kt.vertices.values() {
            if !iv.is_initial {
                continue;
            }
            min_x = min_x.min(iv.pos_zero.x);
            min_y = min_y.min(iv.pos_zero.y);
            max_x = max_x.max(iv.pos_zero.x);
            max_y = max_y.max(iv.pos_zero.y);
        }
        (max_x - min_x).hypot(max_y - min_y) * 1e-4
    }

    /// C++: a stopped vertex always has `next_vertex` on both sides.
    fn seal_stops(&mut self, time: f64) {
        let stopped: Vec<(VertexId, Point2)> = self
            .kt
            .vertices
            .iter()
            .filter(|(id, v)| {
                self.kt.infinite_vertex != Some(*id)
                    && v.time_stop
                        .is_some_and(|t| (t - time).abs() <= 1e-9 * (1.0 + time.abs()))
                    && v.pos_stop.is_some()
            })
            .map(|(id, v)| (id, v.pos_stop.unwrap()))
            .collect();
        for i in 0..stopped.len() {
            for j in (i + 1)..stopped.len() {
                if stopped[i].1.distance_to(stopped[j].1) <= JUNCTION_M {
                    self.set_next_vertex(stopped[i].0, 1, stopped[j].0, false);
                }
            }
        }
    }

    /// C++ `assert(!has_stopped() ^ !!next_vertex[i])` on both sides.
    fn fill_missing_next(&mut self) {
        let ids: Vec<_> = self.kt.vertices.keys().collect();
        for vid in ids {
            if self.kt.infinite_vertex == Some(vid) {
                continue;
            }
            let (tstop, pstop, next) = {
                let v = &self.kt.vertices[vid];
                if !v.time_stop.is_some_and(|t| t.is_finite()) || v.pos_stop.is_none() {
                    continue;
                }
                (v.time_stop.unwrap(), v.pos_stop.unwrap(), v.next_vertex)
            };
            match next {
                [Some(a), None] => {
                    if let Some(v) = self.kt.vertices.get_mut(vid) {
                        v.next_vertex[1] = Some(a);
                    }
                }
                [None, Some(b)] => {
                    if let Some(v) = self.kt.vertices.get_mut(vid) {
                        v.next_vertex[0] = Some(b);
                    }
                }
                [None, None] => {
                    let mut nv = WavefrontVertex::new_kinetic(
                        pstop,
                        Point2::new(0.0, 0.0),
                        tstop,
                        InfiniteSpeedType::None,
                    );
                    nv.pos_start = pstop;
                    let child = self.kt.vertices.insert(nv);
                    let _ = self.kt.stop_vertex_at(child, tstop, pstop);
                    if let Some(v) = self.kt.vertices.get_mut(vid) {
                        v.next_vertex = [Some(child), Some(child)];
                    }
                    if let Some(c) = self.kt.vertices.get_mut(child) {
                        c.prev_vertex = [Some(vid), Some(vid)];
                        c.next_vertex = [Some(vid), Some(vid)];
                    }
                }
                _ => {}
            }
        }
    }

    /// C++ `make_vertex` after a reflex stop: child is born at the parent
    /// stop and dies at the same-time split neighbour (the 0.40 m ridge).
    fn spawn_child_to_neighbor(&mut self, vid: VertexId, time: f64) {
        let (pstop, inc) = {
            let v = &self.kt.vertices[vid];
            if v.next_vertex != [None, None] {
                return;
            }
            let Some(pstop) = v.pos_stop else { return };
            (pstop, v.incident_edges)
        };
        for eid in inc.into_iter().flatten() {
            let e = &self.kt.edges[eid];
            let other = if e.endpoints[0] == vid {
                e.endpoints[1]
            } else if e.endpoints[1] == vid {
                e.endpoints[0]
            } else {
                continue;
            };
            let (w_init, w_t0, w_start) = {
                let w = &self.kt.vertices[other];
                (w.is_initial, w.time_start, w.pos_start)
            };
            if w_init {
                continue;
            }
            if (w_t0 - time).abs() > 1e-9 * (1.0 + time.abs()) {
                continue;
            }
            let d = pstop.distance_to(w_start);
            if d <= 1e-9 || d > self.scene_gap() {
                continue;
            }
            let ang = self.kt.vertices[vid].angle;
            let mut nv = WavefrontVertex::new_kinetic(
                pstop,
                Point2::new(0.0, 0.0),
                time,
                InfiniteSpeedType::None,
            );
            nv.pos_start = pstop;
            nv.angle = ang;
            nv.incident_edges = inc;
            let child = self.kt.vertices.insert(nv);
            let _ = self.kt.stop_vertex_at(child, time, w_start);
            self.set_next_vertex(vid, 0, child, true);
            self.set_next_vertex(child, 0, other, true);
            break;
        }
    }

    /// C++ `WavefrontVertex::set_next_vertex`.
    fn set_next_vertex(&mut self, v: VertexId, side: usize, next: VertexId, head_to_tail: bool) {
        if let Some(vv) = self.kt.vertices.get_mut(v) {
            if vv.next_vertex[side].is_none() {
                vv.next_vertex[side] = Some(next);
            }
        }
        if head_to_tail {
            if let Some(n) = self.kt.vertices.get_mut(next) {
                if n.prev_vertex[side].is_none() {
                    n.prev_vertex[side] = Some(v);
                }
            }
        } else if let Some(n) = self.kt.vertices.get_mut(next) {
            if n.next_vertex[1 - side].is_none() {
                n.next_vertex[1 - side] = Some(v);
            }
        }
    }

    /// C++ `va.pos_stop == vb.pos_stop`. A degenerate newborn otherwise
    /// dies in place and drops the ridge to its partner (lake 31.7 m).
    fn stop_collapse_pair(
        &mut self,
        va: VertexId,
        vb: VertexId,
        time: f64,
    ) -> Result<(), SkeletonError> {
        self.kt.stop_vertex(va, time)?;
        self.kt.stop_vertex(vb, time)?;
        let (pa, pb, a_deg, b_deg) = {
            let a = &self.kt.vertices[va];
            let b = &self.kt.vertices[vb];
            let pa = a.pos_stop.unwrap_or_else(|| a.position_at(time));
            let pb = b.pos_stop.unwrap_or_else(|| b.position_at(time));
            (
                pa,
                pb,
                a.pos_stop
                    .is_some_and(|p| p.distance_to(a.pos_start) <= 1e-3),
                b.pos_stop
                    .is_some_and(|p| p.distance_to(b.pos_start) <= 1e-3),
            )
        };
        if pa.distance_to(pb) <= JUNCTION_M {
            self.set_next_vertex(va, 1, vb, false);
            return Ok(());
        }
        if a_deg && !b_deg {
            self.kt.stop_vertex_at(va, time, pb)?;
            self.set_next_vertex(va, 1, vb, false);
        } else if b_deg && !a_deg {
            self.kt.stop_vertex_at(vb, time, pa)?;
            self.set_next_vertex(vb, 1, va, false);
        }
        Ok(())
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
            if guard > 10_000 || id == start {
                break;
            }
            if self.kt.infinite_vertex == self.kt.triangles[id].vertices[idx] {
                break;
            }
            self.kt.set_vertex(id, idx, new_v);
            if self.kt.triangles[id].live() {
                self.modified(id, false)?;
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
        do_flip(&mut self.kt, Some(&mut self.queue), tid, side)?;
        self.mark_adjacent_update(tid)?;
        self.mark_adjacent_update(ntid)?;
        Ok(())
    }

    fn handle_inf_opposing(
        &mut self,
        mut tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};

        let time = spec.time;
        let n_const = self.kt.triangles[tid]
            .wavefronts
            .iter()
            .filter(|w| w.is_some())
            .count();
        let n_fast = self.kt.triangles[tid]
            .vertices
            .iter()
            .flatten()
            .filter(|v| self.kt.vertices[**v].infinite_speed != InfiniteSpeedType::None)
            .count();

        if n_const == 3 {
            let mut p = None;
            for i in 0..3 {
                let v = self.kt.triangles[tid].vertices[i].unwrap();
                if self.kt.vertices[v].infinite_speed == InfiniteSpeedType::None {
                    self.kt.stop_vertex(v, time)?;
                    if p.is_none() {
                        p = self.kt.vertices[v].pos_stop;
                    }
                }
            }
            if let Some(pos) = p {
                for i in 0..3 {
                    let v = self.kt.triangles[tid].vertices[i].unwrap();
                    if self.kt.vertices[v].infinite_speed != InfiniteSpeedType::None {
                        self.kt.stop_vertex_at(v, time, pos)?;
                    }
                }
            }
            for s in 0..3 {
                let (a, b) = {
                    let t = &self.kt.triangles[tid];
                    (t.vertices[s].unwrap(), t.vertices[cw(s)].unwrap())
                };
                self.set_next_vertex(a, 0, b, false);
                if let Some(eid) = self.kt.triangles[tid].wavefronts[s] {
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

        let Some(fast_idx) = (0..3).find(|&i| {
            self.kt.triangles[tid].vertices[i]
                .and_then(|v| self.kt.vertices.get(v))
                .map(|v| v.infinite_speed != InfiniteSpeedType::None)
                .unwrap_or(false)
        }) else {
            return self.kill_triangle(tid);
        };

        let v_fast = self.kt.triangles[tid].vertices[fast_idx].unwrap();
        if self.kt.infinite_vertex == Some(v_fast) {
            return self.kill_triangle(tid);
        }
        let mut cur = most_around(&self.kt, tid, fast_idx, true);
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > 1000 {
                break;
            }
            let Some(idx) = (0..3).find(|&i| self.kt.triangles[cur.0].vertices[i] == Some(v_fast))
            else {
                break;
            };
            cur.1 = idx;
            let nidx = ccw(idx);
            if self.kt.triangles[cur.0].wavefronts[nidx].is_some() {
                break;
            }
            let Some(nid) = self.kt.triangles[cur.0].neighbors[nidx] else {
                break;
            };
            raw_flip(&mut self.kt, cur.0, nidx, nid)?;
            // C++ inf-opposing: do_raw_flip then modified(n) only.
            self.modified(nid, false)?;
            if let Some(i) = (0..3).find(|&i| self.kt.triangles[cur.0].vertices[i] == Some(v_fast))
            {
                cur = most_around(&self.kt, cur.0, i, true);
            }
        }

        tid = cur.0;
        let vidx = cur.1;
        let v = self.kt.triangles[tid].vertices[vidx].unwrap();
        let v_cw = self.kt.triangles[tid].vertices[cw(vidx)].unwrap();
        let v_ccw = self.kt.triangles[tid].vertices[ccw(vidx)].unwrap();
        let pos = self.kt.vertices[v].position_at(time);
        let pcw = self.kt.vertices[v_cw].position_at(time);
        let pccw = self.kt.vertices[v_ccw].position_at(time);
        let d_cw = pos.distance_to(pcw);
        let d_ccw = pos.distance_to(pccw);

        let (collapse, o) = if self.kt.vertices[v_cw].infinite_speed != InfiniteSpeedType::None {
            (cw(vidx), v_ccw)
        } else if self.kt.vertices[v_ccw].infinite_speed != InfiniteSpeedType::None {
            (ccw(vidx), v_cw)
        } else if (d_cw - d_ccw).abs() <= 1e-12 * (1.0 + d_cw) {
            // Equal spokes: stop both, spoke-collapse the remaining neighbor.
            self.kt.stop_vertex(v_cw, time)?;
            self.kt.stop_vertex(v_ccw, time)?;
            let p = self.kt.vertices[v_cw].pos_stop.unwrap_or(pcw);
            self.kt.stop_vertex_at(v, time, p)?;
            self.set_next_vertex(v, 0, v_cw, false);
            self.set_next_vertex(v, 1, v_ccw, false);
            for s in [cw(vidx), ccw(vidx)] {
                if let Some(eid) = self.kt.triangles[tid].wavefronts[s] {
                    if let Some(e) = self.kt.edges.get_mut(eid) {
                        e.dead = true;
                    }
                }
            }
            if let Some(nid) = self.kt.triangles[tid].neighbors[vidx] {
                let idx_in_n = (0..3)
                    .find(|&s| self.kt.triangles[nid].neighbors[s] == Some(tid))
                    .ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
                        detail: "opposing spoke: backref".into(),
                    })?;
                if let Some(tr) = self.kt.triangles.get_mut(tid) {
                    tr.neighbors[vidx] = None;
                }
                if let Some(n) = self.kt.triangles.get_mut(nid) {
                    n.neighbors[idx_in_n] = None;
                }
                self.spoke_collapse_part2(nid, idx_in_n, time)?;
            }
            if let Some(tr) = self.kt.triangles.get_mut(tid) {
                tr.set_dying();
            }
            self.queue.needs_dropping(self.kt.triangles[tid].tidx)?;
            if let Some(t) = self.kt.triangles.get_mut(tid) {
                t.set_dead();
            }
            return Ok(());
        } else if d_cw < d_ccw {
            (ccw(vidx), v_cw)
        } else {
            (cw(vidx), v_ccw)
        };

        self.kt.stop_vertex(o, time)?;
        let p = self.kt.vertices[o]
            .pos_stop
            .unwrap_or_else(|| self.kt.vertices[o].position_at(time));
        self.kt.stop_vertex_at(v, time, p)?;
        self.set_next_vertex(v, if o == v_ccw { 1 } else { 0 }, o, false);
        let (left_sl, right_sl) = {
            let t = &self.kt.triangles[tid];
            (
                t.wavefronts[cw(collapse)].map(|e| self.kt.edges[e].supporting_line),
                t.wavefronts[ccw(collapse)].map(|e| self.kt.edges[e].supporting_line),
            )
        };
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.set_dying();
        }
        let receivers = move_constraints_to_neighbor(&mut self.kt, tid, collapse)?;
        for ntid in receivers {
            if self.kt.triangles[ntid].live() {
                self.queue.needs_update(self.kt.triangles[ntid].tidx)?;
            }
        }
        self.constraint_collapse_part2(tid, collapse, time, left_sl, right_sl)?;
        let _ = n_fast;
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
        do_flip(&mut self.kt, Some(&mut self.queue), tid, side)?;
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
    compute_collapse(kt, t, now)
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
        let spec = compute_collapse(kt, &kt.triangles[tid], now)?;
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
    let keep: Option<Vec<bool>> = interior_filter.map(|f| {
        let mut k = vec![false; basic.triangles.len()];
        for &ti in f {
            if ti < k.len() {
                k[ti] = true;
            }
        }
        k
    });
    let mut tri_map: Vec<Option<TriangleId>> = vec![None; basic.triangles.len()];
    let mut tid_to_ti: std::collections::HashMap<TriangleId, usize> =
        std::collections::HashMap::new();
    for (ti, tri) in basic.triangles.iter().enumerate() {
        if let Some(ref k) = keep {
            if !k[ti] {
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
        tid_to_ti.insert(tid, ti);
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

    if restrict < 0 && interior_filter.is_none() {
        attach_unbounded_hull(&mut kt, basic, &tri_map, input);
    }

    // Wavefront edges: one per constrained triangle side, oriented so the left
    // normal points into the triangle (toward the opposite vertex).
    let tids: Vec<_> = kt.triangles.keys().collect();
    for &tid in &tids {
        for s in 0..3 {
            // Skip if already has neighbor (unconstrained) or wavefront.
            {
                let t = &kt.triangles[tid];
                if t.neighbors[s].is_some() || t.wavefronts[s].is_some() {
                    continue;
                }
            }
            let source_edge = tid_to_ti
                .get(&tid)
                .and_then(|&ti| basic.constraints[ti][s])
                .or_else(|| {
                    let t = &kt.triangles[tid];
                    let va = t.vertices[ccw(s)]?;
                    let vb = t.vertices[cw(s)]?;
                    input_edge_between(input, &kt, va, vb)
                });
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
            let vc_inf = kt.infinite_vertex == Some(vc);
            let mut pa = kt.vertices[va].pos_zero;
            let mut pb = kt.vertices[vb].pos_zero;
            let pc = kt.vertices[vc].pos_zero;
            // Ensure left-of pa→pb contains pc. Infinite opposite: keep
            // stored order (ccw→cw), which already faces the unbounded side.
            let cross = (pb.x - pa.x) * (pc.y - pa.y) - (pb.y - pa.y) * (pc.x - pa.x);
            let (ea, eb) = if vc_inf || cross >= 0.0 {
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

fn edge_on_point_set_hull(pts: &[Point2], a: Point2, b: Point2) -> bool {
    let mut sign = 0i8;
    for p in pts {
        let c = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let s = if c > 1e-12 {
            1
        } else if c < -1e-12 {
            -1
        } else {
            0
        };
        if s == 0 {
            continue;
        }
        if sign == 0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    true
}

fn input_edge_between(
    input: &Pslg,
    kt: &KineticTriangulation,
    va: VertexId,
    vb: VertexId,
) -> Option<u32> {
    let ia = kt.vertices.get(va)?.source_input_vertex? as usize;
    let ib = kt.vertices.get(vb)?.source_input_vertex? as usize;
    input
        .edges()
        .iter()
        .position(|e| {
            let (a, b) = e.normalized_endpoints();
            (a == ia && b == ib) || (a == ib && b == ia)
        })
        .map(|i| i as u32)
}

/// One unbounded triangle per unique CDT hull edge + a shared infinite vertex.
/// C++ `initialize_tds` copies infinite faces from the CDT.
fn attach_unbounded_hull(
    kt: &mut KineticTriangulation,
    basic: &crate::triangulation::basic::BasicTriangulation,
    tri_map: &[Option<TriangleId>],
    _input: &Pslg,
) {
    use crate::triangulation::kinetic::{ccw, cw};

    // Colinear CDT twins share endpoints and both lack a neighbor — not hull.
    let mut seen: Vec<(u32, u32, TriangleId, usize, VertexId, VertexId, bool)> = Vec::new();
    for (ti, tid) in tri_map.iter().enumerate() {
        let Some(tid) = *tid else { continue };
        for s in 0..3 {
            if kt.triangles[tid].neighbors[s].is_some() {
                continue;
            }
            let t = &kt.triangles[tid];
            let Some(a) = t.vertices[ccw(s)] else {
                continue;
            };
            let Some(b) = t.vertices[cw(s)] else { continue };
            let (ia, ib) = match (
                kt.vertices[a].source_input_vertex,
                kt.vertices[b].source_input_vertex,
            ) {
                (Some(ia), Some(ib)) => (ia.min(ib), ia.max(ib)),
                _ => continue,
            };
            let constrained = basic.constraints[ti][s].is_some();
            if let Some(idx) = seen.iter().position(|e| e.0 == ia && e.1 == ib) {
                // Two unconstrained sides, same endpoints: colinear CDT twins.
                // Two constrained sides are interior/exterior of one PSLG edge.
                if !constrained && !seen[idx].6 {
                    let (_, _, otid, os, _, _, _) = seen.swap_remove(idx);
                    kt.set_neighbor(tid, s, Some(otid), os);
                }
            } else {
                seen.push((ia, ib, tid, s, a, b, constrained));
            }
        }
    }

    let mut hull = Vec::new();
    for (_ia, _ib, tid, s, a, b, constrained) in seen {
        let pa = kt.vertices[a].pos_zero;
        let pb = kt.vertices[b].pos_zero;
        if !edge_on_point_set_hull(&basic.vertices, pa, pb) {
            continue;
        }
        hull.push((tid, s, a, b, constrained, kt.triangles[tid].component));
    }
    if hull.is_empty() {
        return;
    }

    let ub_comp = kt
        .triangles
        .values()
        .map(|t| t.component)
        .max()
        .unwrap_or(0)
        + 1;

    let mut inf = WavefrontVertex::new_kinetic(
        Point2::new(0.0, 0.0),
        Point2::new(0.0, 0.0),
        0.0,
        InfiniteSpeedType::None,
    );
    inf.is_initial = true;
    let inf_id = kt.vertices.insert(inf);
    kt.infinite_vertex = Some(inf_id);

    // U: [∞, b, a] so hull side 0 has ccw=b, cw=a (exterior left of b→a).
    let mut ub = Vec::with_capacity(hull.len());
    for (ftid, fs, a, b, constrained, fcomp) in &hull {
        let uid = kt.alloc_triangle(if *constrained { ub_comp } else { *fcomp });
        kt.set_vertex(uid, 0, inf_id);
        kt.set_vertex(uid, 1, *b);
        kt.set_vertex(uid, 2, *a);
        if !*constrained {
            kt.set_neighbor(*ftid, *fs, Some(uid), 0);
        }
        ub.push(uid);
    }

    // Link adjacent unbounded triangles at each hull vertex.
    // U with vertices[1]=V neighbors U' with vertices[2]=V across ∞–V.
    for i in 0..ub.len() {
        let v = kt.triangles[ub[i]].vertices[1];
        let Some(v) = v else { continue };
        for j in 0..ub.len() {
            if i == j {
                continue;
            }
            if kt.triangles[ub[j]].vertices[2] == Some(v) {
                kt.set_neighbor(ub[i], 2, Some(ub[j]), 1);
                break;
            }
        }
    }

    // C++ flood: unconstrained hull leaks one component around the infinite ring.
    let mut changed = true;
    while changed {
        changed = false;
        for &uid in &ub {
            let nbs = kt.triangles[uid].neighbors;
            let c = kt.triangles[uid].component;
            if c == ub_comp {
                continue;
            }
            for s in 0..3 {
                let Some(nid) = nbs[s] else { continue };
                if kt.triangles[nid].component == ub_comp {
                    kt.triangles[nid].component = c;
                    changed = true;
                }
            }
        }
    }
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
            if kt.infinite_vertex == Some(start_v)
                || kt.vertices[start_v].source_input_vertex.is_none()
            {
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
                    Some(n) if n != (tid, i) => most = n,
                    _ => break,
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
            let (_, vel) = wavefront_vertex_motion(sl_l, sl_r, pos, 0.0);
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
                if kt.infinite_vertex == kt.triangles[id].vertices[idx] {
                    break;
                }
                kt.set_vertex(id, idx, nvid);
                walk = step_around(kt, id, idx, true);
                if walk == Some((tid, i)) {
                    break;
                }
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
            let (_, vel) = wavefront_vertex_motion(sla, slb, pos, 0.0);
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

/// C++ `KineticTriangulation::modified`.
fn mark_modified(
    kt: &mut KineticTriangulation,
    queue: Option<&mut EventQueue>,
    tid: TriangleId,
    front: bool,
) -> Result<(), SkeletonError> {
    kt.put_on_check_refinement(tid, front);
    let Some(t) = kt.triangles.get(tid) else {
        return Ok(());
    };
    let tidx = t.tidx;
    let live = t.live();
    // C++ also invalidates the unbounded neighbor(ccw(inf)). That extra
    // needs_update changes event order on weighted hulls (random-2 131→130)
    // while inf-weighted is still a stub. Queue the triangle only.
    if live {
        if let Some(q) = queue {
            q.needs_update(tidx)?;
        }
    }
    Ok(())
}

/// C++ `do_flip`: raw flip, then `modified(t, true)` and `modified(n)`.
fn do_flip(
    kt: &mut KineticTriangulation,
    mut queue: Option<&mut EventQueue>,
    tid: TriangleId,
    side: usize,
) -> Result<(), SkeletonError> {
    let ntid = kt.triangles[tid].neighbors[side].ok_or_else(|| {
        crate::error::PropagationError::TopologyInvariant {
            detail: "do_flip missing neighbor".into(),
        }
    })?;
    raw_flip(kt, tid, side, ntid)?;
    mark_modified(kt, queue.as_deref_mut(), tid, true)?;
    mark_modified(kt, queue, ntid, false)?;
    Ok(())
}

/// C++ `refine_triangulation`: at most one flip for a 1-reflex unconstrained corner.
fn refine_triangulation(
    kt: &mut KineticTriangulation,
    mut queue: Option<&mut EventQueue>,
    tid: TriangleId,
    _time: f64,
) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    let Some(t) = kt.triangles.get(tid) else {
        return Ok(());
    };
    if !t.live() || t.is_unbounded {
        return Ok(());
    }
    let verts = match t.vertices {
        [Some(a), Some(b), Some(c)] => [a, b, c],
        _ => return Ok(()),
    };
    let reflex = [
        kt.vertices
            .get(verts[0])
            .is_some_and(|v| !v.is_convex_or_straight()),
        kt.vertices
            .get(verts[1])
            .is_some_and(|v| !v.is_convex_or_straight()),
        kt.vertices
            .get(verts[2])
            .is_some_and(|v| !v.is_convex_or_straight()),
    ];
    let n_reflex = reflex.iter().filter(|x| **x).count();
    if n_reflex != 1 {
        return Ok(());
    }
    let ri = reflex.iter().position(|&x| x).unwrap();
    if t.wavefronts[ri].is_some() {
        return Ok(());
    }
    let Some(nid) = t.neighbors[ri] else {
        return Ok(());
    };
    let Some(n) = kt.triangles.get(nid) else {
        return Ok(());
    };
    if n.is_unbounded || !n.live() {
        return Ok(());
    }
    let Some(idx_in_n) = (0..3).find(|&s| n.neighbors[s] == Some(tid)) else {
        return Ok(());
    };
    let Some(o) = n.vertices[idx_in_n] else {
        return Ok(());
    };
    let v = verts[ri];
    let va = verts[ccw(ri)];
    let vb = verts[cw(ri)];

    // C++: if a quadrilateral corner is reflex-or-straight, do not flip.
    // C++ asserts all four vertices have both wavefronts; missing → do not flip.
    if !all_quad_wavefronts_present(kt, v, va, vb, o) {
        return Ok(());
    }
    if adjacent_wavefront_is_reflex_or_straight(kt, v, va, vb, o) {
        return Ok(());
    }

    raw_flip(kt, tid, ri, nid)?;
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
        let _ = raw_flip(kt, tid, ri, nid);
        return Ok(());
    }
    mark_modified(kt, queue.as_deref_mut(), tid, true)?;
    mark_modified(kt, queue, nid, false)?;
    Ok(())
}

fn all_quad_wavefronts_present(
    kt: &KineticTriangulation,
    v: crate::wavefront::vertex::VertexId,
    va: crate::wavefront::vertex::VertexId,
    vb: crate::wavefront::vertex::VertexId,
    o: crate::wavefront::vertex::VertexId,
) -> bool {
    [v, va, vb, o].iter().all(|id| {
        kt.vertices.get(*id).is_some_and(|vert| {
            vert.incident_edges[0].is_some() && vert.incident_edges[1].is_some()
        })
    })
}

fn adjacent_wavefront_is_reflex_or_straight(
    kt: &KineticTriangulation,
    v: crate::wavefront::vertex::VertexId,
    va: crate::wavefront::vertex::VertexId,
    vb: crate::wavefront::vertex::VertexId,
    o: crate::wavefront::vertex::VertexId,
) -> bool {
    let Some(vv) = kt.vertices.get(v) else {
        return false;
    };
    let Some(oo) = kt.vertices.get(o) else {
        return false;
    };
    if let Some(e) = vv.incident_edges[1] {
        if kt.edges.get(e).is_some_and(|ed| ed.endpoints[1] == va)
            && kt
                .vertices
                .get(va)
                .is_some_and(|x| x.is_reflex_or_straight())
        {
            return true;
        }
    }
    if let Some(e) = vv.incident_edges[0] {
        if kt.edges.get(e).is_some_and(|ed| ed.endpoints[0] == vb)
            && kt
                .vertices
                .get(vb)
                .is_some_and(|x| x.is_reflex_or_straight())
        {
            return true;
        }
    }
    if let Some(e) = oo.incident_edges[0] {
        if kt.edges.get(e).is_some_and(|ed| ed.endpoints[0] == va)
            && kt
                .vertices
                .get(va)
                .is_some_and(|x| x.is_reflex_or_straight())
        {
            return true;
        }
    }
    if let Some(e) = oo.incident_edges[1] {
        if kt.edges.get(e).is_some_and(|ed| ed.endpoints[1] == vb)
            && kt
                .vertices
                .get(vb)
                .is_some_and(|x| x.is_reflex_or_straight())
        {
            return true;
        }
    }
    false
}

fn process_check_refinement_queue(
    kt: &mut KineticTriangulation,
    mut queue: Option<&mut EventQueue>,
    time: f64,
) -> Result<(), SkeletonError> {
    let mut guard = 0u32;
    while let Some(tid) = kt.check_refinement_pop() {
        guard += 1;
        if guard > 100_000 {
            return Err(crate::error::PropagationError::NonAdvancingLoop {
                component: kt
                    .triangles
                    .get(tid)
                    .map(|t| t.component as usize)
                    .unwrap_or(0),
                time,
            }
            .into());
        }
        refine_triangulation(kt, queue.as_deref_mut(), tid, time)?;
    }
    Ok(())
}

fn refine_triangulation_initial(kt: &mut KineticTriangulation) -> Result<(), SkeletonError> {
    // Keep the pre-queue multi-pass at t=0: C++ one-pass+guard drops
    // weighted/srpg_octo0000035-random-2 131→130. Per-event drain uses
    // refine_triangulation (reflex-or-straight guard).
    use crate::triangulation::kinetic::{ccw, cw};
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
                continue;
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

    // C++ do_raw_flip_inner assigns vertices[] directly — not set_vertex,
    // which rewrites wavefront endpoints on sides that are about to move.
    let inf = kt.infinite_vertex;
    {
        let t = kt.triangles.get_mut(tid).unwrap();
        t.vertices[ccw(side)] = Some(o);
        t.is_unbounded = inf.is_some() && t.vertices.iter().any(|x| *x == inf);
        t.invalidate_collapse();
    }
    {
        let n = kt.triangles.get_mut(ntid).unwrap();
        n.vertices[ccw(nidx)] = Some(v);
        n.is_unbounded = inf.is_some() && n.vertices.iter().any(|x| *x == inf);
        n.invalidate_collapse();
    }

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
