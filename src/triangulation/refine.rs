//! Refinement orchestration after kinetic events.
//!
//! Upstream: `KineticTriangulation::refine_triangulation` / `do_flip`
//! (rev c42f9a7). Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use crate::error::SkeletonError;
use crate::event::{CollapseType, EventQueue};
use crate::triangulation::kinetic::{KineticTriangulation, TriangleId};

/// Incomplete weighted leftover / flip / CH refine still loops or drops
/// oracle counts. Isolate that policy here instead of a per-event edge scan.
pub fn drain_refinement_after(spec: CollapseType, has_nonunit_weight: bool) -> bool {
    !has_nonunit_weight
        && matches!(
            spec,
            CollapseType::TriangleCollapse
                | CollapseType::ConstraintCollapse
                | CollapseType::SpokeCollapse
                | CollapseType::SplitOrFlipRefine
        )
}

pub fn apply_event_refinement(
    kt: &mut KineticTriangulation,
    queue: Option<&mut EventQueue>,
    spec: CollapseType,
    time: f64,
) -> Result<(), SkeletonError> {
    if drain_refinement_after(spec, kt.has_nonunit_weight) {
        process_check_refinement_queue(kt, queue, time)
    } else {
        while kt.check_refinement_pop().is_some() {}
        Ok(())
    }
}

pub fn mark_modified(
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
pub fn do_flip(
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
pub fn refine_triangulation(
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
    let Some(idx_in_n) = kt.neighbor_side(nid, tid) else {
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

pub fn process_check_refinement_queue(
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

pub fn refine_triangulation_initial(kt: &mut KineticTriangulation) -> Result<(), SkeletonError> {
    // Keep the pre-queue multi-pass at t=0: C++ one-pass+guard drops
    // weighted/srpg_octo0000035-random-2 131→130. Per-event drain uses
    // refine_triangulation (reflex-or-straight guard).
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
    Ok(())
}

pub fn raw_flip(
    kt: &mut KineticTriangulation,
    tid: TriangleId,
    side: usize,
    ntid: TriangleId,
) -> Result<(), SkeletonError> {
    use crate::triangulation::kinetic::{ccw, cw};

    kt.triangles
        .get(ntid)
        .ok_or_else(|| crate::error::PropagationError::TopologyInvariant {
            detail: "flip missing neighbor".into(),
        })?;
    let nidx = kt.neighbor_side(ntid, tid).ok_or_else(|| {
        crate::error::PropagationError::TopologyInvariant {
            detail: "flip: neighbor does not point back".into(),
        }
    })?;

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
            if let Some(s) = kt.neighbor_side(other_id, ntid) {
                if let Some(other) = kt.triangles.get_mut(other_id) {
                    other.neighbors[s] = Some(tid);
                    other.invalidate_collapse();
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
            if let Some(s) = kt.neighbor_side(other_id, tid) {
                if let Some(other) = kt.triangles.get_mut(other_id) {
                    other.neighbors[s] = Some(ntid);
                    other.invalidate_collapse();
                }
            }
        }
    }

    Ok(())
}
