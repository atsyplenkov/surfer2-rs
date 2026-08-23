//! Face-cycle construction and DCEL validation.

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use super::{sewing_map, sewing_set};
use crate::dcel::{ArcGeometry, Face, FaceId, HalfEdge, HalfEdgeId, Node, NodeId};
use crate::error::SkeletonError;
use crate::triangulation::KineticTriangulation;
use crate::wavefront::vertex::VertexId;

pub(super) fn connect_face_cycles(
    kt: &KineticTriangulation,
    halfedges: &mut Vec<HalfEdge>,
    faces: &mut [Face],
    vertex_left_he: &HashMap<VertexId, u32>,
    face_base: &HashMap<u32, u32>,
    ridge_pieces: &HashMap<VertexId, Vec<(u64, u32)>>,
    nodes: &[Node],
    unbounded: FaceId,
) -> Result<Vec<String>, SkeletonError> {
    let n = halfedges.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    for i in 0..n {
        halfedges[i].next = HalfEdgeId(i as u32);
        halfedges[i].prev = HalfEdgeId(i as u32);
    }
    link_ridge_pieces(halfedges, ridge_pieces);

    // C++ `create_remaining_skeleton_dcel_one_face`: walk next_vertex /
    // prev_vertex from each initial vertex, skipping degenerates.
    let mut walked_faces: HashSet<u32> = sewing_set(face_base.len());
    let mut walk_failures = Vec::new();
    let mut seeds: Vec<(u32, VertexId)> = Vec::new();
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
        seeds.push((edge.skeleton_face, id));
    }
    // face_base uses Fx hasher (deterministic bucket order, not SipHash-random).
    for &face in face_base.keys() {
        if seeds.iter().any(|(f, _)| *f == face) {
            continue;
        }
        if let Some(start) = seed_vertex_for_face(kt, face) {
            seeds.push((face, start));
        }
    }
    for (face, id) in seeds {
        if !walked_faces.insert(face) {
            continue;
        }
        if let Err(e) = walk_one_face(
            kt,
            halfedges,
            faces,
            vertex_left_he,
            face_base.get(&face).copied(),
            id,
            face,
        ) {
            match e {
                FaceWalkError::MissingVertex { vertex } => {
                    let mut detail =
                        format!("stage=face-walk face={face} endpoint={vertex:?}: missing vertex");
                    if !walk_failures.is_empty() {
                        detail.push_str("; face-walk failures: ");
                        detail.push_str(&walk_failures.join("; "));
                    }
                    return Err(SkeletonError::IncompleteFaceCycle { detail });
                }
                FaceWalkError::GuardExhausted {
                    face,
                    endpoint,
                    side,
                } => walk_failures.push(format!(
                    "stage=face-walk face={face} endpoint={endpoint:?} side={side}: guard exhausted"
                )),
                FaceWalkError::Stuck {
                    face,
                    endpoint,
                    side,
                } => walk_failures.push(format!(
                    "stage=face-walk face={face} endpoint={endpoint:?} side={side}: stuck"
                )),
            }
        }
    }

    let mut dest: Vec<u32> = halfedges
        .iter()
        .map(|he| halfedges[he.twin.0 as usize].origin.0)
        .collect();
    pair_remaining_by_dest(halfedges, nodes, &dest);
    splice_remaining_into_cycles(halfedges, unbounded, &dest);
    attach_free_ends(halfedges, unbounded, &dest);
    // Skip rebuild / ∞-gap when the linear walk already closed every bounded
    // face with no dangling ends; otherwise keep today's repair constructors.
    if !bounded_cycles_look_complete(halfedges, faces, unbounded) {
        rebuild_incomplete_faces(halfedges, nodes, faces, unbounded, &dest);
        close_infinity_gaps(halfedges, faces, unbounded, &mut dest);
    }
    repair_face_halfedges(halfedges, faces, unbounded);
    Ok(walk_failures)
}

/// Cheap O(E) filter: no dangling bounded ends, and each bounded face root
/// starts a fully reciprocal next/prev cycle that covers every halfedge on
/// that face. When true, incomplete-face rebuild and infinity-gap closure are
/// no-ops for the current halfedge set.
fn bounded_cycles_look_complete(
    halfedges: &[HalfEdge],
    faces: &[Face],
    unbounded: FaceId,
) -> bool {
    let n = halfedges.len();
    if n == 0 {
        return true;
    }

    let mut owned = vec![0usize; faces.len()];
    let mut dangling = 0usize;
    for (i, he) in halfedges.iter().enumerate() {
        if he.face == unbounded {
            continue;
        }
        let f = he.face.0 as usize;
        if f < owned.len() {
            owned[f] += 1;
        }
        if he.next.0 == i as u32 || he.prev.0 == i as u32 {
            dangling += 1;
        }
    }
    if dangling > 0 {
        return false;
    }

    let mut claimed = vec![false; n];
    for (fi, face) in faces.iter().enumerate() {
        if fi as u32 == unbounded.0 {
            continue;
        }
        let expect = owned.get(fi).copied().unwrap_or(0);
        if expect == 0 {
            continue;
        }
        let Some(start) = face.halfedge else {
            return false;
        };
        let start = start.0 as usize;
        if start >= n || halfedges[start].face.0 != fi as u32 {
            return false;
        }
        let mut he = start;
        for count in 1..=expect {
            let cur = &halfedges[he];
            let next = cur.next.0 as usize;
            let prev = cur.prev.0 as usize;
            if next >= n || prev >= n || next == he {
                return false;
            }
            if halfedges[next].prev.0 != he as u32 || halfedges[prev].next.0 != he as u32 {
                return false;
            }
            if halfedges[next].face.0 != fi as u32 || claimed[he] {
                return false;
            }
            claimed[he] = true;
            he = next;
            if he == start {
                if count != expect {
                    return false;
                }
                break;
            }
            if count == expect {
                return false;
            }
        }
    }
    for (i, he) in halfedges.iter().enumerate() {
        if he.face != unbounded && !claimed[i] {
            return false;
        }
    }
    true
}

/// Chain TraceAt-split ridge pieces in time order on both sides of the arc.
fn link_ridge_pieces(
    halfedges: &mut [HalfEdge],
    ridge_pieces: &HashMap<VertexId, Vec<(u64, u32)>>,
) {
    for pieces in ridge_pieces.values() {
        if pieces.len() < 2 {
            continue;
        }
        let mut ordered = pieces.clone();
        ordered.sort_by(|a, b| f64::from_bits(a.0).total_cmp(&f64::from_bits(b.0)));
        for w in ordered.windows(2) {
            let a = w[0].1;
            let b = w[1].1;
            link_successors(halfedges, a, b);
            if (a as usize) < halfedges.len() && (b as usize) < halfedges.len() {
                let ta = halfedges[a as usize].twin.0;
                let tb = halfedges[b as usize].twin.0;
                link_successors(halfedges, tb, ta);
            }
        }
    }
}

fn seed_vertex_for_face(kt: &KineticTriangulation, face: u32) -> Option<VertexId> {
    for (id, v) in &kt.vertices {
        if kt.infinite_vertex == Some(id) {
            continue;
        }
        for side in 0..2 {
            if let Some(eid) = v.incident_edges[side] {
                if kt.edges.get(eid).is_some_and(|e| e.skeleton_face == face) {
                    return Some(id);
                }
            }
        }
    }
    None
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
) -> Result<(), FaceWalkError> {
    let mut prev_he = base;
    let mut cur_v = start;
    let mut side_of_f = 0usize;
    let max_steps = kt.vertices.len().saturating_mul(2).saturating_add(4);
    let mut steps = 0;

    loop {
        steps += 1;
        if steps > max_steps {
            if let (Some(b), Some(p)) = (base, prev_he) {
                link_successors(halfedges, p, b);
            }
            return Err(FaceWalkError::GuardExhausted {
                face,
                endpoint: cur_v,
                side: side_of_f,
            });
        }
        if !vertex_is_degenerate(kt, vertex_left_he, cur_v) {
            if let Some(&left) = vertex_left_he.get(&cur_v) {
                let new_he = if side_of_f == 0 {
                    left
                } else {
                    halfedges[left as usize].twin.0
                };
                if (new_he as usize) < halfedges.len() && halfedges[new_he as usize].face.0 == face
                {
                    if let Some(p) = prev_he {
                        link_successors(halfedges, p, new_he);
                    } else if (face as usize) < faces.len()
                        && faces[face as usize].halfedge.is_none()
                    {
                        faces[face as usize].halfedge = Some(HalfEdgeId(new_he));
                    }
                    prev_he = Some(new_he);
                }
            }
        }

        match next_on_face(kt, vertex_left_he, start, cur_v, side_of_f)? {
            FaceWalk::AtBase => {
                if let (Some(b), Some(p)) = (base, prev_he) {
                    link_successors(halfedges, p, b);
                }
                return Ok(());
            }
            FaceWalk::Looped => {
                if let (Some(p), Some(&left)) = (prev_he, vertex_left_he.get(&start)) {
                    link_successors(halfedges, p, left);
                }
                return Ok(());
            }
            FaceWalk::Vertex(v, side) => {
                cur_v = v;
                side_of_f = side;
            }
            FaceWalk::Stuck { endpoint, side } => {
                if let (Some(b), Some(p)) = (base, prev_he) {
                    link_successors(halfedges, p, b);
                }
                return Err(FaceWalkError::Stuck {
                    face,
                    endpoint,
                    side,
                });
            }
        }
    }
}

enum FaceWalk {
    Vertex(VertexId, usize),
    AtBase,
    Looped,
    Stuck { endpoint: VertexId, side: usize },
}

enum FaceWalkError {
    MissingVertex {
        vertex: VertexId,
    },
    GuardExhausted {
        face: u32,
        endpoint: VertexId,
        side: usize,
    },
    Stuck {
        face: u32,
        endpoint: VertexId,
        side: usize,
    },
}

fn link_successors(halfedges: &mut [HalfEdge], from: u32, to: u32) {
    let from = from as usize;
    let to = to as usize;
    if from >= halfedges.len() || to >= halfedges.len() || from == to {
        return;
    }
    if halfedges[from].next.0 == from as u32 && halfedges[to].prev.0 == to as u32 {
        halfedges[from].next = HalfEdgeId(to as u32);
        halfedges[to].prev = HalfEdgeId(from as u32);
    }
}

fn next_on_face(
    kt: &KineticTriangulation,
    vertex_left_he: &HashMap<VertexId, u32>,
    start: VertexId,
    mut cur_v: VertexId,
    mut side_of_f: usize,
) -> Result<FaceWalk, FaceWalkError> {
    let max_skip = kt.vertices.len().saturating_add(2);
    for _ in 0..max_skip {
        let prev_v = cur_v;
        let Some(prev) = kt.vertices.get(prev_v) else {
            return Err(FaceWalkError::MissingVertex { vertex: prev_v });
        };
        let nxt = if side_of_f == 1 {
            prev.prev_vertex[side_of_f]
        } else {
            prev.next_vertex[side_of_f]
        };

        if nxt.is_none() && side_of_f == 0 {
            // Escaped to infinity: jump via the left wavefront edge.
            let Some(eid) = prev.incident_edges[0] else {
                return Ok(FaceWalk::Stuck {
                    endpoint: prev_v,
                    side: side_of_f,
                });
            };
            let Some(e) = kt.edges.get(eid) else {
                return Ok(FaceWalk::Stuck {
                    endpoint: prev_v,
                    side: side_of_f,
                });
            };
            cur_v = e.endpoints[0];
            side_of_f = 1;
        } else if nxt.is_none() && side_of_f == 1 {
            return Ok(FaceWalk::AtBase);
        } else if nxt == Some(start) {
            return Ok(FaceWalk::Looped);
        } else {
            let Some(n) = nxt else {
                return Ok(FaceWalk::Stuck {
                    endpoint: prev_v,
                    side: side_of_f,
                });
            };
            if kt.vertices.get(n).is_none() {
                return Err(FaceWalkError::MissingVertex { vertex: n });
            }
            let nv = &kt.vertices[n];
            side_of_f = if nv.prev_vertex[0] == Some(prev_v) {
                0
            } else {
                1
            };
            cur_v = n;
        }

        if !vertex_is_degenerate(kt, vertex_left_he, cur_v) {
            return Ok(FaceWalk::Vertex(cur_v, side_of_f));
        }
    }
    Ok(FaceWalk::Stuck {
        endpoint: cur_v,
        side: side_of_f,
    })
}

/// Insert leftover chains into an existing same-face cycle when their
/// endpoint nodes match an existing link.
fn splice_remaining_into_cycles(halfedges: &mut [HalfEdge], unbounded: FaceId, dest: &[u32]) {
    let n = halfedges.len();
    for start in 0..n {
        if halfedges[start].face == unbounded || halfedges[start].prev.0 != start as u32 {
            continue;
        }
        let mut end = start;
        for _ in 0..n {
            let next = halfedges[end].next.0 as usize;
            if next == end {
                break;
            }
            end = next;
        }
        if halfedges[end].next.0 != end as u32 {
            continue;
        }
        for pred in 0..n {
            if halfedges[pred].face != halfedges[start].face
                || halfedges[pred].next.0 == pred as u32
                || dest[pred] != halfedges[start].origin.0
            {
                continue;
            }
            let succ = halfedges[pred].next.0 as usize;
            if dest[end] != halfedges[succ].origin.0 {
                continue;
            }
            halfedges[pred].next = HalfEdgeId(start as u32);
            halfedges[start].prev = HalfEdgeId(pred as u32);
            halfedges[end].next = HalfEdgeId(succ as u32);
            halfedges[succ].prev = HalfEdgeId(end as u32);
            break;
        }
    }
}

fn he_starts_reciprocal_cycle(halfedges: &[HalfEdge], start: usize, face: u32) -> bool {
    let n = halfedges.len();
    if start >= n || halfedges[start].face.0 != face {
        return false;
    }
    let mut he = start;
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > n + 2 {
            return false;
        }
        let cur = &halfedges[he];
        if cur.face.0 != face {
            return false;
        }
        let next = cur.next.0 as usize;
        let prev = cur.prev.0 as usize;
        if next >= n || prev >= n || next == he {
            return false;
        }
        if halfedges[next].prev.0 != he as u32 || halfedges[prev].next.0 != he as u32 {
            return false;
        }
        he = next;
        if he == start {
            return true;
        }
    }
}

/// Link a leftover next to the unique same-face halfedge whose origin is
/// this dest and whose prev is still free. Topology only — no face rewrite.
fn attach_free_ends(halfedges: &mut [HalfEdge], unbounded: FaceId, dest: &[u32]) {
    let n = halfedges.len();
    let (mut starts, _) = leftover_face_node_buckets(halfedges, dest, |i| {
        halfedges[i].face != unbounded && halfedges[i].prev.0 == i as u32
    });
    let mut grew = true;
    let mut guard = 0;
    while grew {
        grew = false;
        guard += 1;
        if guard > n + 2 {
            break;
        }
        for i in 0..n {
            if halfedges[i].face == unbounded || halfedges[i].next.0 != i as u32 {
                continue;
            }
            let key = (halfedges[i].face.0, dest[i]);
            let Some(cands) = starts.get(&key) else {
                continue;
            };
            let mut found = None;
            for &j in cands {
                if i == j || halfedges[j].prev.0 != j as u32 {
                    continue;
                }
                if found.is_some() {
                    found = None;
                    break;
                }
                found = Some(j);
            }
            if let Some(j) = found {
                halfedges[i].next = HalfEdgeId(j as u32);
                halfedges[j].prev = HalfEdgeId(i as u32);
                if let Some(v) = starts.get_mut(&key) {
                    v.retain(|&x| x != j);
                }
                grew = true;
            }
        }
    }
}

fn rebuild_incomplete_faces(
    halfedges: &mut [HalfEdge],
    nodes: &[Node],
    faces: &mut [Face],
    unbounded: FaceId,
    dest: &[u32],
) {
    let mut by_face = vec![Vec::new(); faces.len()];
    for (i, he) in halfedges.iter().enumerate() {
        if he.face != unbounded {
            by_face[he.face.0 as usize].push(i);
        }
    }
    for (face_id, face_halfedges) in by_face.into_iter().enumerate() {
        let face = &mut faces[face_id];
        if face_halfedges.is_empty()
            || face.halfedge.is_some_and(|root| {
                cycle_covers_face(
                    halfedges,
                    root.0 as usize,
                    face_id as u32,
                    face_halfedges.len(),
                )
            })
        {
            continue;
        }

        for &i in &face_halfedges {
            halfedges[i].next = HalfEdgeId(i as u32);
            halfedges[i].prev = HalfEdgeId(i as u32);
        }
        let mut outgoing: HashMap<u32, Vec<usize>> = sewing_map(face_halfedges.len());
        let mut incoming: HashMap<u32, Vec<usize>> = sewing_map(face_halfedges.len());
        for &i in &face_halfedges {
            outgoing.entry(halfedges[i].origin.0).or_default().push(i);
            incoming.entry(dest[i]).or_default().push(i);
        }
        for (node, ins) in &incoming {
            let Some(outs) = outgoing.get(node) else {
                continue;
            };
            for (from, to) in pair_at_vertex(halfedges, nodes, &dest, *node, ins, outs) {
                halfedges[from].next = HalfEdgeId(to as u32);
                halfedges[to].prev = HalfEdgeId(from as u32);
            }
        }
        face.halfedge = face_halfedges.first().map(|&i| HalfEdgeId(i as u32));
    }
}

fn close_infinity_gaps(
    halfedges: &mut Vec<HalfEdge>,
    faces: &[Face],
    unbounded: FaceId,
    dest: &mut Vec<u32>,
) {
    let mut ends = vec![Vec::new(); faces.len()];
    let mut starts = vec![Vec::new(); faces.len()];
    for (i, he) in halfedges.iter().enumerate() {
        if he.face == unbounded {
            continue;
        }
        if he.next.0 == i as u32 {
            ends[he.face.0 as usize].push(i);
        }
        if he.prev.0 == i as u32 {
            starts[he.face.0 as usize].push(i);
        }
    }
    let mut gaps = Vec::new();
    for face in 0..faces.len() {
        if let ([end], [start]) = (ends[face].as_slice(), starts[face].as_slice()) {
            gaps.push((
                face as u32,
                *end,
                *start,
                dest[*end],
                halfedges[*start].origin.0,
            ));
        }
    }

    let mut closed = vec![false; gaps.len()];
    for i in 0..gaps.len() {
        if closed[i] {
            continue;
        }
        let Some(j) = (i + 1..gaps.len())
            .find(|&j| !closed[j] && gaps[i].3 == gaps[j].4 && gaps[i].4 == gaps[j].3)
        else {
            continue;
        };
        let (face_a, end_a, start_a, from, to) = gaps[i];
        let (face_b, end_b, start_b, _, _) = gaps[j];
        let a = halfedges.len() as u32;
        let b = a + 1;
        halfedges.push(HalfEdge {
            twin: HalfEdgeId(b),
            next: HalfEdgeId(start_a as u32),
            prev: HalfEdgeId(end_a as u32),
            face: FaceId(face_a),
            origin: NodeId(from),
            geometry: ArcGeometry::InfinityChord,
            is_input: false,
        });
        halfedges.push(HalfEdge {
            twin: HalfEdgeId(a),
            next: HalfEdgeId(start_b as u32),
            prev: HalfEdgeId(end_b as u32),
            face: FaceId(face_b),
            origin: NodeId(to),
            geometry: ArcGeometry::InfinityChord,
            is_input: false,
        });
        halfedges[end_a].next = HalfEdgeId(a);
        halfedges[start_a].prev = HalfEdgeId(a);
        halfedges[end_b].next = HalfEdgeId(b);
        halfedges[start_b].prev = HalfEdgeId(b);
        dest.push(to);
        dest.push(from);
        closed[i] = true;
        closed[j] = true;
    }
}

fn cycle_covers_face(halfedges: &[HalfEdge], start: usize, face: u32, expected: usize) -> bool {
    let mut he = start;
    for count in 1..=expected {
        if he >= halfedges.len() || halfedges[he].face.0 != face {
            return false;
        }
        let next = halfedges[he].next.0 as usize;
        if next >= halfedges.len() || halfedges[next].prev.0 != he as u32 {
            return false;
        }
        he = next;
        if he == start {
            return count == expected;
        }
    }
    false
}

fn repair_face_halfedges(halfedges: &[HalfEdge], faces: &mut [Face], unbounded: FaceId) {
    for (fi, face) in faces.iter_mut().enumerate() {
        if fi as u32 == unbounded.0 {
            continue;
        }
        if let Some(h) = face.halfedge {
            if he_starts_reciprocal_cycle(halfedges, h.0 as usize, fi as u32) {
                continue;
            }
        }
        face.halfedge = (0..halfedges.len())
            .find(|&i| he_starts_reciprocal_cycle(halfedges, i, fi as u32))
            .map(|i| HalfEdgeId(i as u32));
    }
}

pub(super) fn validate_bounded_face_cycles(
    halfedges: &[HalfEdge],
    faces: &[Face],
    unbounded: FaceId,
) -> Result<(), SkeletonError> {
    let n = halfedges.len();
    if n == 0 {
        return Ok(());
    }

    let mut owned = vec![0usize; faces.len()];
    for he in halfedges {
        let f = he.face.0 as usize;
        if f < owned.len() && he.face != unbounded {
            owned[f] += 1;
        }
    }

    let mut claimed = vec![false; n];
    for (fi, face) in faces.iter().enumerate() {
        if fi as u32 == unbounded.0 {
            continue;
        }
        if owned.get(fi).copied().unwrap_or(0) > 0 && face.halfedge.is_none() {
            return Err(SkeletonError::IncompleteFaceCycle {
                detail: format!(
                    "stage=bounded-validation face={fi}: owns {} halfedges but has no root",
                    owned[fi]
                ),
            });
        }
        let Some(start) = face.halfedge else {
            continue;
        };
        let start = start.0 as usize;
        if start >= n {
            return Err(SkeletonError::IncompleteFaceCycle {
                detail: format!(
                    "stage=bounded-validation face={fi} halfedge={start}: root out of range"
                ),
            });
        }
        if halfedges[start].face.0 as usize != fi {
            return Err(SkeletonError::IncompleteFaceCycle {
                detail: format!(
                    "stage=bounded-validation face={fi} halfedge={start}: root is on face {}",
                    halfedges[start].face.0
                ),
            });
        }
        if halfedges[start].next.0 == start as u32 {
            return Err(SkeletonError::IncompleteFaceCycle {
                detail: format!(
                    "stage=bounded-validation face={fi} halfedge={start}: root is a self-loop"
                ),
            });
        }
        let mut he = start;
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > n + 2 {
                return Err(SkeletonError::IncompleteFaceCycle {
                    detail: format!(
                        "stage=bounded-validation face={fi} halfedge={he}: cycle did not close"
                    ),
                });
            }
            let cur = &halfedges[he];
            if cur.face.0 as usize != fi {
                return Err(SkeletonError::IncompleteFaceCycle {
                    detail: format!(
                        "stage=bounded-validation face={fi} halfedge={he}: walk entered face {}",
                        cur.face.0
                    ),
                });
            }
            let next = cur.next.0 as usize;
            let prev = cur.prev.0 as usize;
            if next >= n || prev >= n {
                return Err(SkeletonError::IncompleteFaceCycle {
                    detail: format!(
                        "stage=bounded-validation face={fi} halfedge={he}: next/prev out of range"
                    ),
                });
            }
            if halfedges[next].prev.0 != he as u32 || halfedges[prev].next.0 != he as u32 {
                return Err(SkeletonError::IncompleteFaceCycle {
                    detail: format!(
                        "stage=bounded-validation face={fi} halfedge={he}: next/prev not reciprocal"
                    ),
                });
            }
            if halfedges[next].face.0 as usize != fi {
                return Err(SkeletonError::IncompleteFaceCycle {
                    detail: format!(
                        "stage=bounded-validation face={fi} halfedge={he}: next is on face {}",
                        halfedges[next].face.0
                    ),
                });
            }
            if claimed[he] {
                return Err(SkeletonError::IncompleteFaceCycle {
                    detail: format!(
                        "stage=bounded-validation face={fi} halfedge={he}: claimed twice"
                    ),
                });
            }
            claimed[he] = true;
            he = next;
            if he == start {
                break;
            }
        }
    }
    for (i, he) in halfedges.iter().enumerate() {
        if he.face == unbounded {
            continue;
        }
        if !claimed[i] {
            return Err(SkeletonError::IncompleteFaceCycle {
                detail: format!(
                    "stage=bounded-validation face={} halfedge={i}: not reachable from face root",
                    he.face.0
                ),
            });
        }
    }
    Ok(())
}

/// Leftover halfedges keyed by (face, origin) and (face, dest).
fn leftover_face_node_buckets(
    halfedges: &[HalfEdge],
    dest: &[u32],
    mut leftover: impl FnMut(usize) -> bool,
) -> (
    HashMap<(u32, u32), Vec<usize>>,
    HashMap<(u32, u32), Vec<usize>>,
) {
    let mut outgoing: HashMap<(u32, u32), Vec<usize>> = sewing_map(halfedges.len());
    let mut incoming: HashMap<(u32, u32), Vec<usize>> = sewing_map(halfedges.len());
    for i in 0..halfedges.len() {
        if !leftover(i) {
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
    (outgoing, incoming)
}

/// Pair leftover self-loops by (face, dest → origin). Unique at a node, or
/// smallest CCW turn when several halfedges share a vertex on one face.
fn pair_remaining_by_dest(halfedges: &mut [HalfEdge], nodes: &[Node], dest: &[u32]) {
    let (outgoing, incoming) =
        leftover_face_node_buckets(halfedges, dest, |i| halfedges[i].next.0 == i as u32);

    // Fx hasher keeps key visit order deterministic across runs.
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

#[cfg(test)]
mod tests {
    use super::validate_bounded_face_cycles;
    use crate::dcel::{ArcGeometry, Face, FaceId, HalfEdge, HalfEdgeId, NodeId};
    use crate::error::SkeletonError;
    use crate::geometry::{Point2, Segment2};

    fn he(twin: u32, next: u32, prev: u32, face: u32, origin: u32) -> HalfEdge {
        HalfEdge {
            twin: HalfEdgeId(twin),
            next: HalfEdgeId(next),
            prev: HalfEdgeId(prev),
            face: FaceId(face),
            origin: NodeId(origin),
            geometry: ArcGeometry::Segment(Segment2::new(
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
            )),
            is_input: false,
        }
    }

    fn face(halfedge: Option<u32>) -> Face {
        Face {
            source_input_edge: None,
            is_beveling: false,
            component: 0,
            halfedge: halfedge.map(HalfEdgeId),
        }
    }

    #[test]
    fn bounded_validation_names_face_without_root() {
        let halfedges = vec![
            he(1, 0, 0, 0, 0),
            he(0, 1, 1, 1, 1), // unbounded twin placeholder
        ];
        let faces = vec![face(None), face(None)];
        let err = validate_bounded_face_cycles(&halfedges, &faces, FaceId(1)).unwrap_err();
        match err {
            SkeletonError::IncompleteFaceCycle { detail } => {
                assert!(
                    detail.contains("stage=bounded-validation"),
                    "missing stage: {detail}"
                );
                assert!(detail.contains("face=0"), "missing face: {detail}");
                assert!(
                    detail.contains("has no root"),
                    "missing root cause: {detail}"
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn bounded_validation_names_unreachable_halfedge() {
        // Reciprocal 0↔1 cycle on face 0; he 2 is owned by face 0 but unreachable.
        let halfedges = vec![
            he(1, 1, 1, 0, 0),
            he(0, 0, 0, 0, 1),
            he(3, 2, 2, 0, 2),
            he(2, 3, 3, 1, 3),
        ];
        let faces = vec![face(Some(0)), face(None)];
        let err = validate_bounded_face_cycles(&halfedges, &faces, FaceId(1)).unwrap_err();
        match err {
            SkeletonError::IncompleteFaceCycle { detail } => {
                assert!(
                    detail.contains("stage=bounded-validation"),
                    "missing stage: {detail}"
                );
                assert!(detail.contains("halfedge=2"), "missing halfedge: {detail}");
                assert!(
                    detail.contains("not reachable from face root"),
                    "missing cause: {detail}"
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
