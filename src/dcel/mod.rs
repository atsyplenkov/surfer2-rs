//! Safe skeleton DCEL built from authoritative arc records.
//!
//! Upstream: `surf/src/SkeletonDCEL.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.
//!
//! Unlike upstream's late pointer walk, halfedges are emitted from arc
//! records and validated before return.

mod cycles;
mod endpoints;

use std::collections::HashMap;

use crate::error::SkeletonError;
use crate::geometry::{Point2, Ray2, Segment2};
use crate::triangulation::KineticTriangulation;
use crate::wavefront::vertex::VertexId;

use cycles::{connect_face_cycles, validate_bounded_face_cycles};
use endpoints::{arc_keys, ensure_node, expand_arcs, EndKey, EndpointUf};

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
    /// Topological closure where C++ skips the infinite vertex.
    InfinityChord,
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
    /// Input-footprint bbox diagonal (original PSLG vertices at t=0).
    footprint_extent: f64,
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

    /// Finite skeleton segments after the documented runaway cutoff.
    ///
    /// Emission stores every finite-looking arc ([`Self::stored_segments`]).
    /// This iterator is the public filtered view used by [`Self::segment_count`],
    /// [`Self::node_count`], and [`Self::total_finite_length`].
    ///
    /// A segment is kept when its endpoints and times are finite,
    /// `end_time >= start_time >= 0`, and its length `ℓ` satisfies
    /// `1e-15 < ℓ ≤ 2 · D`. `D` is the axis-aligned bounding-box diagonal of
    /// the input footprint (original PSLG vertices at t=0), the same extent
    /// used for junction epsilon, scene gap, and endpoint-union tolerance.
    /// An empty or degenerate footprint floors `D` at 1 on that multiplier.
    pub fn finite_segments(&self) -> impl Iterator<Item = SkeletonSegment> + '_ {
        let max_len = self.footprint_extent.max(1.0) * 2.0;
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
            len > 1e-15 && len <= max_len
        })
    }

    /// Unfiltered stored skeleton segments, including any the finite view
    /// would drop as runaway or non-finite.
    pub fn stored_segments(&self) -> &[SkeletonSegment] {
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

/// Build a Skeleton that satisfies the production DCEL contract.
///
/// This is the only constructor from a finished kinetic triangulation.
/// Face-walk failures are retained in `IncompleteFaceCycle` diagnostics when
/// coverage cannot be completed; they are never discarded behind an unchecked
/// `Ok` return.
///
/// ```compile_fail
/// // Unchecked emission without bounded-face validation is not public.
/// use surfer2::dcel::emit_skeleton;
/// ```
pub fn build_dcel(kt: &KineticTriangulation) -> Result<Skeleton, SkeletonError> {
    let (sk, walk_failures) = emit_skeleton_with_diagnostics(kt)?;
    let unbounded = FaceId(sk.faces.len().saturating_sub(1) as u32);
    match validate_bounded_face_cycles(&sk.halfedges, &sk.faces, unbounded) {
        // Non-empty walk_failures here mean repair resolved them; validation passed.
        Ok(()) => Ok(sk),
        Err(SkeletonError::IncompleteFaceCycle { mut detail }) => {
            append_face_walk_failures(&mut detail, &walk_failures);
            Err(SkeletonError::IncompleteFaceCycle { detail })
        }
        Err(error) => Err(error),
    }
}

fn append_face_walk_failures(detail: &mut String, walk_failures: &[String]) {
    if walk_failures.is_empty() {
        return;
    }
    detail.push_str("; face-walk failures: ");
    detail.push_str(&walk_failures.join("; "));
}

fn emit_skeleton_with_diagnostics(
    kt: &KineticTriangulation,
) -> Result<(Skeleton, Vec<String>), SkeletonError> {
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
    let emit_face = |f: u32| {
        if (f as usize) < n_skel_faces {
            FaceId(f)
        } else {
            unbounded
        }
    };

    let mut halfedges: Vec<HalfEdge> = Vec::new();
    let mut segments: Vec<SkeletonSegment> = Vec::new();
    let mut face_base: HashMap<u32, u32> = HashMap::new();
    let mut vertex_left_he: HashMap<VertexId, u32> = HashMap::new();
    let mut ridge_pieces: HashMap<VertexId, Vec<(u64, u32)>> = HashMap::new();
    let mut infinite_node = None;

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

    let (expanded, identity_unions) = expand_arcs(&kt.arcs);
    for (a, b) in identity_unions {
        uf.union(a, b);
    }
    for arc in &expanded {
        if arc.from_input_edge {
            continue;
        }
        let left = emit_face(arc.left_face);
        let right = emit_face(arc.right_face);

        match (arc.end, arc.end_time) {
            (Some(end), Some(et)) => {
                if !(arc.start.distance_to(end) > 0.0) {
                    continue;
                }
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
                if let Some(v) = arc.traced_vertex() {
                    ridge_pieces
                        .entry(v)
                        .or_default()
                        .push((arc.start_time.to_bits(), he));
                    if arc.ridge_origin_vertex() == Some(v) {
                        vertex_left_he.entry(v).or_insert(he);
                    }
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
                let inf_id = *infinite_node.get_or_insert_with(|| {
                    let id = NodeId(nodes.len() as u32);
                    nodes.push(Node {
                        point: None,
                        time: f64::INFINITY,
                        is_infinite: true,
                    });
                    id
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
                if let Some(v) = arc.traced_vertex() {
                    ridge_pieces
                        .entry(v)
                        .or_default()
                        .push((arc.start_time.to_bits(), he));
                    if arc.ridge_origin_vertex() == Some(v) {
                        vertex_left_he.entry(v).or_insert(he);
                    }
                }
            }
        }
    }

    let walk_failures = connect_face_cycles(
        kt,
        &mut halfedges,
        &mut faces,
        &vertex_left_he,
        &face_base,
        &ridge_pieces,
        &nodes,
        unbounded,
    )?;

    for (i, he) in halfedges.iter().enumerate() {
        let twin = he.twin.0 as usize;
        if twin >= halfedges.len() || halfedges[twin].twin.0 != i as u32 {
            let mut detail = format!("twin reciprocity failed at halfedge {i}");
            append_face_walk_failures(&mut detail, &walk_failures);
            return Err(SkeletonError::NonManifoldTopology { detail });
        }
    }

    Ok((
        Skeleton {
            nodes,
            halfedges,
            faces,
            segments,
            footprint_extent: kt.footprint_extent(),
        },
        walk_failures,
    ))
}

#[cfg(test)]
mod tests {
    use super::{append_face_walk_failures, build_dcel};
    use crate::input::Polygon;
    use crate::propagator::Propagator;

    fn finished_square() -> Propagator {
        let poly =
            Polygon::from_flat(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], &[0]).unwrap();
        let mut prop = Propagator::from_polygon(&poly).unwrap();
        while prop.step().unwrap() {}
        prop
    }

    #[test]
    fn build_dcel_returns_validated_skeleton_for_finished_propagation() {
        let prop = finished_square();
        let sk = build_dcel(&prop.kt).expect("validated skeleton");
        assert_eq!(sk.segment_count(), 4);
        assert_eq!(sk.node_count(), 5);
        let unbounded = sk.faces().len().saturating_sub(1);
        for (fi, face) in sk.faces().iter().enumerate() {
            if fi == unbounded {
                continue;
            }
            let Some(start) = face.halfedge else {
                panic!("bounded face {fi} has no root");
            };
            let mut he = start.0 as usize;
            let mut closed = false;
            for _ in 0..=sk.halfedges().len() {
                assert_eq!(sk.halfedges()[he].face.0 as usize, fi);
                he = sk.halfedges()[he].next.0 as usize;
                if he == start.0 as usize {
                    closed = true;
                    break;
                }
            }
            assert!(closed, "bounded face {fi} cycle did not close");
        }
    }

    #[test]
    fn face_walk_failures_are_retained_on_incomplete_cycle_detail() {
        let mut detail =
            "stage=bounded-validation face=3 halfedge=9: not reachable from face root".to_string();
        append_face_walk_failures(
            &mut detail,
            &["stage=face-walk face=3 endpoint=VertexId(7) side=left: stuck".to_string()],
        );
        assert!(detail.contains("stage=bounded-validation"), "{detail}");
        assert!(detail.contains("face-walk failures:"), "{detail}");
        assert!(detail.contains("endpoint=VertexId(7)"), "{detail}");
        let mut empty = "stage=bounded-validation face=0: has no root".to_string();
        append_face_walk_failures(&mut empty, &[]);
        assert_eq!(empty, "stage=bounded-validation face=0: has no root");
    }
}
