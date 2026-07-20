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

        self.handle_event(tidx, spec)?;

        let kt = &self.kt;
        self.queue
            .process_pending_updates(|tidx| recompute_collapse(kt, tidx, self.time))?;

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
        let verts: Vec<_> = {
            let t = &self.kt.triangles[tid];
            t.vertices.iter().flatten().copied().collect()
        };
        for v in verts {
            self.kt.stop_vertex(v, spec.time)?;
        }
        self.kill_triangle(tid)?;
        Ok(())
    }

    fn handle_constraint_collapse(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        let side = spec.relevant_edge as usize;
        let (v0, v1) = {
            let t = &self.kt.triangles[tid];
            let eid = t.wavefronts[side].ok_or_else(|| {
                crate::error::PropagationError::InconsistentEvent {
                    triangle: t.tidx as u64,
                    time: spec.time,
                }
            })?;
            let e = &self.kt.edges[eid];
            (e.endpoints[0], e.endpoints[1])
        };
        self.kt.stop_vertex(v0, spec.time)?;
        self.kt.stop_vertex(v1, spec.time)?;
        self.kill_triangle(tid)?;
        // Mark neighbor across other sides for update
        self.mark_adjacent_update(tid)?;
        Ok(())
    }

    fn handle_spoke_collapse(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        let side = spec.relevant_edge as usize;
        let (va, vb) = {
            let t = &self.kt.triangles[tid];
            let a = t.vertices[crate::triangulation::kinetic::ccw(side)].unwrap();
            let b = t.vertices[crate::triangulation::kinetic::cw(side)].unwrap();
            (a, b)
        };
        self.kt.stop_vertex(va, spec.time)?;
        self.kt.stop_vertex(vb, spec.time)?;
        self.kill_triangle(tid)?;
        self.mark_adjacent_update(tid)?;
        Ok(())
    }

    fn handle_split_or_flip(
        &mut self,
        tid: TriangleId,
        spec: CollapseSpec,
    ) -> Result<(), SkeletonError> {
        // Refine: treat as flip for now when spoke; full split port follows handlers.
        self.handle_flip(tid, spec)
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
    let mut kt = KineticTriangulation::new();
    kt.restrict_component = restrict;

    // Faces from input edges
    for (ei, _) in input.edges().iter().enumerate() {
        kt.faces.push(SkeletonFaceMeta {
            source_input_edge: Some(ei as u32),
            is_beveling: false,
            component: 0,
        });
    }

    // Create kinetic vertices for each input vertex (initial guess: average of incident normals)
    let mut input_vertex_ids = Vec::with_capacity(input.vertex_count());
    // Build adjacency of edges per vertex
    let mut incident: Vec<Vec<(usize, bool)>> = vec![vec![]; input.vertex_count()];
    for (ei, e) in input.edges().iter().enumerate() {
        incident[e.vertices[0]].push((ei, true)); // outgoing along edge
        incident[e.vertices[1]].push((ei, false));
    }

    for (vi, p) in input.vertices().iter().enumerate() {
        let velocity = estimate_vertex_velocity(input, &incident[vi], *p);
        let mut v = WavefrontVertex::new_kinetic(*p, velocity, 0.0, InfiniteSpeedType::None);
        v.is_initial = true;
        v.source_input_vertex = Some(vi as u32);
        input_vertex_ids.push(kt.vertices.insert(v));
    }

    // Create wavefront edges
    let mut input_edge_ids = Vec::with_capacity(input.edge_count());
    for (ei, e) in input.edges().iter().enumerate() {
        let u = input.vertices()[e.vertices[0]];
        let v = input.vertices()[e.vertices[1]];
        let sl = SupportingLine::from_segment(u, v, e.weight).ok_or_else(|| {
            crate::error::PropagationError::TopologyInvariant {
                detail: format!("bad supporting line for edge {ei}"),
            }
        })?;
        let we = WavefrontEdge::new(
            [
                input_vertex_ids[e.vertices[0]],
                input_vertex_ids[e.vertices[1]],
            ],
            sl,
            ei as u32,
            Some(ei as u32),
            false,
        );
        let eid = kt.edges.insert(we);
        // Link incident edges on vertices (best-effort: slot 0/1)
        link_vertex_edge(&mut kt, input_vertex_ids[e.vertices[0]], eid);
        link_vertex_edge(&mut kt, input_vertex_ids[e.vertices[1]], eid);
        input_edge_ids.push(eid);
    }

    // Refine velocities from adjacent supporting lines where degree==2
    for vi in 0..input.vertex_count() {
        if incident[vi].len() == 2 {
            let (e0, _) = incident[vi][0];
            let (e1, _) = incident[vi][1];
            let sl0 = kt.edges[input_edge_ids[e0]].supporting_line;
            let sl1 = kt.edges[input_edge_ids[e1]].supporting_line;
            if let Some((pos, vel)) = intersection_velocity(sl0, sl1) {
                let vid = input_vertex_ids[vi];
                if let Some(v) = kt.vertices.get_mut(vid) {
                    v.pos_zero = pos;
                    v.velocity = vel;
                }
            }
        }
    }

    // Create triangles from basic CDT
    let mut tri_map: Vec<Option<TriangleId>> = vec![None; basic.triangles.len()];
    for (ti, tri) in basic.triangles.iter().enumerate() {
        if let Some(filter) = interior_filter {
            if !filter.contains(&ti) {
                continue;
            }
        } else {
            let comp = basic.components[ti];
            match restrict {
                r if r >= 0 => {
                    if comp != Some(r as u32) {
                        continue;
                    }
                }
                -2 => {}
                _ => {}
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

    // Link neighbors and wavefronts
    for (ti, _) in basic.triangles.iter().enumerate() {
        let Some(tid) = tri_map[ti] else { continue };
        for s in 0..3 {
            if let Some(eid) = basic.constraints[ti][s] {
                kt.set_wavefront(tid, s, input_edge_ids[eid as usize]);
            } else if let Some(nb) = basic.neighbors[ti][s] {
                if let Some(ntid) = tri_map[nb] {
                    // Find reciprocal side later; set one way
                    // Find side on neighbor
                    let mut side_b = 0;
                    for s2 in 0..3 {
                        if basic.neighbors[nb][s2] == Some(ti) {
                            side_b = s2;
                            break;
                        }
                    }
                    kt.set_neighbor(tid, s, Some(ntid), side_b);
                } else {
                    // Neighbor not in component — treat as unconstrained hole? skip
                }
            }
        }
    }

    // Fix sides that still violate xor (boundary without constraint in filtered set)
    let tids: Vec<_> = kt.triangles.keys().collect();
    for tid in tids {
        for s in 0..3 {
            let t = &kt.triangles[tid];
            let n = t.neighbors[s].is_some();
            let w = t.wavefronts[s].is_some();
            if !n && !w {
                // Create a dummy bevel-less supporting line from geometric side
                let va = t.vertices[crate::triangulation::kinetic::ccw(s)];
                let vb = t.vertices[crate::triangulation::kinetic::cw(s)];
                if let (Some(a), Some(b)) = (va, vb) {
                    let pa = kt.vertices[a].pos_zero;
                    let pb = kt.vertices[b].pos_zero;
                    if let Some(sl) = SupportingLine::from_segment(pa, pb, 1.0) {
                        let face = kt.faces.len() as u32;
                        kt.faces.push(SkeletonFaceMeta {
                            source_input_edge: None,
                            is_beveling: true,
                            component: t.component,
                        });
                        let eid = kt.edges.insert(WavefrontEdge::new(
                            [a, b],
                            sl,
                            face,
                            None,
                            true,
                        ));
                        kt.set_wavefront(tid, s, eid);
                    }
                }
            } else if n && w {
                // Prefer constraint
                if let Some(tr) = kt.triangles.get_mut(tid) {
                    tr.neighbors[s] = None;
                }
            }
        }
    }

    Ok(kt)
}

fn estimate_vertex_velocity(
    input: &Pslg,
    incident: &[(usize, bool)],
    p: Point2,
) -> Point2 {
    if incident.is_empty() {
        return Point2::new(0.0, 0.0);
    }
    if incident.len() == 1 {
        // Degree one: bevel later; velocity along perpendicular of the single edge
        let (ei, _) = incident[0];
        let e = &input.edges()[ei];
        let u = input.vertices()[e.vertices[0]];
        let v = input.vertices()[e.vertices[1]];
        if let Some(sl) = SupportingLine::from_segment(u, v, e.weight) {
            return sl.normal;
        }
    }
    if incident.len() >= 2 {
        let (e0, _) = incident[0];
        let (e1, _) = incident[1];
        let s0 = edge_sl(input, e0);
        let s1 = edge_sl(input, e1);
        if let (Some(a), Some(b)) = (s0, s1) {
            if let Some((_, vel)) = intersection_velocity(a, b) {
                return vel;
            }
        }
    }
    let _ = p;
    Point2::new(0.0, 0.0)
}

fn edge_sl(input: &Pslg, ei: usize) -> Option<SupportingLine> {
    let e = &input.edges()[ei];
    let u = input.vertices()[e.vertices[0]];
    let v = input.vertices()[e.vertices[1]];
    SupportingLine::from_segment(u, v, e.weight)
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

/// Flip the edge between `tid` side `side` and neighbor `ntid`.
fn raw_flip(
    kt: &mut KineticTriangulation,
    tid: TriangleId,
    side: usize,
    ntid: TriangleId,
) -> Result<(), SkeletonError> {
    // Classic Delaunay-style flip of shared edge.
    let t = kt.triangles[tid].clone();
    let n = kt.triangles[ntid].clone();
    // Shared edge endpoints
    let a = t.vertices[crate::triangulation::kinetic::ccw(side)].unwrap();
    let b = t.vertices[crate::triangulation::kinetic::cw(side)].unwrap();
    let c = t.vertices[side].unwrap(); // opposite in t
                                        // Find opposite in n
    let mut side_n = 0;
    for s in 0..3 {
        if n.neighbors[s] == Some(tid) {
            side_n = s;
            break;
        }
    }
    let d = n.vertices[side_n].unwrap();

    // New triangles: (c, d, b) and (c, a, d) — shared edge c–d
    // Reassign vertices
    // t gets c, d, b with side opposite d being ... keep it simple:
    // t vertices: c, b, d
    // n vertices: c, d, a
    kt.set_vertex(tid, 0, c);
    kt.set_vertex(tid, 1, b);
    kt.set_vertex(tid, 2, d);
    kt.set_vertex(ntid, 0, c);
    kt.set_vertex(ntid, 1, d);
    kt.set_vertex(ntid, 2, a);

    // Clear and rebuild sides as unconstrained neighbors between t and n
    for s in 0..3 {
        if let Some(tr) = kt.triangles.get_mut(tid) {
            tr.neighbors[s] = None;
            tr.wavefronts[s] = None;
        }
        if let Some(tr) = kt.triangles.get_mut(ntid) {
            tr.neighbors[s] = None;
            tr.wavefronts[s] = None;
        }
    }
    // Shared edge between t and n: side opposite the non-shared vertex
    // t=(c,b,d): edge c-b opp d=2, edge b-d opp c=0, edge d-c opp b=1
    // shared c-d is side 1 (opp b)
    kt.set_neighbor(tid, 1, Some(ntid), 2); // n=(c,d,a): edge c-d opp a = side 2

    // Restore outer wavefronts from old by matching endpoints — best effort
    restore_side_from_old(kt, tid, &t);
    restore_side_from_old(kt, ntid, &n);

    let _ = (a, b);
    Ok(())
}

fn restore_side_from_old(
    kt: &mut KineticTriangulation,
    tid: TriangleId,
    old: &crate::triangulation::kinetic::KineticTriangle,
) {
    for s in 0..3 {
        if let Some(eid) = old.wavefronts[s] {
            let e = &kt.edges[eid];
            let ends = e.endpoints;
            // Find side in new triangle with same endpoints
            let tri = &kt.triangles[tid];
            for ns in 0..3 {
                let va = tri.vertices[crate::triangulation::kinetic::ccw(ns)];
                let vb = tri.vertices[crate::triangulation::kinetic::cw(ns)];
                if (va == Some(ends[0]) && vb == Some(ends[1]))
                    || (va == Some(ends[1]) && vb == Some(ends[0]))
                {
                    // only set if empty
                    if tri.neighbors[ns].is_none() && tri.wavefronts[ns].is_none() {
                        let tidx_drop = tid;
                        let _ = tidx_drop;
                        // need mut
                    }
                }
            }
            let _ = ends;
        }
    }
    // Second pass with mut
    for s in 0..3 {
        if let Some(eid) = old.wavefronts[s] {
            let ends = kt.edges[eid].endpoints;
            let mut found = None;
            {
                let tri = &kt.triangles[tid];
                for ns in 0..3 {
                    let va = tri.vertices[crate::triangulation::kinetic::ccw(ns)];
                    let vb = tri.vertices[crate::triangulation::kinetic::cw(ns)];
                    if ((va == Some(ends[0]) && vb == Some(ends[1]))
                        || (va == Some(ends[1]) && vb == Some(ends[0])))
                        && tri.neighbors[ns].is_none()
                        && tri.wavefronts[ns].is_none()
                    {
                        found = Some(ns);
                        break;
                    }
                }
            }
            if let Some(ns) = found {
                kt.set_wavefront(tid, ns, eid);
            }
        }
    }
}
