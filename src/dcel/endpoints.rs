//! Endpoint keys and node union for DCEL emission.
//!
//! Arc ends are named by kinetic vertex identity (`Start` / `Stop` /
//! `TraceAt`). `TraceAt` splits the host vertex's traced ridge at the
//! landing time so a spoke shares that node — no geometric search.

use std::collections::HashMap;

use crate::dcel::{Node, NodeId};
use crate::geometry::Point2;
use crate::triangulation::kinetic::{ArcEnd, ArcRecord};
use crate::triangulation::KineticTriangulation;
use crate::wavefront::vertex::VertexId;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum EndKey {
    Start(u64),
    Stop(u64),
    TraceAt { v: u64, tbits: u64 },
    Unique(u64),
}

impl EndKey {
    pub(super) fn start(v: VertexId) -> Self {
        Self::Start(slotmap_key(v))
    }
    pub(super) fn stop(v: VertexId) -> Self {
        Self::Stop(slotmap_key(v))
    }
    fn trace_at(v: VertexId, tbits: u64) -> Self {
        Self::TraceAt {
            v: slotmap_key(v),
            tbits,
        }
    }
    fn unique(tag: u64) -> Self {
        Self::Unique(tag)
    }
}

fn slotmap_key(v: VertexId) -> u64 {
    use slotmap::Key;
    v.data().as_ffi()
}

pub(super) struct EndpointUf {
    parent: HashMap<EndKey, EndKey>,
    uniq: u64,
}

impl EndpointUf {
    pub(super) fn from_links(kt: &KineticTriangulation) -> Self {
        let mut uf = Self {
            parent: HashMap::new(),
            uniq: 0,
        };
        for (id, v) in &kt.vertices {
            for side in 0..2 {
                if let Some(n) = v.next_vertex[side] {
                    let nv = &kt.vertices[n];
                    if nv.prev_vertex[side] == Some(id) || nv.next_vertex[1 - side] != Some(id) {
                        uf.union(EndKey::stop(id), EndKey::start(n));
                    }
                    if nv.next_vertex[1 - side] == Some(id) {
                        uf.union(EndKey::stop(id), EndKey::stop(n));
                    }
                }
                if let Some(p) = v.prev_vertex[side] {
                    if kt.vertices[p].prev_vertex[1 - side] == Some(id) {
                        uf.union(EndKey::start(id), EndKey::start(p));
                    }
                }
            }
        }
        uf
    }

    fn find(&mut self, k: EndKey) -> EndKey {
        let p = *self.parent.entry(k).or_insert(k);
        if p != k {
            let r = self.find(p);
            self.parent.insert(k, r);
            r
        } else {
            k
        }
    }

    pub(super) fn union(&mut self, a: EndKey, b: EndKey) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }

    fn fresh(&mut self) -> EndKey {
        self.uniq += 1;
        EndKey::unique(self.uniq)
    }
}

pub(super) fn end_key(end: ArcEnd) -> EndKey {
    match end {
        ArcEnd::Start(v) => EndKey::start(v),
        ArcEnd::Stop(v) => EndKey::stop(v),
        ArcEnd::TraceAt { vertex, time_bits } => EndKey::trace_at(vertex, time_bits),
    }
}

pub(super) fn arc_keys(arc: &ArcRecord) -> (EndKey, EndKey) {
    (
        end_key(arc.start_end),
        match arc.end_end {
            Some(e) => end_key(e),
            None => EndKey::unique(0),
        },
    )
}

pub(super) fn ensure_node(
    uf: &mut EndpointUf,
    nodes: &mut Vec<Node>,
    root_node: &mut HashMap<EndKey, u32>,
    key: EndKey,
    p: Point2,
    time: f64,
) -> NodeId {
    let key = if matches!(key, EndKey::Unique(0)) {
        uf.fresh()
    } else {
        key
    };
    let r = uf.find(key);
    if let Some(&i) = root_node.get(&r) {
        return NodeId(i);
    }
    let id = nodes.len() as u32;
    nodes.push(Node {
        point: Some(p),
        time,
        is_infinite: false,
    });
    root_node.insert(r, id);
    NodeId(id)
}

/// Split traced ridges through every `TraceAt` landing on the same vertex.
/// Also returns identity unions for omitted zero-length pieces (same vertex).
pub(super) fn expand_arcs(arcs: &[ArcRecord]) -> (Vec<ArcRecord>, Vec<(EndKey, EndKey)>) {
    let mut hits: HashMap<u64, Vec<(u64, Point2)>> = HashMap::new();
    for a in arcs {
        if let Some(ArcEnd::TraceAt { vertex, time_bits }) = a.end_end {
            if let Some(p) = a.end {
                hits.entry(slotmap_key(vertex))
                    .or_default()
                    .push((time_bits, p));
            }
        }
    }
    for list in hits.values_mut() {
        list.sort_by(|(a, _), (b, _)| f64::from_bits(*a).total_cmp(&f64::from_bits(*b)));
        list.dedup_by_key(|(t, _)| *t);
    }

    let mut out = Vec::with_capacity(arcs.len());
    let mut unions = Vec::new();
    for a in arcs {
        if let Some(v) = a.traced_vertex() {
            if let Some(hs) = hits.get(&slotmap_key(v)) {
                if !hs.is_empty() {
                    let (pieces, u) = split_traced(a, v, hs);
                    out.extend(pieces);
                    unions.extend(u);
                    continue;
                }
            }
        }
        out.push(a.clone());
    }
    (out, unions)
}

fn split_traced(
    arc: &ArcRecord,
    v: VertexId,
    hits: &[(u64, Point2)],
) -> (Vec<ArcRecord>, Vec<(EndKey, EndKey)>) {
    let t0 = arc.start_time;
    let t0b = t0.to_bits();
    let mut pts: Vec<(ArcEnd, f64, Point2)> = vec![(ArcEnd::Start(v), t0, arc.start)];

    match (arc.end, arc.end_time) {
        (Some(end), Some(et)) => {
            let etb = et.to_bits();
            let mut extra = Vec::new();
            for &(tb, p) in hits {
                let t = f64::from_bits(tb);
                if tb == t0b {
                    extra.push((
                        end_key(ArcEnd::TraceAt {
                            vertex: v,
                            time_bits: tb,
                        }),
                        EndKey::start(v),
                    ));
                    continue;
                }
                if tb == etb {
                    extra.push((
                        end_key(ArcEnd::TraceAt {
                            vertex: v,
                            time_bits: tb,
                        }),
                        EndKey::stop(v),
                    ));
                    continue;
                }
                if t > t0 && t < et {
                    pts.push((
                        ArcEnd::TraceAt {
                            vertex: v,
                            time_bits: tb,
                        },
                        t,
                        p,
                    ));
                }
            }
            pts.push((ArcEnd::Stop(v), et, end));
            let (pieces, u) = pieces_between(arc, &pts);
            extra.extend(u);
            (pieces, extra)
        }
        _ => {
            for &(tb, p) in hits {
                let t = f64::from_bits(tb);
                if tb == t0b || t <= t0 {
                    continue;
                }
                pts.push((
                    ArcEnd::TraceAt {
                        vertex: v,
                        time_bits: tb,
                    },
                    t,
                    p,
                ));
            }
            if pts.len() == 1 {
                return (vec![arc.clone()], Vec::new());
            }
            let (mut out, unions) = pieces_between(arc, &pts);
            if let Some(&(id, t, p)) = pts.last() {
                let mut ray = arc.clone();
                ray.start = p;
                ray.end = None;
                ray.start_time = t;
                ray.end_time = None;
                ray.start_end = id;
                ray.end_end = None;
                out.push(ray);
            }
            if out.is_empty() {
                (vec![arc.clone()], unions)
            } else {
                (out, unions)
            }
        }
    }
}

fn pieces_between(
    arc: &ArcRecord,
    pts: &[(ArcEnd, f64, Point2)],
) -> (Vec<ArcRecord>, Vec<(EndKey, EndKey)>) {
    let mut out = Vec::new();
    let mut unions = Vec::new();
    for w in pts.windows(2) {
        let (id0, t0, p0) = w[0];
        let (id1, t1, p1) = w[1];
        if !(p0.distance_to(p1) > 0.0) {
            unions.push((end_key(id0), end_key(id1)));
            continue;
        }
        let mut piece = arc.clone();
        piece.start = p0;
        piece.end = Some(p1);
        piece.start_time = t0;
        piece.end_time = Some(t1);
        piece.start_end = id0;
        piece.end_end = Some(id1);
        out.push(piece);
    }
    if out.is_empty() && pts.len() <= 2 {
        (vec![arc.clone()], unions)
    } else {
        (out, unions)
    }
}
