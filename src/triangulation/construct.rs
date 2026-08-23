//! Kinetic triangulation construction from a PSLG.
//!
//! Kinetic vertices, bevels, and unbounded-hull attachment live beside
//! the kinetic triangulation they construct. The propagator remains the
//! event engine.
//!
//! Upstream: `surf/src/KineticTriangulation.*` init
//! (`create_kinetic_vertices`, `create_bevels`, unbounded hull)
//! (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use crate::error::SkeletonError;
use crate::geometry::Point2;
use crate::input::Pslg;
use crate::triangulation::around_vertex::{most_around, step_around};
use crate::triangulation::basic::BasicTriangulation;
use crate::triangulation::kinetic::{KineticTriangulation, SkeletonFaceMeta, TriangleId};
use crate::triangulation::refine::{raw_flip, refine_triangulation_initial};
use crate::wavefront::edge::WavefrontEdge;
use crate::wavefront::supporting_line::{wavefront_vertex_motion, SupportingLine};
use crate::wavefront::vertex::{
    vertex_angle_from_lines, InfiniteSpeedType, VertexId, WavefrontVertex,
};

pub(crate) fn initialize_kinetic(
    basic: &BasicTriangulation,
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
        attach_unbounded_hull(&mut kt, basic, &tri_map);
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

    create_kinetic_vertices(&mut kt, input)?;
    create_bevels(&mut kt, input)?;
    refine_triangulation_initial(&mut kt)?;
    kt.has_nonunit_weight = kt
        .edges
        .values()
        .any(|e| (e.supporting_line.weight - 1.0).abs() > 1e-15);

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
fn create_kinetic_vertices(
    kt: &mut KineticTriangulation,
    input: &Pslg,
) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    // C++ skips degree-1: they always need a bevel cap between the two
    // sides of the dangling edge.
    let mut deg = vec![0u32; input.vertex_count()];
    for e in input.edges() {
        deg[e.vertices[0]] += 1;
        deg[e.vertices[1]] += 1;
    }

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
            if kt.vertices[start_v]
                .source_input_vertex
                .is_some_and(|s| deg.get(s as usize) == Some(&1))
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
    let mut deg = vec![0u32; input.vertex_count()];
    for e in input.edges() {
        deg[e.vertices[0]] += 1;
        deg[e.vertices[1]] += 1;
    }

    // C++ walks triangles in id order and bevels the first NULL slot at
    // each degree-1 vertex; later incidents already have a kinetic vertex.
    let mut starts: Vec<(i32, u32, TriangleId, usize, u32)> = kt
        .triangles
        .iter()
        .filter(|(_, t)| t.live())
        .flat_map(|(tid, t)| (0..3).map(move |i| (tid, t.component, t.tidx, i, t.vertices[i])))
        .filter_map(|(tid, component, tidx, i, vid)| {
            let vid = vid?;
            let src = kt.vertices[vid].source_input_vertex?;
            if deg[src as usize] == 1 {
                Some((component, tidx, tid, i, src))
            } else {
                None
            }
        })
        .collect();
    starts.sort_unstable_by_key(|s| (s.0, s.1));
    let mut seen = vec![false; input.vertex_count()];
    for (_, _, tid, i, src) in starts {
        if seen[src as usize] {
            continue;
        }
        seen[src as usize] = true;
        create_bevels_at_vertex(kt, tid, i)?;
    }
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
        let ccw_vid = kt.triangles[t_it.0].vertices[ccw(t_it.1)];
        let infinite_ccw = ccw_vid.is_some() && ccw_vid == kt.infinite_vertex;
        let (split_this, need_flip) = if last {
            (false, false)
        } else if next_cw.is_none() {
            (true, false)
        } else if infinite_ccw {
            // C++ unbounded: split if next-cw's ccw is not strictly left of
            // the current supporting line; flip if this triangle's cw is.
            let next_t = next_cw.unwrap();
            let next_ccw = kt.triangles[next_t.0].vertices[ccw(next_t.1)];
            if let Some(nv) = next_ccw.filter(|v| Some(*v) != kt.infinite_vertex) {
                let sl = kt.edges[current_edge].supporting_line;
                let split = sl.orientation_point(kt.vertices[nv].pos_zero) <= 0.0;
                let flip = split
                    && kt.triangles[t_it.0].vertices[cw(t_it.1)]
                        .filter(|v| Some(*v) != kt.infinite_vertex)
                        .is_some_and(|v| sl.orientation_point(kt.vertices[v].pos_zero) > 0.0);
                (split, flip)
            } else {
                (true, false)
            }
        } else {
            match ccw_vid {
                Some(cv) => {
                    let p = kt.vertices[cv].pos_zero;
                    let cross = bevel_dir.x * (p.y - pos.y) - bevel_dir.y * (p.x - pos.x);
                    (cross > 0.0, false)
                }
                None => (true, false),
            }
        };
        if split_this && !last {
            let new_t = split_vertex_bevel(kt, t_it.0, t_it.1, current_edge, v1)?;
            if need_flip {
                if let Some(nid) = kt.triangles[new_t].neighbors[0] {
                    raw_flip(kt, new_t, 0, nid)?;
                }
            }
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
        if let Some(s) = kt.neighbor_side(nb, tid) {
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
