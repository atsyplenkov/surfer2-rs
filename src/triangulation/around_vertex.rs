//! Circulate incident triangles around a kinetic vertex.
//!
//! Upstream: `AroundVertexIterator` in `KineticTriangulation.h`.

use super::kinetic::{ccw, cw, KineticTriangulation, Side, TriangleId};

/// One C++ `AroundVertexIterator` step.
///
/// `++` uses the cw table (`use_cw = true`); `--` uses ccw.
pub fn step_around(
    kt: &KineticTriangulation,
    tid: TriangleId,
    v_idx: Side,
    use_cw: bool,
) -> Option<(TriangleId, Side)> {
    let t = kt.triangles.get(tid)?;
    // C++ never circulates around the infinite vertex (the unbounded
    // ring is a cycle). Walking it does not terminate.
    if kt.infinite_vertex.is_some() && t.vertices[v_idx] == kt.infinite_vertex {
        return None;
    }
    let next = if use_cw {
        t.neighbors[cw(v_idx)]
    } else {
        t.neighbors[ccw(v_idx)]
    }?;
    let n = kt.triangles.get(next)?;
    let t_in_n = (0..3).find(|&s| n.neighbors[s] == Some(tid))?;
    let v_in_n = if use_cw { cw(t_in_n) } else { ccw(t_in_n) };
    Some((next, v_in_n))
}

/// Walk `++`/`--` until there is no neighbor (C++ `most_cw` / `most_ccw`).
pub fn most_around(
    kt: &KineticTriangulation,
    tid: TriangleId,
    v_idx: Side,
    use_cw: bool,
) -> (TriangleId, Side) {
    let mut cur = (tid, v_idx);
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 10_000 {
            return cur;
        }
        match step_around(kt, cur.0, cur.1, use_cw) {
            Some(n) => {
                if n.0 == tid {
                    return cur;
                }
                cur = n;
            }
            None => return cur,
        }
    }
}

/// Yield (triangle, index_of_vertex_in_triangle) circulating CCW around `v`.
pub fn triangles_around_vertex(
    kt: &KineticTriangulation,
    start: TriangleId,
    v: crate::wavefront::vertex::VertexId,
) -> Vec<(TriangleId, Side)> {
    let mut out = Vec::new();
    let mut current = Some(start);
    let mut guard = 0;
    while let Some(t) = current {
        guard += 1;
        if guard > 10_000 {
            break;
        }
        let tri = match kt.triangles.get(t) {
            Some(tr) if tr.live() => tr,
            _ => break,
        };
        let mut idx = None;
        for i in 0..3 {
            if tri.vertices[i] == Some(v) {
                idx = Some(i);
                break;
            }
        }
        let Some(i) = idx else { break };
        if out.iter().any(|(id, _)| *id == t) {
            break;
        }
        out.push((t, i));
        current = step_around(kt, t, i, true).map(|(id, _)| id);
    }
    out
}
