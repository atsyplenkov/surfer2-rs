//! Kinetic triangulation arenas and topology helpers.
//!
//! Upstream: `surf/src/KineticTriangle.*`, `KineticTriangulation.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use slotmap::{new_key_type, SlotMap};

use crate::error::PropagationError;
use crate::event::{CollapseSpec, CollapseType};
use crate::geometry::Point2;
use crate::wavefront::edge::{EdgeId, WavefrontEdge};
use crate::wavefront::vertex::{InfiniteSpeedType, VertexId, WavefrontVertex};

new_key_type! { pub struct TriangleId; }

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

/// Authoritative arc emitted when a wavefront vertex stops.
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
        self.tidx_map[tidx as usize] = Some(id);
        id
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
        let (cw_e, ccw_e) = {
            let Some(tr) = self.triangles.get_mut(t) else {
                return;
            };
            tr.vertices[i] = Some(v);
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

    pub fn stop_vertex_at(
        &mut self,
        v: VertexId,
        t: f64,
        pos: Point2,
    ) -> Result<(), PropagationError> {
        let (left_face, right_face, start_t, start_pos) = {
            let vert = self
                .vertices
                .get(v)
                .ok_or_else(|| PropagationError::TopologyInvariant {
                    detail: "stop_vertex: missing vertex".into(),
                })?;
            if vert.is_stopped() {
                return Ok(());
            }
            let mut faces = [0u32, 0u32];
            for s in 0..2 {
                if let Some(eid) = vert.incident_edges[s] {
                    if let Some(e) = self.edges.get(eid) {
                        faces[s] = e.skeleton_face;
                    }
                }
            }
            (
                faces[0],
                faces[1],
                vert.time_start,
                vert.position_at(vert.time_start),
            )
        };
        if let Some(vert) = self.vertices.get_mut(v) {
            vert.stop(t, pos);
        }
        self.arcs.push(ArcRecord {
            start: start_pos,
            end: Some(pos),
            start_time: start_t,
            end_time: Some(t),
            left_face,
            right_face,
            component: 0,
            from_input_edge: false,
        });
        Ok(())
    }

    pub fn stop_vertex(&mut self, v: VertexId, t: f64) -> Result<(), PropagationError> {
        let pos = match self.vertices.get(v) {
            Some(vert) if !vert.is_stopped() => vert.position_at(t),
            _ => return Ok(()),
        };
        self.stop_vertex_at(v, t, pos)
    }

    /// Validate live triangles' local topology.
    pub fn check_invariants(&self, time: f64) -> Result<(), PropagationError> {
        let _ = time;
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
                    let mut ok = false;
                    for s2 in 0..3 {
                        if other.neighbors[s2] == self.triangle_by_tidx(t.tidx) {
                            ok = true;
                            break;
                        }
                    }
                    // Also check by identity
                    let tid = self.triangle_by_tidx(t.tidx);
                    ok = other.neighbors.iter().any(|n| *n == tid);
                    if !ok {
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

/// Fast-kernel collapse computation (subset → full port grows here).
pub fn compute_collapse_fast(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    if let Some(c) = t.collapse_cache {
        return Ok(c);
    }
    if t.is_dead || t.is_dying {
        return Ok(CollapseSpec::never(t.component));
    }

    // Infinite speed check
    let mut inf = InfiniteSpeedType::None;
    for i in 0..3 {
        if let Some(vid) = t.vertices[i] {
            if let Some(v) = kt.vertices.get(vid) {
                match v.infinite_speed {
                    InfiniteSpeedType::Opposing => {
                        inf = InfiniteSpeedType::Opposing;
                        break;
                    }
                    InfiniteSpeedType::Weighted => inf = InfiniteSpeedType::Weighted,
                    InfiniteSpeedType::None => {}
                }
            }
        }
    }
    if inf == InfiniteSpeedType::Opposing {
        return Ok(CollapseSpec::new(
            t.component,
            CollapseType::FaceHasInfinitelyFastVertexOpposing,
            time_now,
        ));
    }
    if inf == InfiniteSpeedType::Weighted {
        // Pick constrained edge with max weight
        let mut best_edge = -1i8;
        let mut best_w = f64::NEG_INFINITY;
        for s in 0..3 {
            if let Some(eid) = t.wavefronts[s] {
                if let Some(e) = kt.edges.get(eid) {
                    let w = e.supporting_line.weight;
                    if w > best_w {
                        best_w = w;
                        best_edge = s as i8;
                    }
                }
            }
        }
        if best_edge >= 0 {
            return Ok(CollapseSpec::with_edge_secondary(
                t.component,
                CollapseType::FaceHasInfinitelyFastVertexWeighted,
                time_now,
                best_edge,
                best_w,
            ));
        }
        return Ok(CollapseSpec::new(
            t.component,
            CollapseType::InvalidEvent,
            time_now,
        ));
    }

    // Count constraints
    let mut n_const = 0;
    let mut const_sides = [false; 3];
    for s in 0..3 {
        if t.wavefronts[s].is_some() {
            const_sides[s] = true;
            n_const += 1;
        }
    }

    if t.is_unbounded {
        return compute_unbounded_fast(kt, t, time_now, &const_sides);
    }

    match n_const {
        3 => {
            let mut tmin = f64::INFINITY;
            for s in 0..3 {
                if let Some(tc) = constraint_collapse_time(kt, t, s, time_now) {
                    tmin = tmin.min(tc);
                }
            }
            if tmin.is_finite() {
                Ok(CollapseSpec::new(
                    t.component,
                    CollapseType::TriangleCollapse,
                    tmin,
                ))
            } else if t
                .vertices
                .iter()
                .flatten()
                .all(|v| kt.vertices.get(*v).map(|x| x.is_stopped()).unwrap_or(false))
            {
                Ok(CollapseSpec::new(
                    t.component,
                    CollapseType::TriangleCollapse,
                    time_now,
                ))
            } else {
                generic_collapse_fast(kt, t, time_now)
            }
        }
        2 => collapse_bounded_constrained_2(kt, t, time_now),
        1 => collapse_bounded_constrained_1(kt, t, time_now),
        0 => compute_flip_event_fast(kt, t, time_now),
        _ => Ok(CollapseSpec::never(t.component)),
    }
}

fn collapse_bounded_constrained_2(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    let mut specs = Vec::new();
    for s in 0..3 {
        if t.wavefronts[s].is_none() {
            continue;
        }
        if let Some(tc) = constraint_collapse_time(kt, t, s, time_now) {
            specs.push(CollapseSpec::with_edge(
                t.component,
                CollapseType::ConstraintCollapse,
                tc,
                s as i8,
            ));
        }
    }
    match specs.len() {
        0 => Ok(CollapseSpec::never(t.component)),
        1 => Ok(specs[0]),
        _ => {
            if (specs[0].time - specs[1].time).abs() <= 1e-15 * (1.0 + specs[0].time.abs()) {
                Ok(CollapseSpec::new(
                    t.component,
                    CollapseType::TriangleCollapse,
                    specs[0].time.min(specs[1].time),
                ))
            } else if specs[0] < specs[1] {
                Ok(specs[0])
            } else {
                Ok(specs[1])
            }
        }
    }
}

fn collapse_bounded_constrained_1(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    use crate::numeric::{determinant_poly_f64, solve_quadratic_f64};

    let c_idx = (0..3).find(|&s| t.wavefronts[s].is_some()).unwrap();
    let edge_collapse = constraint_collapse_time(kt, t, c_idx, time_now).map(|tc| {
        CollapseSpec::with_edge(
            t.component,
            CollapseType::ConstraintCollapse,
            tc,
            c_idx as i8,
        )
    });

    let mut ps = [[0.0; 2]; 3];
    let mut vs = [[0.0; 2]; 3];
    for i in 0..3 {
        let vid = t.vertices[i].unwrap();
        let v = &kt.vertices[vid];
        ps[i] = [v.pos_zero.x, v.pos_zero.y];
        vs[i] = [v.velocity.x, v.velocity.y];
    }
    let det = determinant_poly_f64(ps[0], vs[0], ps[1], vs[1], ps[2], vs[2]);

    if let Some(cand) = edge_collapse {
        if det.degree() < 2 {
            return Ok(cand);
        }
        // accept_collapse_bounded_constrained_1 simplified
        let a = det.c[2];
        let b = det.c[1];
        // derivative 2*a*t + b at collapse
        let d = 2.0 * a * cand.time + b;
        if a < 0.0 || d <= 0.0 {
            return Ok(cand);
        }
        // else fall through to split/flip refine
    }

    // SPLIT_OR_FLIP_REFINE at det root ≥ now
    let split_time = match det.degree() {
        0 => {
            if det.c[0] == 0.0 {
                time_now
            } else {
                return Ok(CollapseSpec::never(t.component));
            }
        }
        1 => {
            if det.c[1] == 0.0 {
                return Ok(CollapseSpec::never(t.component));
            }
            let tc = -det.c[0] / det.c[1];
            if tc < time_now {
                return Ok(CollapseSpec::never(t.component));
            }
            tc
        }
        2 => {
            let roots = solve_quadratic_f64(det.c[2], det.c[1], det.c[0]);
            if roots.count == 0 {
                return Ok(CollapseSpec::never(t.component));
            }
            let cand = if det.c[2] < 0.0 { roots.x1 } else { roots.x0 };
            if cand >= time_now {
                cand
            } else if roots.count == 2 && roots.x1 >= time_now {
                roots.x1
            } else {
                return Ok(CollapseSpec::never(t.component));
            }
        }
        _ => return Ok(CollapseSpec::never(t.component)),
    };
    Ok(CollapseSpec::with_edge(
        t.component,
        CollapseType::SplitOrFlipRefine,
        split_time,
        c_idx as i8,
    ))
}

fn constraint_collapse_time(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    side: usize,
    now: f64,
) -> Option<f64> {
    use crate::event::EdgeCollapseType;
    use crate::wavefront::edge::compute_edge_collapse_f64;
    let eid = t.wavefronts[side]?;
    let e = kt.edges.get(eid)?;
    let v0 = kt.vertices.get(e.endpoints[0])?;
    let v1 = kt.vertices.get(e.endpoints[1])?;
    // Absolute formula uses pos_zero + velocity (linear motions).
    let spec = compute_edge_collapse_f64(v0.pos_zero, v0.velocity, v1.pos_zero, v1.velocity, now);
    match spec.type_ {
        EdgeCollapseType::Future | EdgeCollapseType::Always => Some(spec.time.max(now)),
        _ => None,
    }
}

fn compute_flip_event_fast(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    let could_flip = t.vertices.iter().flatten().any(|&vid| {
        kt.vertices
            .get(vid)
            .map(|v| v.is_reflex_or_straight())
            .unwrap_or(false)
    });
    if !could_flip {
        return Ok(CollapseSpec::never(t.component));
    }
    let result = generic_collapse_fast(kt, t, time_now)?;
    if result.type_ == CollapseType::VertexMovesOverSpoke {
        let i = result.relevant_edge as usize;
        let reflex = t.vertices[i]
            .and_then(|vid| kt.vertices.get(vid))
            .map(|v| v.is_reflex_or_straight())
            .unwrap_or(false);
        if !reflex {
            return Ok(CollapseSpec::never(t.component));
        }
    }
    Ok(result)
}

fn generic_collapse_fast(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    use crate::numeric::{determinant_poly_f64, solve_quadratic_f64};

    let mut ps = [[0.0; 2]; 3];
    let mut vs = [[0.0; 2]; 3];
    for i in 0..3 {
        let vid = t.vertices[i].ok_or_else(|| PropagationError::TopologyInvariant {
            detail: "generic collapse missing vertex".into(),
        })?;
        let v = kt
            .vertices
            .get(vid)
            .ok_or_else(|| PropagationError::TopologyInvariant {
                detail: "generic collapse dangling vertex".into(),
            })?;
        // Position polynomials are relative to t=0: pos_zero + t * vel
        ps[i] = [v.pos_zero.x, v.pos_zero.y];
        vs[i] = [v.velocity.x, v.velocity.y];
    }
    let det = determinant_poly_f64(ps[0], vs[0], ps[1], vs[1], ps[2], vs[2]);
    let collapse_time = match det.degree() {
        0 => {
            if det.c[0] == 0.0 {
                time_now
            } else {
                return Ok(CollapseSpec::never(t.component));
            }
        }
        1 => {
            // a0 + a1 t = 0 → t = -a0/a1
            if det.c[1] == 0.0 {
                return Ok(CollapseSpec::never(t.component));
            }
            let tc = -det.c[0] / det.c[1];
            if tc < time_now {
                return Ok(CollapseSpec::never(t.component));
            }
            tc
        }
        2 => {
            let roots = solve_quadratic_f64(det.c[2], det.c[1], det.c[0]);
            if roots.count == 0 {
                return Ok(CollapseSpec::never(t.component));
            }
            // Upstream: sign of leading coeff selects root
            let sign2 = det.c[2].signum();
            let candidate = if sign2 < 0.0 { roots.x1 } else { roots.x0 };
            if candidate < time_now {
                if roots.count == 2 && roots.x1 >= time_now {
                    roots.x1
                } else {
                    return Ok(CollapseSpec::never(t.component));
                }
            } else {
                candidate
            }
        }
        _ => return Ok(CollapseSpec::never(t.component)),
    };

    // Classify at collapse time
    classify_at_time(kt, t, collapse_time)
}

fn classify_at_time(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time: f64,
) -> Result<CollapseSpec, PropagationError> {
    let mut pts = [Point2::new(0.0, 0.0); 3];
    for i in 0..3 {
        let vid = t.vertices[i].unwrap();
        pts[i] = kt.vertices[vid].position_at(time);
    }
    let mut zero_edges = 0;
    let mut zero_side = 0;
    let mut lengths = [0.0; 3];
    for s in 0..3 {
        let a = pts[ccw(s)];
        let b = pts[cw(s)];
        let len = a.distance_to(b);
        lengths[s] = len;
    }
    let max_l = lengths[0].max(lengths[1]).max(lengths[2]);
    // Fast kernel: det root is not exact; treat a collapsed spoke as zero
    // when it is tiny vs the other sides (C++ uses exact NT == 0).
    let eps = 1e-9 * (1.0 + max_l);
    for s in 0..3 {
        if lengths[s] <= eps {
            zero_edges += 1;
            zero_side = s;
        }
    }
    if zero_edges == 2 {
        zero_edges = 3;
    }
    match zero_edges {
        3 => Ok(CollapseSpec::new(
            t.component,
            CollapseType::TriangleCollapse,
            time,
        )),
        1 => {
            if t.wavefronts[zero_side].is_some() {
                Ok(CollapseSpec::with_edge(
                    t.component,
                    CollapseType::ConstraintCollapse,
                    time,
                    zero_side as i8,
                ))
            } else {
                Ok(CollapseSpec::with_edge(
                    t.component,
                    CollapseType::SpokeCollapse,
                    time,
                    zero_side as i8,
                ))
            }
        }
        0 => {
            // Longest edge
            let mut best = 0;
            for s in 1..3 {
                if lengths[s] > lengths[best] {
                    best = s;
                }
            }
            Ok(CollapseSpec::with_edge_secondary(
                t.component,
                CollapseType::VertexMovesOverSpoke,
                time,
                best as i8,
                lengths[best] * lengths[best],
            ))
        }
        _ => Ok(CollapseSpec::with_edge(
            t.component,
            CollapseType::SplitOrFlipRefine,
            time,
            zero_side as i8,
        )),
    }
}

fn compute_unbounded_fast(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
    _const_sides: &[bool; 3],
) -> Result<CollapseSpec, PropagationError> {
    // Simplified: try constraint collapses; else NEVER
    for s in 0..3 {
        if t.wavefronts[s].is_some() {
            if let Some(tc) = constraint_collapse_time(kt, t, s, time_now) {
                return Ok(CollapseSpec::with_edge(
                    t.component,
                    CollapseType::ConstraintCollapse,
                    tc,
                    s as i8,
                ));
            }
        }
    }
    let _ = kt;
    Ok(CollapseSpec::never(t.component))
}
