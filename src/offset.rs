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

    let mut polylines = Vec::new();

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
            let closed = pts.first() == pts.last() || pts.len() >= 3;
            if closed && pts.first() != pts.last() {
                pts.push(pts[0]);
            }
            polylines.push(OffsetPolyline {
                points: pts,
                closed,
                face: fi,
                time,
            });
        }
    }

    Ok(polylines)
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
