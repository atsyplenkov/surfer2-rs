//! Face-cycle construction and DCEL validation.

use std::collections::HashMap;

use crate::dcel::{Face, FaceId, HalfEdge, HalfEdgeId, Node};
use crate::error::SkeletonError;
use crate::triangulation::KineticTriangulation;
use crate::wavefront::vertex::VertexId;

pub(super) fn connect_face_cycles(
    kt: &KineticTriangulation,
    halfedges: &mut [HalfEdge],
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
    let mut walked_faces = std::collections::HashSet::new();
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
    for (&face, _) in face_base {
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
                    return Err(SkeletonError::IncompleteFaceCycle {
                        detail: format!(
                            "stage=face-walk face={face} endpoint={vertex:?}: missing vertex"
                        ),
                    });
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

    pair_remaining_by_dest(halfedges, nodes);
    splice_remaining_into_cycles(halfedges, unbounded);
    attach_free_ends(halfedges, unbounded);
    repair_face_halfedges(halfedges, faces, unbounded);
    Ok(walk_failures)
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

/// Insert leftover self-loops into an existing same-face cycle when
/// `dest(pred) == origin(he)` and `dest(he) == origin(succ)`.
fn splice_remaining_into_cycles(halfedges: &mut [HalfEdge], unbounded: FaceId) {
    let n = halfedges.len();
    let dest = |he: usize, hes: &[HalfEdge]| hes[hes[he].twin.0 as usize].origin.0;
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > n + 2 {
            break;
        }
        let mut did = false;
        for i in 0..n {
            if halfedges[i].face == unbounded || halfedges[i].next.0 != i as u32 {
                continue;
            }
            let origin_i = halfedges[i].origin.0;
            let dest_i = dest(i, halfedges);
            for k in 0..n {
                if k == i || halfedges[k].face != halfedges[i].face {
                    continue;
                }
                if halfedges[k].next.0 == k as u32 {
                    continue;
                }
                if dest(k, halfedges) != origin_i {
                    continue;
                }
                let m = halfedges[k].next.0 as usize;
                if dest_i != halfedges[m].origin.0 {
                    continue;
                }
                halfedges[k].next = HalfEdgeId(i as u32);
                halfedges[i].prev = HalfEdgeId(k as u32);
                halfedges[i].next = HalfEdgeId(m as u32);
                halfedges[m].prev = HalfEdgeId(i as u32);
                did = true;
                break;
            }
        }
        if !did {
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
fn attach_free_ends(halfedges: &mut [HalfEdge], unbounded: FaceId) {
    let n = halfedges.len();
    let dest: Vec<u32> = halfedges
        .iter()
        .map(|he| halfedges[he.twin.0 as usize].origin.0)
        .collect();
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
            let mut found = None;
            for j in 0..n {
                if i == j || halfedges[j].face != halfedges[i].face {
                    continue;
                }
                if halfedges[j].prev.0 == j as u32 && halfedges[j].origin.0 == dest[i] {
                    if found.is_some() {
                        found = None;
                        break;
                    }
                    found = Some(j);
                }
            }
            if let Some(j) = found {
                halfedges[i].next = HalfEdgeId(j as u32);
                halfedges[j].prev = HalfEdgeId(i as u32);
                grew = true;
            }
        }
    }
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
