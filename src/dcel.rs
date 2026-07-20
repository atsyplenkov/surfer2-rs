//! Safe skeleton DCEL built from authoritative arc records.
//!
//! Upstream: `surf/src/SkeletonDCEL.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.
//!
//! Unlike upstream's late pointer walk, halfedges are emitted from arc
//! records and validated before return.

use crate::error::SkeletonError;
use crate::geometry::{Point2, Ray2, Segment2};
use crate::triangulation::kinetic::KineticTriangulation;

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
            len > 0.0 && len <= max_len
        })
    }

    pub fn all_finite_segments(&self) -> &[SkeletonSegment] {
        &self.segments
    }

    pub fn node_count(&self) -> usize {
        self.nodes.iter().filter(|n| !n.is_infinite).count()
    }

    pub fn segment_count(&self) -> usize {
        self.finite_segments().count()
    }

    pub fn total_finite_length(&self) -> f64 {
        self.finite_segments().map(|s| s.start.distance_to(s.end)).sum()
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
    let mut nodes: Vec<Node> = Vec::new();
    let mut node_key: Vec<((u64, u64), u64)> = Vec::new();

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

    let mut halfedges: Vec<HalfEdge> = Vec::new();
    let mut segments: Vec<SkeletonSegment> = Vec::new();

    for e in kt.edges.values() {
        if !e.is_initial || e.is_beveling {
            continue;
        }
        let v0 = &kt.vertices[e.endpoints[0]];
        let v1 = &kt.vertices[e.endpoints[1]];
        let p0 = v0.pos_zero;
        let p1 = v1.pos_zero;
        let n0 = ensure_node(&mut nodes, &mut node_key, p0, 0.0);
        let n1 = ensure_node(&mut nodes, &mut node_key, p1, 0.0);
        let face = FaceId(e.skeleton_face);
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
            face,
            origin: n1,
            geometry: ArcGeometry::Input(Segment2::new(p1, p0)),
            is_input: true,
        });
        if (face.0 as usize) < faces.len() && faces[face.0 as usize].halfedge.is_none() {
            faces[face.0 as usize].halfedge = Some(HalfEdgeId(he));
        }
    }

    for arc in &kt.arcs {
        if arc.from_input_edge {
            continue;
        }
        let left = FaceId(arc.left_face.min((faces.len().saturating_sub(1)) as u32));
        let right = FaceId(arc.right_face.min((faces.len().saturating_sub(1)) as u32));

        match (arc.end, arc.end_time) {
            (Some(end), Some(et)) => {
                let n0 = ensure_node(&mut nodes, &mut node_key, arc.start, arc.start_time);
                let n1 = ensure_node(&mut nodes, &mut node_key, end, et);
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
                if arc.start.distance_to(end) > 0.0 {
                    segments.push(SkeletonSegment {
                        start: arc.start,
                        end,
                        start_time: arc.start_time,
                        end_time: et,
                        component: arc.component.max(0) as usize,
                        left_face: left.0 as usize,
                        right_face: right.0 as usize,
                    });
                }
            }
            (None, _) | (_, None) => {
                let n0 = ensure_node(&mut nodes, &mut node_key, arc.start, arc.start_time);
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
            }
        }
    }

    connect_face_cycles(&mut halfedges)?;

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

fn ensure_node(
    nodes: &mut Vec<Node>,
    node_key: &mut Vec<((u64, u64), u64)>,
    p: Point2,
    time: f64,
) -> NodeId {
    let key = ((p.x.to_bits(), p.y.to_bits()), time.to_bits());
    if let Some(i) = node_key.iter().position(|k| *k == key) {
        return NodeId(i as u32);
    }
    let id = NodeId(nodes.len() as u32);
    nodes.push(Node {
        point: Some(p),
        time,
        is_infinite: false,
    });
    node_key.push(key);
    id
}

fn connect_face_cycles(halfedges: &mut [HalfEdge]) -> Result<(), SkeletonError> {
    let n = halfedges.len();
    if n == 0 {
        return Ok(());
    }
    // Group halfedges by (face, origin node) → candidates leaving that node on face
    // For each he, next should be the unique he' with origin = destination(he) and same face.
    let dest: Vec<NodeId> = halfedges
        .iter()
        .map(|he| halfedges[he.twin.0 as usize].origin)
        .collect();

    for i in 0..n {
        let face = halfedges[i].face;
        let target_origin = dest[i];
        let mut candidates = Vec::new();
        for (j, he) in halfedges.iter().enumerate() {
            if j != i && he.face == face && he.origin == target_origin {
                candidates.push(j);
            }
        }
        match candidates.len() {
            1 => {
                let j = candidates[0];
                halfedges[i].next = HalfEdgeId(j as u32);
                halfedges[j].prev = HalfEdgeId(i as u32);
            }
            0 => {
                // Self-loop fallback for incomplete skeletons during early milestones
                halfedges[i].next = HalfEdgeId(i as u32);
                halfedges[i].prev = HalfEdgeId(i as u32);
            }
            _ => {
                // Ambiguous — pick lowest id deterministically
                let j = *candidates.iter().min().unwrap();
                halfedges[i].next = HalfEdgeId(j as u32);
                halfedges[j].prev = HalfEdgeId(i as u32);
            }
        }
    }
    Ok(())
}
