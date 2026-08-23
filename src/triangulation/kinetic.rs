//! Kinetic triangulation arenas and topology helpers.
//!
//! Upstream: `surf/src/KineticTriangle.*`, `KineticTriangulation.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use std::collections::VecDeque;

use slotmap::{new_key_type, SlotMap};

use crate::error::PropagationError;
use crate::event::CollapseSpec;
use crate::geometry::{scene_extent, Point2};
use crate::wavefront::edge::{EdgeId, WavefrontEdge};
use crate::wavefront::vertex::{VertexId, WavefrontVertex};

new_key_type! { pub struct TriangleId; }

/// Face id stored on an arc when that side is not a skeleton face.
pub const UNBOUNDED_FACE: u32 = u32::MAX;

fn faces_from_incident(
    inc: [Option<EdgeId>; 2],
    edges: &SlotMap<EdgeId, WavefrontEdge>,
) -> [u32; 2] {
    let mut faces = [UNBOUNDED_FACE, UNBOUNDED_FACE];
    for s in 0..2 {
        if let Some(eid) = inc[s] {
            if let Some(e) = edges.get(eid) {
                faces[s] = e.skeleton_face;
            }
        }
    }
    faces
}

/// Side index 0..3.
pub type Side = usize;

#[inline]
pub fn cw(i: Side) -> Side {
    (i + 2) % 3
}
#[inline]
pub fn ccw(i: Side) -> Side {
    (i + 1) % 3
}

#[derive(Debug, Clone)]
pub struct KineticTriangle {
    pub vertices: [Option<VertexId>; 3],
    pub neighbors: [Option<TriangleId>; 3],
    pub wavefronts: [Option<EdgeId>; 3],
    pub component: i32,
    pub is_dead: bool,
    pub is_dying: bool,
    pub is_unbounded: bool,
    pub collapse_cache: Option<CollapseSpec>,
    /// Dense index into event queue maps (stable for life of triangle entry).
    pub tidx: u32,
}

impl KineticTriangle {
    pub fn new(component: i32, tidx: u32) -> Self {
        Self {
            vertices: [None, None, None],
            neighbors: [None, None, None],
            wavefronts: [None, None, None],
            component,
            is_dead: false,
            is_dying: false,
            is_unbounded: false,
            collapse_cache: None,
            tidx,
        }
    }

    pub fn invalidate_collapse(&mut self) {
        self.collapse_cache = None;
    }

    pub fn set_dying(&mut self) {
        self.is_dying = true;
        self.invalidate_collapse();
    }

    pub fn set_dead(&mut self) {
        self.is_dead = true;
        self.is_dying = false;
        self.invalidate_collapse();
    }

    pub fn live(&self) -> bool {
        !self.is_dead && !self.is_dying
    }

    /// Exactly one of neighbor[i] or wavefront[i] for each side (when live).
    pub fn check_side_xor(&self) -> Result<(), PropagationError> {
        for i in 0..3 {
            let n = self.neighbors[i].is_some();
            let w = self.wavefronts[i].is_some();
            if n == w {
                return Err(PropagationError::TopologyInvariant {
                    detail: format!(
                        "triangle tidx {}: side {i} neighbor/wavefront xor violated (n={n}, w={w})",
                        self.tidx
                    ),
                });
            }
        }
        Ok(())
    }
}

/// One endpoint of a skeleton arc, named by a kinetic vertex end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArcEnd {
    /// Birth of a kinetic vertex.
    Start(VertexId),
    /// Death of a kinetic vertex.
    Stop(VertexId),
    /// Landing on a still-live vertex's traced trajectory at `time_bits`
    /// (`f64::to_bits`). DCEL splits that vertex's ridge through this node.
    TraceAt { vertex: VertexId, time_bits: u64 },
}

/// Authoritative arc emitted when a wavefront vertex stops, or an
/// output-only spoke between two existing vertex ends.
#[derive(Debug, Clone)]
pub struct ArcRecord {
    pub start: Point2,
    pub end: Option<Point2>, // None → ray
    pub start_time: f64,
    pub end_time: Option<f64>,
    pub left_face: u32,
    pub right_face: u32,
    pub component: i32,
    pub from_input_edge: bool,
    pub start_end: ArcEnd,
    pub end_end: Option<ArcEnd>,
}

impl ArcRecord {
    pub fn traced(
        vertex: VertexId,
        start: Point2,
        end: Option<Point2>,
        start_time: f64,
        end_time: Option<f64>,
        left_face: u32,
        right_face: u32,
        component: i32,
    ) -> Self {
        Self {
            start,
            end,
            start_time,
            end_time,
            left_face,
            right_face,
            component,
            from_input_edge: false,
            start_end: ArcEnd::Start(vertex),
            end_end: end.map(|_| ArcEnd::Stop(vertex)),
        }
    }

    /// Kinetic vertex that traced this ridge piece (including TraceAt splits).
    pub fn traced_vertex(&self) -> Option<VertexId> {
        let v = match self.start_end {
            ArcEnd::Start(v) | ArcEnd::TraceAt { vertex: v, .. } => v,
            ArcEnd::Stop(_) => return None,
        };
        match self.end_end {
            Some(ArcEnd::Stop(w)) if w == v => Some(v),
            Some(ArcEnd::TraceAt { vertex, .. }) if vertex == v => Some(v),
            None => Some(v),
            _ => None,
        }
    }

    /// First (birth) piece of a traced ridge, used as the face-walk left HE.
    pub fn ridge_origin_vertex(&self) -> Option<VertexId> {
        match self.start_end {
            ArcEnd::Start(v) if self.traced_vertex() == Some(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct KineticTriangulation {
    pub triangles: SlotMap<TriangleId, KineticTriangle>,
    pub vertices: SlotMap<VertexId, WavefrontVertex>,
    pub edges: SlotMap<EdgeId, WavefrontEdge>,
    /// Dense tidx → TriangleId
    pub tidx_map: Vec<Option<TriangleId>>,
    pub next_tidx: u32,
    pub infinite_vertex: Option<VertexId>,
    pub arcs: Vec<ArcRecord>,
    pub faces: Vec<SkeletonFaceMeta>,
    pub restrict_component: i32, // -1 seq, -2 sim, >=0 index
    /// C++ `check_refinement` / `tidx_in_check_refinement`.
    pub check_refinement: VecDeque<TriangleId>,
    pub tidx_in_check_refinement: Vec<bool>,
    /// Immutable after wavefront edges are created. Weights never change.
    pub has_nonunit_weight: bool,
}

#[derive(Debug, Clone)]
pub struct SkeletonFaceMeta {
    pub source_input_edge: Option<u32>,
    pub is_beveling: bool,
    pub component: i32,
}

impl KineticTriangulation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc_triangle(&mut self, component: i32) -> TriangleId {
        let tidx = self.next_tidx;
        self.next_tidx += 1;
        let id = self.triangles.insert(KineticTriangle::new(component, tidx));
        if self.tidx_map.len() <= tidx as usize {
            self.tidx_map.resize(tidx as usize + 1, None);
        }
        if self.tidx_in_check_refinement.len() <= tidx as usize {
            self.tidx_in_check_refinement
                .resize(tidx as usize + 1, false);
        }
        self.tidx_map[tidx as usize] = Some(id);
        id
    }

    /// C++ `put_on_check_refinement`. No-op if already queued.
    pub fn put_on_check_refinement(&mut self, tid: TriangleId, front: bool) {
        let Some(t) = self.triangles.get(tid) else {
            return;
        };
        let i = t.tidx as usize;
        if self.tidx_in_check_refinement.len() <= i {
            self.tidx_in_check_refinement.resize(i + 1, false);
        }
        if self.tidx_in_check_refinement[i] {
            return;
        }
        self.tidx_in_check_refinement[i] = true;
        if front {
            self.check_refinement.push_front(tid);
        } else {
            self.check_refinement.push_back(tid);
        }
    }

    /// C++ `check_refinement_pop`.
    pub fn check_refinement_pop(&mut self) -> Option<TriangleId> {
        let tid = self.check_refinement.pop_front()?;
        if let Some(t) = self.triangles.get(tid) {
            let i = t.tidx as usize;
            if i < self.tidx_in_check_refinement.len() {
                self.tidx_in_check_refinement[i] = false;
            }
        }
        Some(tid)
    }

    pub fn triangle_by_tidx(&self, tidx: u32) -> Option<TriangleId> {
        self.tidx_map.get(tidx as usize).copied().flatten()
    }

    pub fn set_neighbor(
        &mut self,
        a: TriangleId,
        side_a: Side,
        b: Option<TriangleId>,
        side_b: Side,
    ) {
        if let Some(ta) = self.triangles.get_mut(a) {
            ta.neighbors[side_a] = b;
            ta.wavefronts[side_a] = None;
            ta.invalidate_collapse();
        }
        if let Some(bid) = b {
            if let Some(tb) = self.triangles.get_mut(bid) {
                tb.neighbors[side_b] = Some(a);
                tb.wavefronts[side_b] = None;
                tb.invalidate_collapse();
            }
        }
    }

    /// Which side of `neighbor` points at triangle `of`.
    pub fn neighbor_side(&self, neighbor: TriangleId, of: TriangleId) -> Option<Side> {
        let n = self.triangles.get(neighbor)?;
        (0..3).find(|&s| n.neighbors[s] == Some(of))
    }

    pub fn set_wavefront(&mut self, t: TriangleId, side: Side, e: EdgeId) {
        if let Some(tr) = self.triangles.get_mut(t) {
            tr.wavefronts[side] = Some(e);
            tr.neighbors[side] = None;
            tr.invalidate_collapse();
        }
        if let Some(edge) = self.edges.get_mut(e) {
            edge.incident_triangle = Some(t);
        }
    }

    pub fn set_vertex(&mut self, t: TriangleId, i: Side, v: VertexId) {
        // C++ KineticTriangle::set_vertex: constrained sides incident to i
        // keep wavefront endpoints [0]=ccw(side), [1]=cw(side).
        let inf = self.infinite_vertex;
        let (cw_e, ccw_e) = {
            let Some(tr) = self.triangles.get_mut(t) else {
                return;
            };
            tr.vertices[i] = Some(v);
            tr.is_unbounded = inf.is_some() && tr.vertices.iter().any(|x| *x == inf);
            tr.invalidate_collapse();
            (tr.wavefronts[cw(i)], tr.wavefronts[ccw(i)])
        };
        if let Some(eid) = cw_e {
            if let Some(e) = self.edges.get_mut(eid) {
                e.endpoints[0] = v;
            }
        }
        if let Some(eid) = ccw_e {
            if let Some(e) = self.edges.get_mut(eid) {
                e.endpoints[1] = v;
            }
        }
    }

    /// Axis-aligned bbox diagonal of the input footprint (original PSLG
    /// vertices at t=0). Unbounded-hull extras are not input.
    pub(crate) fn footprint_extent(&self) -> f64 {
        scene_extent(
            self.vertices
                .values()
                .filter(|v| v.source_input_vertex.is_some())
                .map(|v| v.pos_zero),
        )
    }

    /// Component of `v`'s incident kinetic triangle, or 0 if none.
    pub(crate) fn vertex_component(&self, v: VertexId) -> i32 {
        if let Some(vert) = self.vertices.get(v) {
            for side in 0..2 {
                if let Some(eid) = vert.incident_edges[side] {
                    if let Some(tid) = self.edges.get(eid).and_then(|e| e.incident_triangle) {
                        if let Some(t) = self.triangles.get(tid) {
                            return t.component;
                        }
                    }
                }
            }
        }
        for t in self.triangles.values() {
            if t.live() && t.vertices.iter().any(|&x| x == Some(v)) {
                return t.component;
            }
        }
        0
    }

    pub fn stop_vertex_at(
        &mut self,
        v: VertexId,
        t: f64,
        pos: Point2,
    ) -> Result<(), PropagationError> {
        if self.infinite_vertex == Some(v) {
            return Ok(());
        }
        let Some(vert) = self.vertices.get(v) else {
            return Err(PropagationError::TopologyInvariant {
                detail: "stop_vertex: missing vertex".into(),
            });
        };
        let start_pos = vert.pos_start;
        let start_t = vert.time_start;
        let stopped = vert.is_stopped();
        let amend = stopped
            && vert
                .time_stop
                .is_some_and(|ts| (ts - t).abs() <= 1e-9 * (1.0 + t.abs()))
            && vert
                .pos_stop
                .is_some_and(|p| p.distance_to(start_pos) <= 1e-3)
            && pos.distance_to(start_pos) > 1e-3;
        let inc = vert.incident_edges;
        if stopped {
            if amend {
                self.vertices[v].pos_stop = Some(pos);
                if let Some(arc) = self
                    .arcs
                    .iter_mut()
                    .rev()
                    .find(|a| a.traced_vertex() == Some(v))
                {
                    arc.end = Some(pos);
                    arc.end_time = Some(t);
                } else {
                    let faces = faces_from_incident(inc, &self.edges);
                    self.arcs.push(ArcRecord::traced(
                        v,
                        start_pos,
                        Some(pos),
                        start_t,
                        Some(t),
                        faces[0],
                        faces[1],
                        self.vertex_component(v),
                    ));
                }
            }
            return Ok(());
        }
        let faces = faces_from_incident(inc, &self.edges);
        let (left_face, right_face) = (faces[0], faces[1]);
        if let Some(vert) = self.vertices.get_mut(v) {
            vert.stop(t, pos);
        }
        self.arcs.push(ArcRecord::traced(
            v,
            start_pos,
            Some(pos),
            start_t,
            Some(t),
            left_face,
            right_face,
            self.vertex_component(v),
        ));
        Ok(())
    }

    /// Output-only spoke between two existing vertex ends. Does not insert
    /// kinetic vertices or rewrite `next_vertex` / `prev_vertex`.
    ///
    /// `left_face` / `right_face` are the two bounded faces adjacent to the
    /// spoke (typically the child's two incident wavefront faces). Using
    /// `UNBOUNDED_FACE` for the right side leaves a TraceAt landing that
    /// cannot join reciprocal bounded cycles.
    pub fn emit_spoke(
        &mut self,
        from: VertexId,
        to: VertexId,
        time: f64,
        min_len: f64,
        left_face: u32,
        right_face: u32,
    ) -> Result<(), PropagationError> {
        let from_v =
            self.vertices
                .get(from)
                .ok_or_else(|| PropagationError::TopologyInvariant {
                    detail: format!("emit_spoke: missing start vertex {from:?}"),
                })?;
        let to_v = self
            .vertices
            .get(to)
            .ok_or_else(|| PropagationError::TopologyInvariant {
                detail: format!("emit_spoke: missing end vertex {to:?}"),
            })?;
        let start = from_v.pos_start;
        let end = to_v.position_at(time);
        if !(start.distance_to(end) > min_len) {
            return Ok(());
        }
        let end_id = if to_v.is_stopped() {
            ArcEnd::Stop(to)
        } else if time.to_bits() == to_v.time_start.to_bits() {
            ArcEnd::Start(to)
        } else {
            ArcEnd::TraceAt {
                vertex: to,
                time_bits: time.to_bits(),
            }
        };
        let start_time = from_v.time_start;
        let component = self.vertex_component(from);
        self.arcs.push(ArcRecord {
            start,
            end: Some(end),
            start_time,
            end_time: Some(time),
            left_face,
            right_face,
            component,
            from_input_edge: false,
            start_end: ArcEnd::Start(from),
            end_end: Some(end_id),
        });
        Ok(())
    }

    pub fn stop_vertex(&mut self, v: VertexId, t: f64) -> Result<(), PropagationError> {
        if self.infinite_vertex == Some(v) {
            return Ok(());
        }
        let pos = match self.vertices.get(v) {
            Some(vert) if !vert.is_stopped() => vert.position_at(t),
            _ => return Ok(()),
        };
        self.stop_vertex_at(v, t, pos)
    }

    /// Validate live triangles' local topology.
    pub fn check_invariants(&self, _time: f64) -> Result<(), PropagationError> {
        for (_, t) in &self.triangles {
            if !t.live() {
                continue;
            }
            for i in 0..3 {
                if t.vertices[i].is_none() && !t.is_unbounded {
                    return Err(PropagationError::TopologyInvariant {
                        detail: format!("live triangle {} missing vertex {i}", t.tidx),
                    });
                }
            }
            t.check_side_xor()?;
            // Reciprocal neighbors
            for s in 0..3 {
                if let Some(nb) = t.neighbors[s] {
                    let other = self.triangles.get(nb).ok_or_else(|| {
                        PropagationError::TopologyInvariant {
                            detail: "neighbor handle dangling".into(),
                        }
                    })?;
                    let tid = self.triangle_by_tidx(t.tidx);
                    if !other.neighbors.iter().any(|n| *n == tid) {
                        return Err(PropagationError::TopologyInvariant {
                            detail: format!("neighbor reciprocity failed tidx {}", t.tidx),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}
