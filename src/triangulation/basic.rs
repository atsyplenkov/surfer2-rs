//! Static constrained Delaunay triangulation and planar region labeling.
//!
//! Upstream: `surf/src/BasicTriangulation.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.
//!
//! Spade is confined here; no Spade handle crosses into the kinetic engine.

use std::collections::{HashMap, VecDeque};

use spade::{ConstrainedDelaunayTriangulation, Point2 as SpadePoint, Triangulation as _};

use crate::error::{InputError, TriangulationError};
use crate::geometry::Point2;
use crate::input::Pslg;

/// Owned static triangulation copied out of Spade.
#[derive(Debug, Clone)]
pub struct BasicTriangulation {
    pub vertices: Vec<Point2>,
    /// Finite triangles as vertex index triples (CCW).
    pub triangles: Vec<[usize; 3]>,
    /// For each triangle side i (opposite vertex i? we store side as edge opposite
    /// to vertices[i] standard: side i connects vertices[(i+1)%3]–[(i+2)%3]).
    /// Neighbor triangle index or `None` if boundary/wavefront constraint.
    pub neighbors: Vec<[Option<usize>; 3]>,
    /// If side is constrained, Some(input_edge_id).
    pub constraints: Vec<[Option<u32>; 3]>,
    /// Component id per finite triangle; `None` = exterior / unrestricted.
    pub components: Vec<Option<u32>>,
    pub max_component: Option<u32>,
    /// Infinite outer face sentinel — not stored as triangle; boundary sides
    /// with no neighbor are outer.
    pub input_edge_endpoints: Vec<[usize; 2]>,
}

// Spade 2.x requires position on the vertex type. Store coordinates in VData.
#[derive(Clone, Debug)]
struct VertexData {
    x: f64,
    y: f64,
    input_index: usize,
}

impl spade::HasPosition for VertexData {
    type Scalar = f64;
    fn position(&self) -> SpadePoint<f64> {
        SpadePoint::new(self.x, self.y)
    }
}

type Cdt2 = ConstrainedDelaunayTriangulation<VertexData>;

pub fn build_basic_triangulation(pslg: &Pslg) -> Result<BasicTriangulation, TriangulationError> {
    if pslg.vertices().is_empty() || pslg.edges().is_empty() {
        return Err(TriangulationError::RegionClassification);
    }

    let mut cdt: Cdt2 = ConstrainedDelaunayTriangulation::new();
    let mut input_to_handle = Vec::with_capacity(pslg.vertex_count());

    for (i, p) in pslg.vertices().iter().enumerate() {
        let handle = cdt
            .insert(VertexData {
                x: p.x,
                y: p.y,
                input_index: i,
            })
            .map_err(|_| TriangulationError::VertexInsertion { index: i })?;
        input_to_handle.push(handle);
    }

    // Map canonical endpoint pair → input edge id and insert constraints.
    let mut edge_lookup: HashMap<(usize, usize), u32> =
        HashMap::with_capacity(pslg.edges().len());
    for (ei, e) in pslg.edges().iter().enumerate() {
        let (a, b) = e.normalized_endpoints();
        edge_lookup.insert((a, b), ei as u32);
        let h0 = input_to_handle[e.vertices[0]];
        let h1 = input_to_handle[e.vertices[1]];
        let _added = cdt.add_constraint(h0, h1);
    }

    // Verify constraints exist unsplit: for each input edge, directed edge present.
    for (ei, e) in pslg.edges().iter().enumerate() {
        let h0 = input_to_handle[e.vertices[0]];
        let h1 = input_to_handle[e.vertices[1]];
        let found = cdt.get_edge_from_neighbors(h0, h1).is_some()
            || cdt.get_edge_from_neighbors(h1, h0).is_some();
        if !found {
            return Err(TriangulationError::ConstraintSplitOrLost { edge: ei });
        }
    }

    // Collect finite faces
    let mut triangles = Vec::new();
    for face in cdt.inner_faces() {
        let vs = face.vertices();
        if vs.len() != 3 {
            continue;
        }
        let tri = [
            vs[0].data().input_index,
            vs[1].data().input_index,
            vs[2].data().input_index,
        ];
        triangles.push(ensure_ccw(pslg.vertices(), tri));
    }

    let n = triangles.len();
    let mut neighbors = vec![[None; 3]; n];
    let mut constraints = vec![[None; 3]; n];
    let mut directed: HashMap<(usize, usize), usize> = HashMap::with_capacity(n * 3);
    for (ti, tri) in triangles.iter().enumerate() {
        for s in 0..3 {
            let a = tri[(s + 1) % 3];
            let b = tri[(s + 2) % 3];
            directed.insert((a, b), ti);
            if let Some(&eid) = edge_lookup.get(&canonical(a, b)) {
                constraints[ti][s] = Some(eid);
            }
        }
    }
    for (ti, tri) in triangles.iter().enumerate() {
        for s in 0..3 {
            let a = tri[(s + 1) % 3];
            let b = tri[(s + 2) % 3];
            if let Some(&tj) = directed.get(&(b, a)) {
                neighbors[ti][s] = Some(tj);
            }
        }
    }

    // Component tagging: left of first input edge = 0, right = 1, then BFS.
    let first = pslg.edges().first().ok_or(TriangulationError::RegionClassification)?;
    let u = first.vertices[0];
    let v = first.vertices[1];
    let left = directed.get(&(u, v)).copied();

    let mut assigned = vec![None; n];
    let mut next_id = 0u32;
    let mut max_component = None;

    // C++ `tag_components`: seed only the face left of the first input
    // edge. The right face is discovered across that constraint; if the
    // flood already reached it around a degree-1 tip, both sides share a
    // component (dangling spur). Seeding both immediately splits a spur.
    let mut seed_order: Vec<usize> = Vec::new();
    if let Some(t) = left {
        seed_order.push(t);
    }
    // Also collect any unvisited later

    let mut region_queue: VecDeque<(usize, u32)> = VecDeque::new();
    for t in seed_order {
        if assigned[t].is_none() {
            assigned[t] = Some(next_id);
            max_component = Some(next_id);
            region_queue.push_back((t, next_id));
            next_id += 1;
        }
    }

    let mut frontier: VecDeque<usize> = VecDeque::new();
    // Process each component flood, collecting constrained-adj for later seeds
    let mut next_seeds: VecDeque<usize> = VecDeque::new();
    for &(s, _) in &region_queue {
        frontier.push_back(s);
    }

    while !frontier.is_empty() || !next_seeds.is_empty() || region_queue.iter().any(|_| false) {
        if frontier.is_empty() {
            if let Some(s) = next_seeds.pop_front() {
                if assigned[s].is_none() {
                    assigned[s] = Some(next_id);
                    max_component = Some(next_id);
                    frontier.push_back(s);
                    next_id += 1;
                }
                continue;
            }
            // Any remaining triangle
            if let Some(t) = assigned.iter().position(|c| c.is_none()) {
                assigned[t] = Some(next_id);
                max_component = Some(next_id);
                frontier.push_back(t);
                next_id += 1;
            } else {
                break;
            }
            continue;
        }
        let t = frontier.pop_front().unwrap();
        let comp = assigned[t].unwrap();
        for s in 0..3 {
            if let Some(nbi) = neighbors[t][s] {
                if constraints[t][s].is_some() {
                    if assigned[nbi].is_none() {
                        next_seeds.push_back(nbi);
                    }
                } else if assigned[nbi].is_none() {
                    assigned[nbi] = Some(comp);
                    frontier.push_back(nbi);
                }
            }
        }
    }

    let input_edge_endpoints = pslg
        .edges()
        .iter()
        .map(|e| e.vertices)
        .collect();

    Ok(BasicTriangulation {
        vertices: pslg.vertices().to_vec(),
        triangles,
        neighbors,
        constraints: constraints,
        components: assigned,
        max_component,
        input_edge_endpoints,
    })
}

/// Odd/even interior selection for a polygon CDT: faces reachable from outer
/// by crossing an odd number of constraints form the interior.
pub fn select_polygon_interior(
    basic: &BasicTriangulation,
) -> Result<Vec<usize>, TriangulationError> {
    let n = basic.triangles.len();
    if n == 0 {
        return Err(TriangulationError::RegionClassification);
    }
    // Boundary faces: any triangle with a null neighbor side that is constrained
    // (outer boundary) or unconstrained (should not happen in closed polygon).
    // Parity BFS from outside: start at a virtual outer, cross every boundary side.
    let mut parity = vec![None; n]; // Some(odd)=interior candidate
    let mut q = VecDeque::new();

    // Mark all triangles with a missing neighbor as adjacent to exterior with
    // parity = (constraint on that side ? 1 : 0)
    for ti in 0..n {
        for s in 0..3 {
            if basic.neighbors[ti][s].is_none() {
                let p = if basic.constraints[ti][s].is_some() {
                    1u8
                } else {
                    0u8
                };
                match parity[ti] {
                    None => {
                        parity[ti] = Some(p);
                        q.push_back(ti);
                    }
                    Some(old) => {
                        if old != p {
                            // conflicting — still take min discovery
                        }
                    }
                }
            }
        }
    }

    if q.is_empty() {
        // No boundary? Fully constrained internal only — treat component of max size with odd
        return Err(TriangulationError::RegionClassification);
    }

    while let Some(t) = q.pop_front() {
        let p = parity[t].unwrap();
        for s in 0..3 {
            if let Some(nb) = basic.neighbors[t][s] {
                let cross = if basic.constraints[t][s].is_some() {
                    1
                } else {
                    0
                };
                let np = p ^ cross;
                if parity[nb].is_none() {
                    parity[nb] = Some(np);
                    q.push_back(nb);
                }
            }
        }
    }

    let interior: Vec<usize> = parity
        .iter()
        .enumerate()
        .filter_map(|(i, p)| if p == &Some(1) { Some(i) } else { None })
        .collect();

    if interior.is_empty() {
        return Err(TriangulationError::RegionClassification);
    }

    // Connectivity of interior under non-constraint adjacency
    let mut seen = vec![false; n];
    let mut stack = vec![interior[0]];
    seen[interior[0]] = true;
    let mut count = 0;
    while let Some(t) = stack.pop() {
        count += 1;
        for s in 0..3 {
            if basic.constraints[t][s].is_some() {
                continue;
            }
            if let Some(nb) = basic.neighbors[t][s] {
                if parity[nb] == Some(1) && !seen[nb] {
                    seen[nb] = true;
                    stack.push(nb);
                }
            }
        }
    }
    if count != interior.len() {
        return Err(TriangulationError::RegionClassification);
    }
    Ok(interior)
}

fn ensure_ccw(verts: &[Point2], tri: [usize; 3]) -> [usize; 3] {
    let a = verts[tri[0]];
    let b = verts[tri[1]];
    let c = verts[tri[2]];
    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if cross < 0.0 {
        [tri[0], tri[2], tri[1]]
    } else {
        tri
    }
}

fn canonical(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

impl From<InputError> for TriangulationError {
    fn from(_: InputError) -> Self {
        TriangulationError::RegionClassification
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Polygon;

    #[test]
    fn square_cdt() {
        let poly = Polygon::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ],
            vec![],
        )
        .unwrap();
        let pslg = poly.as_pslg().unwrap();
        let basic = build_basic_triangulation(&pslg).unwrap();
        assert!(basic.triangles.len() >= 2);
        let interior = select_polygon_interior(&basic).unwrap();
        assert!(!interior.is_empty());
    }

    #[test]
    fn ring_reversal_same_interior() {
        let cw = Polygon::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(0.0, 1.0),
                Point2::new(1.0, 1.0),
                Point2::new(1.0, 0.0),
            ],
            vec![],
        )
        .unwrap();
        let ccw = Polygon::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ],
            vec![],
        )
        .unwrap();
        let a = build_basic_triangulation(&cw.as_pslg().unwrap()).unwrap();
        let b = build_basic_triangulation(&ccw.as_pslg().unwrap()).unwrap();
        let ia = select_polygon_interior(&a).unwrap();
        let ib = select_polygon_interior(&b).unwrap();
        assert_eq!(ia.len(), ib.len());
    }
}
