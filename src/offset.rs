//! Offset contours derived from the skeleton DCEL.
//!
//! Upstream: `SkeletonDCEL::make_offset` in `surf/src/SkeletonDCEL.cpp`.
//! No `z <= epsilon` heuristic: intersections use exact time comparisons.
//! Chains are joined through twin adjacency, not coordinate matching.

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

struct OffsetChain {
    face: usize,
    pts: Vec<Point2>,
    start_he: usize,
    end_he: usize,
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

    let mut pieces: Vec<OffsetChain> = Vec::new();
    for (fi, face) in skeleton.faces().iter().enumerate() {
        let Some(start_he) = face.halfedge else {
            continue;
        };
        collect_face_chains(skeleton, fi, start_he.0 as usize, time, &mut pieces)?;
    }

    Ok(join_by_twins(skeleton, pieces, time))
}

fn collect_face_chains(
    skeleton: &Skeleton,
    fi: usize,
    start_he: usize,
    time: f64,
    pieces: &mut Vec<OffsetChain>,
) -> Result<(), SkeletonError> {
    let n = skeleton.halfedges.len();
    let mut cycle: Vec<Option<(usize, Point2)>> = Vec::new();
    let mut he_id = start_he;
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > n + 2 {
            return Err(SkeletonError::Offset {
                time,
                detail: format!("face {fi} cycle did not close"),
            });
        }
        let hit = intersect_arc_at_time(&skeleton.halfedges[he_id].geometry, skeleton, he_id, time)
            .map(|p| (he_id, p));
        cycle.push(hit);
        let next = skeleton.halfedges[he_id].next.0 as usize;
        if next == start_he {
            break;
        }
        he_id = next;
    }
    if cycle.is_empty() {
        return Ok(());
    }

    // One chain per face: every hit on the CCB, in walk order. Misses are
    // other roof edges (input at t=0, later ridges) and do not split the
    // wavefront. Adjacent faces join through the twins of the first/last hit.
    let hits: Vec<(usize, Point2)> = cycle.into_iter().flatten().collect();
    if hits.len() >= 2 {
        let mut pts = Vec::new();
        for (_, p) in &hits {
            if pts.last().map(|q| *q != *p).unwrap_or(true) {
                pts.push(*p);
            }
        }
        if pts.len() >= 2 {
            pieces.push(OffsetChain {
                face: fi,
                start_he: hits[0].0,
                end_he: hits[hits.len() - 1].0,
                pts,
            });
        }
    }
    Ok(())
}

fn join_by_twins(
    skeleton: &Skeleton,
    mut pieces: Vec<OffsetChain>,
    time: f64,
) -> Vec<OffsetPolyline> {
    let n = pieces.len();
    if n == 0 {
        return Vec::new();
    }
    let mut start_of: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut end_of: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, p) in pieces.iter().enumerate() {
        start_of.insert(p.start_he, i);
        end_of.insert(p.end_he, i);
    }
    let mut used = vec![false; n];
    let mut out = Vec::new();

    for i in 0..n {
        if used[i] {
            continue;
        }
        used[i] = true;
        let face = pieces[i].face;
        let mut pts = std::mem::take(&mut pieces[i].pts);
        let mut start_he = pieces[i].start_he;
        let mut end_he = pieces[i].end_he;

        loop {
            let twin = skeleton.halfedges[end_he].twin.0 as usize;
            if let Some(&j) = start_of.get(&twin) {
                if !used[j] {
                    used[j] = true;
                    pts.extend(std::mem::take(&mut pieces[j].pts).into_iter().skip(1));
                    end_he = pieces[j].end_he;
                    continue;
                }
            }
            if let Some(&j) = end_of.get(&twin) {
                if !used[j] {
                    used[j] = true;
                    pts.extend(std::mem::take(&mut pieces[j].pts).into_iter().rev().skip(1));
                    end_he = pieces[j].start_he;
                    continue;
                }
            }
            break;
        }
        loop {
            let twin = skeleton.halfedges[start_he].twin.0 as usize;
            if let Some(&j) = end_of.get(&twin) {
                if !used[j] {
                    used[j] = true;
                    let mut prefix = std::mem::take(&mut pieces[j].pts);
                    prefix.pop();
                    prefix.append(&mut pts);
                    pts = prefix;
                    start_he = pieces[j].start_he;
                    continue;
                }
            }
            if let Some(&j) = start_of.get(&twin) {
                if !used[j] {
                    used[j] = true;
                    let mut prefix: Vec<Point2> = std::mem::take(&mut pieces[j].pts)
                        .into_iter()
                        .rev()
                        .collect();
                    prefix.pop();
                    prefix.append(&mut pts);
                    pts = prefix;
                    start_he = pieces[j].end_he;
                    continue;
                }
            }
            break;
        }

        let closed = skeleton.halfedges[end_he].twin.0 as usize == start_he && pts.len() >= 3;
        if closed && pts.first() != pts.last() {
            pts.push(pts[0]);
        }
        if pts.len() < 2 {
            continue;
        }
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
            Some(Point2::new(
                ray.origin.x + dt * ray.direction.x,
                ray.origin.y + dt * ray.direction.y,
            ))
        }
        ArcGeometry::InfinityChord => None,
    }
}
