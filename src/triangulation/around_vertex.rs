//! Circulate incident triangles around a kinetic vertex.
//!
//! Upstream: `AroundVertexIterator` in `KineticTriangulation.h`.

use super::kinetic::{ccw, cw, KineticTriangulation, Side, TriangleId};
use crate::wavefront::vertex::VertexId;

/// Yield (triangle, index_of_vertex_in_triangle) circulating CCW around `v`.
pub fn triangles_around_vertex(
    kt: &KineticTriangulation,
    start: TriangleId,
    v: VertexId,
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
            break; // closed loop
        }
        out.push((t, i));
        // Step to neighbor across edge cw(i) → next around vertex
        current = tri.neighbors[cw(i)];
        let _ = ccw(i);
    }
    out
}
