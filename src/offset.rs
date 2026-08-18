//! Offset contours derived from the skeleton DCEL.
//!
//! Upstream: `SkeletonDCEL::make_offset` in `surf/src/SkeletonDCEL.cpp`.
//! No `z <= epsilon` heuristic: intersections use exact time comparisons.

use crate::dcel::{ArcGeometry, Skeleton};
use crate::error::SkeletonError;
use crate::geometry::Point2;

#[derive(Debug, Clone, PartialEq)]
pub struct OffsetPolyline {
    pub points: Vec<Point2>,
    pub closed: bool,
    pub face: usize,
    pub time: f64,
}

/// Compute offset polylines at roof height / wavefront time `time`.
pub fn offset(skeleton: &Skeleton, time: f64) -> Result<Vec<OffsetPolyline>, SkeletonError> {
    if !time.is_finite() || time < 0.0 {
        return Err(SkeletonError::Offset {
            time,
            detail: "offset time must be finite and non-negative".into(),
        });
    }
    if skeleton.halfedges.is_empty() {
        return Ok(vec![]);
    }

    let mut chains: Vec<(usize, Vec<Point2>)> = Vec::new();

    for (fi, face) in skeleton.faces().iter().enumerate() {
        let Some(start_he) = face.halfedge else {
            continue;
        };
        let mut pts = Vec::new();
        let mut he_id = start_he.0 as usize;
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > skeleton.halfedges.len() + 2 {
                break;
            }
            let he = &skeleton.halfedges[he_id];
            if let Some(p) = intersect_arc_at_time(&he.geometry, skeleton, he_id, time) {
                if pts.last().map(|q| *q != p).unwrap_or(true) {
                    pts.push(p);
                }
            }
            let next = he.next.0 as usize;
            if next == start_he.0 as usize {
                break;
            }
            he_id = next;
        }
        if pts.len() >= 2 {
            chains.push((fi, pts));
        }
    }

    Ok(stitch_offset_chains(chains, time))
}

fn stitch_offset_chains(chains: Vec<(usize, Vec<Point2>)>, time: f64) -> Vec<OffsetPolyline> {
    let n = chains.len();
    if n == 0 {
        return Vec::new();
    }
    let mut used = vec![false; n];
    let mut out = Vec::new();

    for i in 0..n {
        if used[i] {
            continue;
        }
        used[i] = true;
        let face = chains[i].0;
        let mut pts = chains[i].1.clone();
        let mut grew = true;
        while grew {
            grew = false;
            for j in 0..n {
                if used[j] {
                    continue;
                }
                let c = &chains[j].1;
                let Some(&head) = pts.first() else { break };
                let Some(&tail) = pts.last() else { break };
                if tail == c[0] {
                    pts.extend(c.iter().copied().skip(1));
                } else if tail == *c.last().unwrap() {
                    pts.extend(c.iter().rev().copied().skip(1));
                } else if head == *c.last().unwrap() {
                    let mut prefix: Vec<Point2> = c[..c.len() - 1].to_vec();
                    prefix.append(&mut pts);
                    pts = prefix;
                } else if head == c[0] {
                    let mut prefix: Vec<Point2> = c.iter().skip(1).rev().copied().collect();
                    prefix.append(&mut pts);
                    pts = prefix;
                } else {
                    continue;
                }
                used[j] = true;
                grew = true;
            }
        }
        if pts.len() < 2 {
            continue;
        }
        if pts.len() >= 3 && pts.first() != pts.last() {
            if let (Some(&a), Some(&b)) = (pts.first(), pts.last()) {
                if a == b || a.distance_to(b) <= 1e-12 {
                    pts.push(pts[0]);
                }
            }
        }
        let closed = pts.len() >= 3 && pts.first() == pts.last();
        out.push(OffsetPolyline {
            points: pts,
            closed,
            face,
            time,
        });
    }
    out
}

fn intersect_arc_at_time(
    geom: &ArcGeometry,
    skeleton: &Skeleton,
    he_id: usize,
    time: f64,
) -> Option<Point2> {
    let he = &skeleton.halfedges[he_id];
    let origin = &skeleton.nodes[he.origin.0 as usize];
    let dest_node = &skeleton.nodes[skeleton.halfedges[he.twin.0 as usize].origin.0 as usize];

    match geom {
        ArcGeometry::Input(seg) => {
            if time == 0.0 {
                Some(seg.start)
            } else {
                None
            }
        }
        ArcGeometry::Segment(seg) => {
            let t0 = origin.time;
            let t1 = dest_node.time;
            if !t0.is_finite() || !t1.is_finite() {
                return None;
            }
            let (t_lo, t_hi, p_lo, p_hi) = if t0 <= t1 {
                (t0, t1, seg.start, seg.end)
            } else {
                (t1, t0, seg.end, seg.start)
            };
            if time < t_lo || time > t_hi {
                return None;
            }
            if t_hi == t_lo {
                return Some(p_lo);
            }
            let alpha = (time - t_lo) / (t_hi - t_lo);
            Some(Point2::new(
                p_lo.x + alpha * (p_hi.x - p_lo.x),
                p_lo.y + alpha * (p_hi.y - p_lo.y),
            ))
        }
        ArcGeometry::Ray(ray) => {
            let t0 = origin.time;
            if !t0.is_finite() || time < t0 {
                return None;
            }
            let dt = time - t0;
            // Direction in ray is not time-parameterized unit speed; full port
            // stores velocity. Use direction as velocity proxy.
            Some(Point2::new(
                ray.origin.x + dt * ray.direction.x,
                ray.origin.y + dt * ray.direction.y,
            ))
        }
    }
}
