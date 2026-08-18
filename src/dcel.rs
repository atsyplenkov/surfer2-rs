//! Safe skeleton DCEL built from authoritative arc records.
//!
//! Upstream: `surf/src/SkeletonDCEL.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.
//!
//! Unlike upstream's late pointer walk, halfedges are emitted from arc
//! records and validated before return.

use std::collections::HashMap;

use crate::error::SkeletonError;
use crate::geometry::{Point2, Ray2, Segment2};
use crate::triangulation::kinetic::KineticTriangulation;
use crate::wavefront::vertex::VertexId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HalfEdgeId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceId(pub u32);

#[derive(Debug, Clone)]
pub struct Node {
    pub point: Option<Point2>,
    pub time: f64,
    pub is_infinite: bool,
}

#[derive(Debug, Clone)]
pub struct HalfEdge {
    pub twin: HalfEdgeId,
    pub next: HalfEdgeId,
    pub prev: HalfEdgeId,
    pub face: FaceId,
    pub origin: NodeId,
    pub geometry: ArcGeometry,
    pub is_input: bool,
}

#[derive(Debug, Clone)]
pub enum ArcGeometry {
    Segment(Segment2),
    Ray(Ray2),
    /// Input edge at time 0.
    Input(Segment2),
}

#[derive(Debug, Clone)]
pub struct Face {
    pub source_input_edge: Option<u32>,
    pub is_beveling: bool,
    pub component: usize,
    pub halfedge: Option<HalfEdgeId>,
}

/// Finite skeleton segment for public iteration (non-input arcs once each).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonSegment {
    pub start: Point2,
    pub end: Point2,
    pub start_time: f64,
    pub end_time: f64,
    pub component: usize,
    pub left_face: usize,
    pub right_face: usize,
    start_node: NodeId,
    end_node: NodeId,
}

impl SkeletonSegment {
    pub fn start_node(self) -> NodeId {
        self.start_node
    }

    pub fn end_node(self) -> NodeId {
        self.end_node
    }
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub nodes: Vec<Node>,
    pub halfedges: Vec<HalfEdge>,
    pub faces: Vec<Face>,
    segments: Vec<SkeletonSegment>,
}

impl Skeleton {
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn halfedges(&self) -> &[HalfEdge] {
        &self.halfedges
    }
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    pub fn finite_segments(&self) -> impl Iterator<Item = SkeletonSegment> + '_ {
        // Bound runaway arcs using only t=0 endpoints (input footprint).
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut any = false;
        for s in &self.segments {
            if s.start_time == 0.0 && s.start.is_finite() {
                any = true;
                min_x = min_x.min(s.start.x);
                min_y = min_y.min(s.start.y);
                max_x = max_x.max(s.start.x);
                max_y = max_y.max(s.start.y);
            }
        }
        let diag = if any {
            (max_x - min_x).hypot(max_y - min_y).max(1.0)
        } else {
            1.0e6
        };
        let max_len = diag * 2.0;
        self.segments.iter().copied().filter(move |s| {
            if !(s.start.is_finite()
                && s.end.is_finite()
                && s.start_time.is_finite()
                && s.end_time.is_finite()
                && s.end_time >= s.start_time
                && s.start_time >= 0.0)
            {
                return false;
            }
            let len = s.start.distance_to(s.end);
            len > 1e-9 && len <= max_len
        })
    }

    pub fn all_finite_segments(&self) -> &[SkeletonSegment] {
        &self.segments
    }

    pub fn node_count(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        for s in self.finite_segments() {
            seen.insert(s.start_node.0);
            seen.insert(s.end_node.0);
        }
        seen.len()
    }

    pub fn segment_count(&self) -> usize {
        self.finite_segments().count()
    }

    pub fn total_finite_length(&self) -> f64 {
        self.finite_segments()
            .map(|s| s.start.distance_to(s.end))
            .sum()
    }

    /// Offset contours at wavefront time `t`.
    pub fn offset(
        &self,
        time: f64,
    ) -> Result<Vec<crate::offset::OffsetPolyline>, crate::error::SkeletonError> {
        crate::offset::offset(self, time)
    }
}

pub fn build_dcel(kt: &KineticTriangulation) -> Result<Skeleton, SkeletonError> {
    let mut uf = EndpointUf::from_links(kt);
    let mut nodes: Vec<Node> = Vec::new();
    let mut root_node: HashMap<EndKey, u32> = HashMap::new();

    let mut faces: Vec<Face> = kt
        .faces
        .iter()
        .map(|f| Face {
            source_input_edge: f.source_input_edge,
            is_beveling: f.is_beveling,
            component: f.component.max(0) as usize,
            halfedge: None,
        })
        .collect();
    if faces.is_empty() {
        faces.push(Face {
            source_input_edge: None,
            is_beveling: false,
            component: 0,
            halfedge: None,
        });
    }
    // Opposite of an emanating input halfedge is not on the skeleton face
    // (C++: only the emanating side sits on the face CCB).
    let n_skel_faces = faces.len();
    let unbounded = FaceId(faces.len() as u32);
    faces.push(Face {
        source_input_edge: None,
        is_beveling: false,
        component: 0,
        halfedge: None,
    });
    let clamp_face = |f: u32| FaceId(f.min((n_skel_faces.saturating_sub(1)) as u32));

    let mut halfedges: Vec<HalfEdge> = Vec::new();
    let mut segments: Vec<SkeletonSegment> = Vec::new();
    let mut face_base: HashMap<u32, u32> = HashMap::new();
    let mut vertex_left_he: HashMap<VertexId, u32> = HashMap::new();

    // One emanating input HE per input face, from the two *initial*
    // kinetic vertices of that edge. Current WavefrontEdge::endpoints are
    // rewritten by splits/collapses and must not be used as t=0 corners.
    let mut face_ends: HashMap<u32, [Option<VertexId>; 2]> = HashMap::new();
    for (id, v) in &kt.vertices {
        if !v.is_initial || v.source_input_vertex.is_some() {
            continue;
        }
        if kt.infinite_vertex == Some(id) {
            continue;
        }
        for side in 0..2 {
            let Some(eid) = v.incident_edges[side] else {
                continue;
            };
            let Some(e) = kt.edges.get(eid) else {
                continue;
            };
            if e.is_beveling {
                continue;
            }
            let slot = face_ends.entry(e.skeleton_face).or_insert([None, None]);
            let idx = 1 - side; // side 1 = origin, side 0 = dest
            if slot[idx].is_none() {
                slot[idx] = Some(id);
            }
        }
    }
    let mut face_ids: Vec<u32> = face_ends.keys().copied().collect();
    face_ids.sort_unstable();
    for face_id in face_ids {
        let ends = face_ends[&face_id];
        let (Some(va), Some(vb)) = (ends[0], ends[1]) else {
            continue;
        };
        let p0 = kt.vertices[va].pos_start;
        let p1 = kt.vertices[vb].pos_start;
        let n0 = ensure_node(
            &mut uf,
            &mut nodes,
            &mut root_node,
            EndKey::start(va),
            p0,
            0.0,
        );
        let n1 = ensure_node(
            &mut uf,
            &mut nodes,
            &mut root_node,
            EndKey::start(vb),
            p1,
            0.0,
        );
        let face = FaceId(face_id);
        let he = halfedges.len() as u32;
        let twin = he + 1;
        let seg = Segment2::new(p0, p1);
        halfedges.push(HalfEdge {
            twin: HalfEdgeId(twin),
            next: HalfEdgeId(he),
            prev: HalfEdgeId(he),
            face,
            origin: n0,
            geometry: ArcGeometry::Input(seg),
            is_input: true,
        });
        halfedges.push(HalfEdge {
            twin: HalfEdgeId(he),
            next: HalfEdgeId(twin),
            prev: HalfEdgeId(twin),
            face: unbounded,
            origin: n1,
            geometry: ArcGeometry::Input(Segment2::new(p1, p0)),
            is_input: true,
        });
        if (face.0 as usize) < faces.len() && faces[face.0 as usize].halfedge.is_none() {
            faces[face.0 as usize].halfedge = Some(HalfEdgeId(he));
        }
        face_base.entry(face.0).or_insert(he);
    }

    for arc in &kt.arcs {
        if arc.from_input_edge {
            continue;
        }
        let left = clamp_face(arc.left_face);
        let right = clamp_face(arc.right_face);

        match (arc.end, arc.end_time) {
            (Some(end), Some(et)) => {
                let (k0, k1) = arc_keys(arc);
                let n0 = ensure_node(
                    &mut uf,
                    &mut nodes,
                    &mut root_node,
                    k0,
                    arc.start,
                    arc.start_time,
                );
                let n1 = ensure_node(&mut uf, &mut nodes, &mut root_node, k1, end, et);
                let he = halfedges.len() as u32;
                let twin = he + 1;
                let seg = Segment2::new(arc.start, end);
                halfedges.push(HalfEdge {
                    twin: HalfEdgeId(twin),
                    next: HalfEdgeId(he),
                    prev: HalfEdgeId(he),
                    face: left,
                    origin: n0,
                    geometry: ArcGeometry::Segment(seg),
                    is_input: false,
                });
                halfedges.push(HalfEdge {
                    twin: HalfEdgeId(he),
                    next: HalfEdgeId(twin),
                    prev: HalfEdgeId(twin),
                    face: right,
                    origin: n1,
                    geometry: ArcGeometry::Segment(Segment2::new(end, arc.start)),
                    is_input: false,
                });
                if let Some(v) = arc.vertex {
                    vertex_left_he.entry(v).or_insert(he);
                }
                if arc.start.distance_to(end) > 0.0 {
                    segments.push(SkeletonSegment {
                        start: arc.start,
                        end,
                        start_time: arc.start_time,
                        end_time: et,
                        component: arc.component.max(0) as usize,
                        left_face: left.0 as usize,
                        right_face: right.0 as usize,
                        start_node: n0,
                        end_node: n1,
                    });
                }
            }
            (None, _) | (_, None) => {
                let n0 = ensure_node(
                    &mut uf,
                    &mut nodes,
                    &mut root_node,
                    arc_keys(arc).0,
                    arc.start,
                    arc.start_time,
                );
                let inf_id = NodeId(nodes.len() as u32);
                nodes.push(Node {
                    point: None,
                    time: f64::INFINITY,
                    is_infinite: true,
                });
                let dir = Point2::new(1.0, 0.0);
                let he = halfedges.len() as u32;
                let twin = he + 1;
                let ray = Ray2 {
                    origin: arc.start,
                    direction: dir,
                };
                halfedges.push(HalfEdge {
                    twin: HalfEdgeId(twin),
                    next: HalfEdgeId(he),
                    prev: HalfEdgeId(he),
                    face: left,
                    origin: n0,
                    geometry: ArcGeometry::Ray(ray),
                    is_input: false,
                });
                halfedges.push(HalfEdge {
                    twin: HalfEdgeId(he),
                    next: HalfEdgeId(twin),
                    prev: HalfEdgeId(twin),
                    face: right,
                    origin: inf_id,
                    geometry: ArcGeometry::Ray(ray),
                    is_input: false,
                });
                if let Some(v) = arc.vertex {
                    vertex_left_he.entry(v).or_insert(he);
                }
            }
        }
    }

    connect_face_cycles(
        kt,
        &mut halfedges,
        &mut faces,
        &vertex_left_he,
        &face_base,
        &nodes,
    )?;

    for (i, he) in halfedges.iter().enumerate() {
        let twin = he.twin.0 as usize;
        if twin >= halfedges.len() || halfedges[twin].twin.0 != i as u32 {
            return Err(SkeletonError::NonManifoldTopology {
                detail: format!("twin reciprocity failed at halfedge {i}"),
            });
        }
    }

    Ok(Skeleton {
        nodes,
        halfedges,
        faces,
        segments,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct EndKey {
    v: u64,
    end: u8, // 0 start, 1 stop
}

impl EndKey {
    fn start(v: VertexId) -> Self {
        Self {
            v: slotmap_key(v),
            end: 0,
        }
    }
    fn stop(v: VertexId) -> Self {
        Self {
            v: slotmap_key(v),
            end: 1,
        }
    }
    fn unique(tag: u64) -> Self {
        Self { v: tag, end: 2 }
    }
}

fn slotmap_key(v: VertexId) -> u64 {
    use slotmap::Key;
    v.data().as_ffi()
}

struct EndpointUf {
    parent: HashMap<EndKey, EndKey>,
    uniq: u64,
}

impl EndpointUf {
    fn from_links(kt: &KineticTriangulation) -> Self {
        let mut uf = Self {
            parent: HashMap::new(),
            uniq: 0,
        };
        for (id, v) in &kt.vertices {
            for side in 0..2 {
                if let Some(n) = v.next_vertex[side] {
                    let nv = &kt.vertices[n];
                    if nv.prev_vertex[side] == Some(id) || nv.next_vertex[1 - side] != Some(id) {
                        // head-to-tail (default if reciprocal next is unset)
                        uf.union(EndKey::stop(id), EndKey::start(n));
                    }
                    if nv.next_vertex[1 - side] == Some(id) {
                        uf.union(EndKey::stop(id), EndKey::stop(n));
                    }
                }
                if let Some(p) = v.prev_vertex[side] {
                    if kt.vertices[p].prev_vertex[1 - side] == Some(id) {
                        uf.union(EndKey::start(id), EndKey::start(p));
                    }
                }
            }
        }
        // Leftover dummy spokes have no next_vertex into the live chain.
        // Join their stop onto a real vertex that died at the same place.
        let quant = |p: Point2| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
        let mut real_at: HashMap<(i64, i64), EndKey> = HashMap::new();
        for (id, v) in &kt.vertices {
            if v.incident_edges == [None, None] && !v.is_initial {
                continue;
            }
            if v.pos_start.is_finite() {
                real_at
                    .entry(quant(v.pos_start))
                    .or_insert(EndKey::start(id));
            }
            if let Some(p) = v.pos_stop {
                if v.time_stop.is_some_and(|t| t.is_finite()) {
                    real_at.entry(quant(p)).or_insert(EndKey::stop(id));
                }
            }
        }
        for (id, v) in &kt.vertices {
            if !(v.incident_edges == [None, None] && !v.is_initial) {
                continue;
            }
            if v.pos_start.is_finite() {
                if let Some(&r) = real_at.get(&quant(v.pos_start)) {
                    uf.union(EndKey::start(id), r);
                }
            }
            if let Some(p) = v.pos_stop {
                if v.time_stop.is_some_and(|t| t.is_finite()) {
                    if let Some(&r) = real_at.get(&quant(p)) {
                        uf.union(EndKey::stop(id), r);
                    }
                }
            }
        }
        uf
    }

    fn find(&mut self, k: EndKey) -> EndKey {
        let p = *self.parent.entry(k).or_insert(k);
        if p != k {
            let r = self.find(p);
            self.parent.insert(k, r);
            r
        } else {
            k
        }
    }

    fn union(&mut self, a: EndKey, b: EndKey) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }

    fn fresh(&mut self) -> EndKey {
        self.uniq += 1;
        EndKey::unique(self.uniq)
    }
}

fn arc_keys(arc: &crate::triangulation::kinetic::ArcRecord) -> (EndKey, EndKey) {
    match arc.vertex {
        Some(v) => (EndKey::start(v), EndKey::stop(v)),
        None => (EndKey::unique(0), EndKey::unique(0)),
    }
}

fn ensure_node(
    uf: &mut EndpointUf,
    nodes: &mut Vec<Node>,
    root_node: &mut HashMap<EndKey, u32>,
    key: EndKey,
    p: Point2,
    time: f64,
) -> NodeId {
    let key = if key.end == 2 && key.v == 0 {
        uf.fresh()
    } else {
        key
    };
    let r = uf.find(key);
    if let Some(&i) = root_node.get(&r) {
        return NodeId(i);
    }
    let id = nodes.len() as u32;
    nodes.push(Node {
        point: Some(p),
        time,
        is_infinite: false,
    });
    root_node.insert(r, id);
    NodeId(id)
}

fn connect_face_cycles(
    kt: &KineticTriangulation,
    halfedges: &mut [HalfEdge],
    faces: &mut [Face],
    vertex_left_he: &HashMap<VertexId, u32>,
    face_base: &HashMap<u32, u32>,
    nodes: &[Node],
) -> Result<(), SkeletonError> {
    let n = halfedges.len();
    if n == 0 {
        return Ok(());
    }
    for i in 0..n {
        halfedges[i].next = HalfEdgeId(i as u32);
        halfedges[i].prev = HalfEdgeId(i as u32);
    }

    // C++ `create_remaining_skeleton_dcel_one_face`: walk next_vertex /
    // prev_vertex from each initial vertex, skipping degenerates.
    let mut walked_faces = std::collections::HashSet::new();
    for (id, v) in &kt.vertices {
        if !v.is_initial || v.is_degenerate {
            continue;
        }
        if kt.infinite_vertex == Some(id) || v.source_input_vertex.is_some() {
            continue;
        }
        let Some(e0) = v.incident_edges[0] else {
            continue;
        };
        let Some(edge) = kt.edges.get(e0) else {
            continue;
        };
        let face = edge.skeleton_face;
        if !walked_faces.insert(face) {
            continue;
        }
        walk_one_face(
            kt,
            halfedges,
            faces,
            vertex_left_he,
            face_base.get(&face).copied(),
            id,
            face,
        );
    }

    pair_remaining_by_dest(halfedges, nodes);
    Ok(())
}

fn vertex_is_degenerate(
    kt: &KineticTriangulation,
    vertex_left_he: &HashMap<VertexId, u32>,
    v: VertexId,
) -> bool {
    let Some(vert) = kt.vertices.get(v) else {
        return true;
    };
    if vert.is_degenerate {
        return true;
    }
    if vertex_left_he.get(&v).is_none() {
        return true;
    }
    if let (Some(ps), Some(ts)) = (vert.pos_stop, vert.time_stop) {
        if ts.is_finite() && ps.distance_to(vert.pos_start) == 0.0 {
            return true;
        }
    }
    false
}

fn walk_one_face(
    kt: &KineticTriangulation,
    halfedges: &mut [HalfEdge],
    faces: &mut [Face],
    vertex_left_he: &HashMap<VertexId, u32>,
    base: Option<u32>,
    start: VertexId,
    face: u32,
) {
    let mut prev_he = base;
    let mut cur_v = start;
    let mut side_of_f = 0usize;
    let max_steps = kt.vertices.len().saturating_mul(2).saturating_add(4);
    let mut steps = 0;

    loop {
        steps += 1;
        if steps > max_steps {
            break;
        }
        if !vertex_is_degenerate(kt, vertex_left_he, cur_v) {
            if let Some(&left) = vertex_left_he.get(&cur_v) {
                let new_he = if side_of_f == 0 {
                    left
                } else {
                    halfedges[left as usize].twin.0
                };
                if (new_he as usize) < halfedges.len() {
                    if let Some(p) = prev_he {
                        if (p as usize) < halfedges.len() {
                            halfedges[p as usize].next = HalfEdgeId(new_he);
                            halfedges[new_he as usize].prev = HalfEdgeId(p);
                        }
                    } else if (face as usize) < faces.len()
                        && faces[face as usize].halfedge.is_none()
                    {
                        faces[face as usize].halfedge = Some(HalfEdgeId(new_he));
                    }
                    prev_he = Some(new_he);
                }
            }
        }

        match next_on_face(kt, vertex_left_he, start, cur_v, side_of_f) {
            FaceWalk::AtBase => {
                if let (Some(b), Some(p)) = (base, prev_he) {
                    if (b as usize) < halfedges.len() && (p as usize) < halfedges.len() {
                        halfedges[p as usize].next = HalfEdgeId(b);
                        halfedges[b as usize].prev = HalfEdgeId(p);
                    }
                }
                break;
            }
            FaceWalk::Looped => {
                if let (Some(p), Some(&left)) = (prev_he, vertex_left_he.get(&start)) {
                    if (p as usize) < halfedges.len() {
                        halfedges[p as usize].next = HalfEdgeId(left);
                        halfedges[left as usize].prev = HalfEdgeId(p);
                    }
                }
                break;
            }
            FaceWalk::Vertex(v, side) => {
                cur_v = v;
                side_of_f = side;
            }
            FaceWalk::Stuck => break,
        }
    }
}

enum FaceWalk {
    Vertex(VertexId, usize),
    AtBase,
    Looped,
    Stuck,
}

fn next_on_face(
    kt: &KineticTriangulation,
    vertex_left_he: &HashMap<VertexId, u32>,
    start: VertexId,
    mut cur_v: VertexId,
    mut side_of_f: usize,
) -> FaceWalk {
    let max_skip = kt.vertices.len().saturating_add(2);
    for _ in 0..max_skip {
        let prev_v = cur_v;
        let Some(prev) = kt.vertices.get(prev_v) else {
            return FaceWalk::Stuck;
        };
        let nxt = if side_of_f == 1 {
            prev.prev_vertex[side_of_f]
        } else {
            prev.next_vertex[side_of_f]
        };

        if nxt.is_none() && side_of_f == 0 {
            // Escaped to infinity: jump via the left wavefront edge.
            let Some(eid) = prev.incident_edges[0] else {
                return FaceWalk::Stuck;
            };
            let Some(e) = kt.edges.get(eid) else {
                return FaceWalk::Stuck;
            };
            cur_v = e.endpoints[0];
            side_of_f = 1;
        } else if nxt.is_none() && side_of_f == 1 {
            return FaceWalk::AtBase;
        } else if nxt == Some(start) {
            return FaceWalk::Looped;
        } else {
            let Some(n) = nxt else {
                return FaceWalk::Stuck;
            };
            let Some(nv) = kt.vertices.get(n) else {
                return FaceWalk::Stuck;
            };
            side_of_f = if nv.prev_vertex[0] == Some(prev_v) {
                0
            } else {
                1
            };
            cur_v = n;
        }

        if !vertex_is_degenerate(kt, vertex_left_he, cur_v) {
            return FaceWalk::Vertex(cur_v, side_of_f);
        }
    }
    FaceWalk::Stuck
}

/// Pair leftover self-loops by (face, dest → origin). Unique at a node, or
/// smallest CCW turn when several halfedges share a vertex on one face.
fn pair_remaining_by_dest(halfedges: &mut [HalfEdge], nodes: &[Node]) {
    let n = halfedges.len();
    let dest: Vec<u32> = halfedges
        .iter()
        .map(|he| halfedges[he.twin.0 as usize].origin.0)
        .collect();

    let mut outgoing: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    let mut incoming: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for i in 0..n {
        if halfedges[i].next.0 != i as u32 {
            continue;
        }
        outgoing
            .entry((halfedges[i].face.0, halfedges[i].origin.0))
            .or_default()
            .push(i);
        incoming
            .entry((halfedges[i].face.0, dest[i]))
            .or_default()
            .push(i);
    }

    let keys: Vec<(u32, u32)> = outgoing.keys().copied().collect();
    for key in keys {
        let outs = match outgoing.get(&key) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => continue,
        };
        let ins = match incoming.get(&key) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => continue,
        };
        let pairs = pair_at_vertex(halfedges, nodes, &dest, key.1, &ins, &outs);
        for (i, j) in pairs {
            if i == j {
                continue;
            }
            if halfedges[i].next.0 == i as u32 && halfedges[j].prev.0 == j as u32 {
                halfedges[i].next = HalfEdgeId(j as u32);
                halfedges[j].prev = HalfEdgeId(i as u32);
            }
        }
    }
}

fn pair_at_vertex(
    halfedges: &[HalfEdge],
    nodes: &[Node],
    dest: &[u32],
    node: u32,
    ins: &[usize],
    outs: &[usize],
) -> Vec<(usize, usize)> {
    if ins.len() == 1 && outs.len() == 1 && ins[0] != outs[0] {
        return vec![(ins[0], outs[0])];
    }
    if ins.len() == 1 && outs.len() == 1 {
        return Vec::new();
    }

    let origin_pt = nodes.get(node as usize).and_then(|n| n.point);
    let angle_of = |he: usize| -> f64 {
        let Some(o) = origin_pt else {
            return he as f64;
        };
        let Some(dpt) = nodes.get(dest[he] as usize).and_then(|n| n.point) else {
            return he as f64;
        };
        (dpt.y - o.y).atan2(dpt.x - o.x)
    };

    let mut unused_out: Vec<usize> = outs.to_vec();
    let mut pairs = Vec::new();
    for &i in ins {
        if unused_out.is_empty() {
            break;
        }
        let twin = halfedges[i].twin.0 as usize;
        let from = if twin < halfedges.len() {
            angle_of(twin)
        } else {
            angle_of(i) + std::f64::consts::PI
        };
        let mut best = 0usize;
        let mut best_d = f64::INFINITY;
        for (k, &j) in unused_out.iter().enumerate() {
            if j == i {
                continue;
            }
            let d = ccw_delta(from, angle_of(j));
            if d < best_d {
                best_d = d;
                best = k;
            }
        }
        if best_d.is_finite() {
            let j = unused_out.swap_remove(best);
            pairs.push((i, j));
        }
    }
    pairs
}

fn ccw_delta(from: f64, to: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut d = to - from;
    if !d.is_finite() {
        return f64::INFINITY;
    }
    while d <= 1e-15 {
        d += two_pi;
    }
    while d > two_pi {
        d -= two_pi;
    }
    d
}
