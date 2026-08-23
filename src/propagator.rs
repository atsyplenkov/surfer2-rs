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
use crate::triangulation::collapse::compute_collapse;
use crate::triangulation::refine::{apply_event_refinement, do_flip, mark_modified, raw_flip};
use crate::triangulation::{initialize_kinetic, KineticTriangulation, TriangleId};
use crate::wavefront::edge::WavefrontEdge;
use crate::wavefront::split_leftover::{
    classify_successor_motion, leftover_action, leftover_schedule, LeftoverAction,
    LeftoverSchedule, SuccessorMotion,
};
use crate::wavefront::supporting_line::{wavefront_vertex_motion, SupportingLine};
use crate::wavefront::vertex::{
    infinite_speed_type, vertex_angle_from_lines, InfiniteSpeedType, VertexId, WavefrontVertex,
};
use crate::{ComponentMode, Options};

/// UTM / metre-valued inputs: 5 mm snap. Unit GraphML uses near-exact
/// equality so C++ double's ~1e-11 hub chords are not merged.
const JUNCTION_EPS_METRE: f64 = 5e-3;
const JUNCTION_EPS_UNIT: f64 = 1e-15;
const JUNCTION_METRE_SCALE: f64 = 1e3;

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
    /// Scale-aware stand-in for C++ exact `pos_stop` equality.
    junction_eps: f64,
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
            junction_eps: junction_eps_from_coords(input.vertices().iter().copied()),
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
            junction_eps: junction_eps_from_coords(pslg.vertices().iter().copied()),
        })
    }

    fn same_stop(&self, a: Point2, b: Point2) -> bool {
        a.distance_to(b) <= self.junction_eps
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
                let faces = [
                    v.incident_edges[0]
                        .and_then(|e| self.kt.edges.get(e).map(|ed| ed.skeleton_face))
                        .unwrap_or(crate::triangulation::kinetic::UNBOUNDED_FACE),
                    v.incident_edges[1]
                        .and_then(|e| self.kt.edges.get(e).map(|ed| ed.skeleton_face))
                        .unwrap_or(crate::triangulation::kinetic::UNBOUNDED_FACE),
                ];
                self.kt
                    .arcs
                    .push(crate::triangulation::kinetic::ArcRecord::traced(
                        vid,
                        start,
                        None,
                        v.time_start,
                        None,
                        faces[0],
                        faces[1],
                        0,
                    ));
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
            CollapseType::FaceHasInfinitelyFastVertexOpposing
            | CollapseType::FaceHasInfinitelyFastVertexWeighted => {
                self.handle_inf_opposing(tid, spec)?
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
        apply_event_refinement(&mut self.kt, Some(&mut self.queue), spec.type_, spec.time)?;
        Ok(())
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
                if self.same_stop(pa, pb) {
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
                let idx_in_n = self.kt.neighbor_side(nid, tid).ok_or_else(|| {
                    crate::error::PropagationError::TopologyInvariant {
                        detail: "triangle collapse: neighbor missing backref".into(),
                    }
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

        self.retire_face(tid)?;
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
        self.stop_collapse_pair(va, vb, spec.time)?;
        self.collapse_side(tid, edge_idx, spec.time)
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

        let idx_in_n = self.kt.neighbor_side(nid, tid).ok_or_else(|| {
            crate::error::PropagationError::TopologyInvariant {
                detail: "spoke collapse: neighbor missing backref".into(),
            }
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

        let (v_opp, va) = {
            let t = &self.kt.triangles[tid];
            (
                t.vertices[edge_idx].unwrap(),
                t.vertices[ccw(edge_idx)].unwrap(),
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
                    let idx_in_n = self.kt.neighbor_side(nid, tid).ok_or_else(|| {
                        crate::error::PropagationError::TopologyInvariant {
                            detail: "spoke part2 complete: backref".into(),
                        }
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
            self.retire_face(tid)?;
            return Ok(());
        }

        self.collapse_side(tid, edge_idx, time)
    }

    /// Collapse this side of this triangle: snapshot supports before moving constraints.
    fn collapse_side(
        &mut self,
        tid: TriangleId,
        edge_idx: usize,
        time: f64,
    ) -> Result<(), SkeletonError> {
        use crate::triangulation::kinetic::{ccw, cw};
        let (left_sl, right_sl) = {
            let t = &self.kt.triangles[tid];
            let left_sl = t.wavefronts[cw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
            let right_sl = t.wavefronts[ccw(edge_idx)].map(|e| self.kt.edges[e].supporting_line);
            (left_sl, right_sl)
        };
        if let Some(tr) = self.kt.triangles.get_mut(tid) {
            tr.set_dying();
        }
        let receivers = move_constraints_to_neighbor(&mut self.kt, tid, edge_idx)?;
        for ntid in receivers {
            if self.kt.triangles[ntid].live() {
                self.queue.needs_update(self.kt.triangles[ntid].tidx)?;
            }
        }
        self.constraint_collapse_part2(tid, edge_idx, time, left_sl, right_sl)
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
            let idx_a = self.kt.neighbor_side(na, tid);
            let idx_b = self.kt.neighbor_side(nb, tid);
            if let (Some(ia), Some(ib)) = (idx_a, idx_b) {
                self.kt.set_neighbor(na, ia, Some(nb), ib);
            }
        } else if let Some(na) = na_id {
            if let Some(ia) = self.kt.neighbor_side(na, tid) {
                if let Some(tr) = self.kt.triangles.get_mut(na) {
                    tr.neighbors[ia] = None;
                    tr.invalidate_collapse();
                }
                if self.kt.triangles[na].live() {
                    self.queue.needs_update(self.kt.triangles[na].tidx)?;
                }
            }
        } else if let Some(nb) = nb_id {
            if let Some(ib) = self.kt.neighbor_side(nb, tid) {
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

        self.retire_face(tid)?;
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
            if let Some(i) = self.kt.neighbor_side(na, tid) {
                self.kt.set_wavefront(na, i, nea);
                self.schedule_leftover_collapse(na, i, nva_id, nva_motion, pos, time)?;
                if self.kt.triangles[na].live() {
                    self.queue.needs_update(self.kt.triangles[na].tidx)?;
                }
            }
        }
        if let Some(nb) = nb_id {
            if let Some(i) = self.kt.neighbor_side(nb, tid) {
                self.kt.set_wavefront(nb, i, neb);
                self.schedule_leftover_collapse(nb, i, nvb_id, nvb_motion, pos, time)?;
                if self.kt.triangles[nb].live() {
                    self.queue.needs_update(self.kt.triangles[nb].tidx)?;
                }
            }
        }

        self.retire_face(tid)?;
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

    /// Output-only spoke from a stuck 3-const leftover child to the far
    /// leftover endpoint. Does not insert kinetic vertices.
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
        // The leftover wavefront edge's face is one side of the spoke; the
        // child's other incident edge supplies the opposite bounded face.
        // Orientation: traveling child→far, put the leftover face on the
        // right so the directed HE child→far lies on the opposite face and
        // can close that face's free end at the child.
        let right_face = e.skeleton_face;
        let left_face = {
            let child_v = &self.kt.vertices[child];
            let mut other = crate::triangulation::kinetic::UNBOUNDED_FACE;
            for s in 0..2 {
                let Some(oid) = child_v.incident_edges[s] else {
                    continue;
                };
                if oid == eid {
                    continue;
                }
                if let Some(oe) = self.kt.edges.get(oid) {
                    other = oe.skeleton_face;
                    break;
                }
            }
            other
        };
        self.kt
            .emit_spoke(child, far, time, self.junction_eps, left_face, right_face)
            .map_err(SkeletonError::from)
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
                if self.same_stop(stopped[i].1, stopped[j].1) {
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
        if self.same_stop(pa, pb) {
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
                mark_modified(&mut self.kt, Some(&mut self.queue), id, false)?;
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
            self.retire_face(tid)?;
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
            self.retire_face(tid)?;
            return Ok(());
        }

        let Some(fast_idx) = (0..3).find(|&i| {
            self.kt.triangles[tid].vertices[i]
                .and_then(|v| self.kt.vertices.get(v))
                .map(|v| v.infinite_speed != InfiniteSpeedType::None)
                .unwrap_or(false)
        }) else {
            return self.retire_face(tid);
        };

        let v_fast = self.kt.triangles[tid].vertices[fast_idx].unwrap();
        if self.kt.infinite_vertex == Some(v_fast) {
            return self.retire_face(tid);
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
            mark_modified(&mut self.kt, Some(&mut self.queue), nid, false)?;
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
                let idx_in_n = self.kt.neighbor_side(nid, tid).ok_or_else(|| {
                    crate::error::PropagationError::TopologyInvariant {
                        detail: "opposing spoke: backref".into(),
                    }
                })?;
                if let Some(tr) = self.kt.triangles.get_mut(tid) {
                    tr.neighbors[vidx] = None;
                }
                if let Some(n) = self.kt.triangles.get_mut(nid) {
                    n.neighbors[idx_in_n] = None;
                }
                self.spoke_collapse_part2(nid, idx_in_n, time)?;
            }
            self.retire_face(tid)?;
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
        self.collapse_side(tid, collapse, time)
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

    /// Dying → drop from queue → dead → clear neighbour back-pointers.
    fn retire_face(&mut self, tid: TriangleId) -> Result<(), SkeletonError> {
        let tidx = {
            let t = self.kt.triangles.get_mut(tid).ok_or_else(|| {
                crate::error::PropagationError::TopologyInvariant {
                    detail: "retire missing triangle".into(),
                }
            })?;
            t.set_dying();
            t.tidx
        };
        self.queue.needs_dropping(tidx)?;
        if let Some(t) = self.kt.triangles.get_mut(tid) {
            t.set_dead();
        }
        let neighbors: Vec<_> = self.kt.triangles[tid]
            .neighbors
            .iter()
            .copied()
            .flatten()
            .collect();
        for nb in neighbors {
            if let Some(s) = self.kt.neighbor_side(nb, tid) {
                if let Some(n) = self.kt.triangles.get_mut(nb) {
                    n.neighbors[s] = None;
                    n.invalidate_collapse();
                    if n.live() {
                        self.queue.needs_update(n.tidx)?;
                    }
                }
            }
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

/// f64 stand-in for C++ exact `pos_stop` equality.
///
/// Based on bounding-box diagonal so a translated unit polygon keeps the
/// near-exact snap. Metre-scale scenes (diagonal ≥ 1 km) use 5 mm.
fn junction_eps_from_coords(pts: impl Iterator<Item = Point2>) -> f64 {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for p in pts {
        any = true;
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    if !any {
        return JUNCTION_EPS_UNIT;
    }
    let extent = (max_x - min_x).hypot(max_y - min_y);
    if extent.is_finite() && extent >= JUNCTION_METRE_SCALE {
        JUNCTION_EPS_METRE
    } else {
        JUNCTION_EPS_UNIT
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

        let nidx = kt.neighbor_side(nid, tid).ok_or_else(|| {
            crate::error::PropagationError::TopologyInvariant {
                detail: "move_constraint: neighbor missing backref".into(),
            }
        })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junction_eps_uses_extent_not_origin() {
        let unit = [Point2::new(-1.5, -0.95), Point2::new(0.2, 0.4)];
        let shifted = [
            Point2::new(-1.5 + 1000.0, -0.95 + 1000.0),
            Point2::new(0.2 + 1000.0, 0.4 + 1000.0),
        ];
        assert_eq!(
            junction_eps_from_coords(unit.into_iter()),
            JUNCTION_EPS_UNIT
        );
        assert_eq!(
            junction_eps_from_coords(shifted.into_iter()),
            JUNCTION_EPS_UNIT
        );
        let utm = [
            Point2::new(535_525.0, 7_023_032.0),
            Point2::new(549_793.0, 7_050_954.0),
        ];
        assert_eq!(
            junction_eps_from_coords(utm.into_iter()),
            JUNCTION_EPS_METRE
        );
    }
}
