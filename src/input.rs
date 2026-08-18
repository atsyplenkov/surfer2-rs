//! Input models: general PSLG and polygon-with-holes façade.
//!
//! Upstream: `surf/src/BasicInput.*`, GraphML vertex/edge loading in `BGLGraph.*`.
//! Additive edge weights from GraphML are intentionally omitted (upstream never
//! applies them to propagation).

use crate::error::InputError;
use crate::geometry::{
    require_finite_point, segments_properly_intersect, Point2,
};

/// Undirected input constraint with multiplicative positive weight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputEdge {
    pub vertices: [usize; 2],
    pub weight: f64,
}

impl InputEdge {
    pub fn new(v0: usize, v1: usize, weight: f64) -> Self {
        Self {
            vertices: [v0, v1],
            weight,
        }
    }

    #[inline]
    pub fn normalized_endpoints(self) -> (usize, usize) {
        let a = self.vertices[0];
        let b = self.vertices[1];
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
}

/// Planar straight-line graph accepted by the general skeleton API.
#[derive(Debug, Clone, PartialEq)]
pub struct Pslg {
    vertices: Vec<Point2>,
    edges: Vec<InputEdge>,
}

impl Pslg {
    pub fn new(vertices: Vec<Point2>, edges: Vec<InputEdge>) -> Result<Self, InputError> {
        validate_pslg(&vertices, &edges)?;
        Ok(Self { vertices, edges })
    }

    /// Already-validated clone; `Polygon::new` already ran `validate_pslg`.
    pub(crate) fn from_validated(vertices: Vec<Point2>, edges: Vec<InputEdge>) -> Self {
        Self { vertices, edges }
    }

    pub fn from_flat(
        xy: &[[f64; 2]],
        edge_vertices: &[[usize; 2]],
        weights: Option<&[f64]>,
    ) -> Result<Self, InputError> {
        if xy.is_empty() {
            return Err(InputError::Empty);
        }
        let vertices: Vec<Point2> = xy.iter().copied().map(Point2::from_array).collect();
        let edges = match weights {
            None => edge_vertices
                .iter()
                .map(|e| InputEdge::new(e[0], e[1], 1.0))
                .collect(),
            Some(w) => {
                if w.len() != edge_vertices.len() {
                    return Err(InputError::Empty);
                }
                edge_vertices
                    .iter()
                    .zip(w.iter())
                    .map(|(e, &weight)| InputEdge::new(e[0], e[1], weight))
                    .collect()
            }
        };
        Self::new(vertices, edges)
    }

    #[inline]
    pub fn vertices(&self) -> &[Point2] {
        &self.vertices
    }

    #[inline]
    pub fn edges(&self) -> &[InputEdge] {
        &self.edges
    }

    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    #[inline]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

/// Polygon with optional holes. Ring winding is ignored; interior is the
/// odd-parity region of the constraint arrangement.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    /// All vertices in order: exterior then holes.
    vertices: Vec<Point2>,
    /// Edge list derived from rings (closed, last→first).
    edges: Vec<InputEdge>,
    /// `ring_offsets[i]` = start vertex index of ring i; final sentinel = vertex count.
    ring_offsets: Vec<usize>,
}

impl Polygon {
    pub fn new(exterior: Vec<Point2>, holes: Vec<Vec<Point2>>) -> Result<Self, InputError> {
        let mut rings = Vec::with_capacity(1 + holes.len());
        rings.push(exterior);
        rings.extend(holes);
        from_rings(rings)
    }

    /// Flat coordinates with ring start offsets. `ring_offsets` must include a
    /// trailing sentinel equal to `xy.len()`, or omit it (inferred).
    pub fn from_flat(xy: &[[f64; 2]], ring_offsets: &[usize]) -> Result<Self, InputError> {
        if xy.is_empty() || ring_offsets.is_empty() {
            return Err(InputError::Empty);
        }
        let mut offsets: Vec<usize> = ring_offsets.to_vec();
        if *offsets.last().unwrap() != xy.len() {
            offsets.push(xy.len());
        }
        if offsets[0] != 0 {
            return Err(InputError::Empty);
        }
        for w in offsets.windows(2) {
            if w[1] < w[0] {
                return Err(InputError::Empty);
            }
        }
        let mut rings = Vec::with_capacity(offsets.len() - 1);
        for w in offsets.windows(2) {
            let ring: Vec<Point2> = xy[w[0]..w[1]]
                .iter()
                .copied()
                .map(Point2::from_array)
                .collect();
            rings.push(ring);
        }
        from_rings(rings)
    }

    #[inline]
    pub fn vertices(&self) -> &[Point2] {
        &self.vertices
    }

    #[inline]
    pub fn edges(&self) -> &[InputEdge] {
        &self.edges
    }

    #[inline]
    pub fn ring_offsets(&self) -> &[usize] {
        &self.ring_offsets
    }

    #[inline]
    pub fn ring_count(&self) -> usize {
        self.ring_offsets.len().saturating_sub(1)
    }

    #[inline]
    pub fn hole_count(&self) -> usize {
        self.ring_count().saturating_sub(1)
    }

    /// View as a PSLG (shared topology; weight 1 on every edge).
    pub fn as_pslg(&self) -> Result<Pslg, InputError> {
        Ok(Pslg::from_validated(self.vertices.clone(), self.edges.clone()))
    }
}

fn from_rings(mut rings: Vec<Vec<Point2>>) -> Result<Polygon, InputError> {
    if rings.is_empty() {
        return Err(InputError::Empty);
    }
    for (ri, ring) in rings.iter_mut().enumerate() {
        strip_closing_duplicate(ring);
        if ring.len() < 3 {
            return Err(InputError::RingTooSmall { ring: ri });
        }
        for (i, p) in ring.iter().enumerate() {
            require_finite_point(*p, i)?;
        }
        for i in 0..ring.len() {
            let j = (i + 1) % ring.len();
            if ring[i] == ring[j] {
                return Err(InputError::DuplicateConsecutiveVertices {
                    ring: ri,
                    offset: i,
                });
            }
        }
    }

    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut ring_offsets = Vec::with_capacity(rings.len() + 1);
    ring_offsets.push(0);

    for ring in &rings {
        let base = vertices.len();
        let n = ring.len();
        vertices.extend_from_slice(ring);
        for i in 0..n {
            let v0 = base + i;
            let v1 = base + ((i + 1) % n);
            edges.push(InputEdge::new(v0, v1, 1.0));
        }
        ring_offsets.push(vertices.len());
    }

    validate_pslg(&vertices, &edges)?;
    // Additional ring nesting checks: holes must lie inside exterior via
    // point-in-polygon on their first vertex (strict for non-touching contract).
    if rings.len() > 1 {
        let exterior = &rings[0];
        for (hi, hole) in rings.iter().enumerate().skip(1) {
            let probe = hole[0];
            if !point_in_ring_odd_even(probe, exterior) {
                return Err(InputError::HoleOutsideExterior { hole: hi - 1 });
            }
            // Hole must not contain exterior vertices (crude nesting sanity).
            if point_in_ring_odd_even(exterior[0], hole) {
                return Err(InputError::HoleOutsideExterior { hole: hi - 1 });
            }
        }
    }

    Ok(Polygon {
        vertices,
        edges,
        ring_offsets,
    })
}

fn strip_closing_duplicate(ring: &mut Vec<Point2>) {
    if ring.len() >= 2 {
        let first = ring[0];
        let last = *ring.last().unwrap();
        if first == last {
            ring.pop();
        }
    }
}

fn validate_pslg(vertices: &[Point2], edges: &[InputEdge]) -> Result<(), InputError> {
    if vertices.is_empty() {
        return Err(InputError::Empty);
    }
    for (i, p) in vertices.iter().enumerate() {
        require_finite_point(*p, i)?;
    }
    let n = vertices.len();
    let mut seen = std::collections::BTreeSet::new();
    for (ei, e) in edges.iter().enumerate() {
        let v0 = e.vertices[0];
        let v1 = e.vertices[1];
        if v0 >= n || v1 >= n {
            return Err(InputError::VertexIndexOutOfRange {
                index: v0.max(v1),
                vertex_count: n,
            });
        }
        if v0 == v1 || vertices[v0] == vertices[v1] {
            return Err(InputError::ZeroLengthEdge { edge: ei, v0, v1 });
        }
        if !(e.weight.is_finite() && e.weight > 0.0) {
            return Err(InputError::NonPositiveWeight {
                edge: ei,
                weight: e.weight,
            });
        }
        let key = e.normalized_endpoints();
        if !seen.insert(key) {
            return Err(InputError::DuplicateEdge { v0: key.0, v1: key.1 });
        }
    }

    // Proper crossings between non-adjacent edges.
    for i in 0..edges.len() {
        let a0 = vertices[edges[i].vertices[0]];
        let a1 = vertices[edges[i].vertices[1]];
        let ia = edges[i].normalized_endpoints();
        for j in (i + 1)..edges.len() {
            let ib = edges[j].normalized_endpoints();
            // Share a vertex → adjacent or incident; not a proper crossing.
            if ia.0 == ib.0 || ia.0 == ib.1 || ia.1 == ib.0 || ia.1 == ib.1 {
                continue;
            }
            let b0 = vertices[edges[j].vertices[0]];
            let b1 = vertices[edges[j].vertices[1]];
            if segments_properly_intersect(a0, a1, b0, b1) {
                return Err(InputError::CrossingConstraints { e0: i, e1: j });
            }
        }
    }
    Ok(())
}

/// Odd-even fill rule; boundary counts as outside for hole nesting probes.
fn point_in_ring_odd_even(p: Point2, ring: &[Point2]) -> bool {
    let mut inside = false;
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let pi = ring[i];
        let pj = ring[j];
        let intersect = ((pi.y > p.y) != (pj.y > p.y))
            && (p.x
                < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_square_polygon() {
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
        assert_eq!(poly.vertex_count_via(), 4);
        assert_eq!(poly.edges().len(), 4);
        assert_eq!(poly.hole_count(), 0);
    }

    impl Polygon {
        fn vertex_count_via(&self) -> usize {
            self.vertices().len()
        }
    }

    #[test]
    fn closing_duplicate_stripped() {
        let poly = Polygon::from_flat(
            &[
                [0.0, 0.0],
                [2.0, 0.0],
                [2.0, 2.0],
                [0.0, 2.0],
                [0.0, 0.0],
            ],
            &[0],
        )
        .unwrap();
        assert_eq!(poly.vertices().len(), 4);
    }

    #[test]
    fn rejects_zero_weight() {
        let err = Pslg::from_flat(
            &[[0.0, 0.0], [1.0, 0.0]],
            &[[0, 1]],
            Some(&[0.0]),
        )
        .unwrap_err();
        assert!(matches!(err, InputError::NonPositiveWeight { .. }));
    }

    #[test]
    fn rejects_crossing() {
        let err = Pslg::from_flat(
            &[[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]],
            &[[0, 1], [2, 3]],
            None,
        )
        .unwrap_err();
        assert!(matches!(err, InputError::CrossingConstraints { .. }));
    }

    #[test]
    fn hole_inside_square() {
        let poly = Polygon::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(3.0, 0.0),
                Point2::new(3.0, 3.0),
                Point2::new(0.0, 3.0),
            ],
            vec![vec![
                Point2::new(1.0, 1.0),
                Point2::new(2.0, 1.0),
                Point2::new(2.0, 2.0),
                Point2::new(1.0, 2.0),
            ]],
        )
        .unwrap();
        assert_eq!(poly.hole_count(), 1);
        assert_eq!(poly.edges().len(), 8);
    }
}
