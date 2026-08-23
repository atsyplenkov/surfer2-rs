//! Collapse-event prediction for the kinetic triangulation.
//!
//! Upstream: `surf/src/KineticTriangle.*` compute_collapse (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.

use crate::error::PropagationError;
use crate::event::{CollapseSpec, CollapseType};
use crate::geometry::Point2;
use crate::triangulation::kinetic::{ccw, cw, KineticTriangle, KineticTriangulation};
use crate::wavefront::supporting_line::SupportingLine;
use crate::wavefront::vertex::{InfiniteSpeedType, WavefrontVertex};

pub fn compute_collapse(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    if let Some(c) = t.collapse_cache {
        return Ok(c);
    }
    if t.is_dead || t.is_dying {
        return Ok(CollapseSpec::never(t.component));
    }

    // Infinite speed check
    let mut inf = InfiniteSpeedType::None;
    for i in 0..3 {
        if let Some(vid) = t.vertices[i] {
            if let Some(v) = kt.vertices.get(vid) {
                match v.infinite_speed {
                    InfiniteSpeedType::Opposing => {
                        inf = InfiniteSpeedType::Opposing;
                        break;
                    }
                    InfiniteSpeedType::Weighted => inf = InfiniteSpeedType::Weighted,
                    InfiniteSpeedType::None => {}
                }
            }
        }
    }
    if inf == InfiniteSpeedType::Opposing {
        return Ok(CollapseSpec::new(
            t.component,
            CollapseType::FaceHasInfinitelyFastVertexOpposing,
            time_now,
        ));
    }
    if inf == InfiniteSpeedType::Weighted {
        // Pick constrained edge with max weight
        let mut best_edge = -1i8;
        let mut best_w = f64::NEG_INFINITY;
        for s in 0..3 {
            if let Some(eid) = t.wavefronts[s] {
                if let Some(e) = kt.edges.get(eid) {
                    let w = e.supporting_line.weight;
                    if w > best_w {
                        best_w = w;
                        best_edge = s as i8;
                    }
                }
            }
        }
        if best_edge >= 0 {
            return Ok(CollapseSpec::with_edge_secondary(
                t.component,
                CollapseType::FaceHasInfinitelyFastVertexWeighted,
                time_now,
                best_edge,
                best_w,
            ));
        }
        return Ok(CollapseSpec::new(
            t.component,
            CollapseType::InvalidEvent,
            time_now,
        ));
    }

    // Count constraints
    let mut n_const = 0;
    let mut const_sides = [false; 3];
    for s in 0..3 {
        if t.wavefronts[s].is_some() {
            const_sides[s] = true;
            n_const += 1;
        }
    }

    let unbounded = t.is_unbounded
        || kt
            .infinite_vertex
            .is_some_and(|inf| t.vertices.iter().any(|v| *v == Some(inf)));
    if unbounded {
        return compute_unbounded_fast(kt, t, time_now);
    }

    match n_const {
        3 => {
            // C++ compute_collapse_bounded_constrained_3: 3-const is always
            // a triangle collapse (asserts all three edges collapse together).
            let mut tmin = f64::INFINITY;
            for s in 0..3 {
                if let Some(tc) = constraint_collapse_time(kt, t, s, time_now) {
                    tmin = tmin.min(tc);
                }
            }
            Ok(CollapseSpec::new(
                t.component,
                CollapseType::TriangleCollapse,
                if tmin.is_finite() { tmin } else { time_now },
            ))
        }
        2 => collapse_bounded_constrained_2(kt, t, time_now),
        1 => collapse_bounded_constrained_1(kt, t, time_now),
        0 => compute_flip_event_fast(kt, t, time_now),
        _ => Ok(CollapseSpec::never(t.component)),
    }
}

fn collapse_bounded_constrained_2(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    let mut specs = Vec::new();
    for s in 0..3 {
        if t.wavefronts[s].is_none() {
            continue;
        }
        if let Some(tc) = constraint_collapse_time(kt, t, s, time_now) {
            specs.push(CollapseSpec::with_edge(
                t.component,
                CollapseType::ConstraintCollapse,
                tc,
                s as i8,
            ));
        }
    }
    match specs.len() {
        0 => {
            if signed_area_at(kt, t, time_now) < 0.0 {
                Ok(CollapseSpec::new(
                    t.component,
                    CollapseType::TriangleCollapse,
                    time_now,
                ))
            } else {
                Ok(CollapseSpec::never(t.component))
            }
        }
        1 => Ok(specs[0]),
        _ => {
            if (specs[0].time - specs[1].time).abs() <= 1e-15 * (1.0 + specs[0].time.abs()) {
                Ok(CollapseSpec::new(
                    t.component,
                    CollapseType::TriangleCollapse,
                    specs[0].time.min(specs[1].time),
                ))
            } else if specs[0] < specs[1] {
                Ok(specs[0])
            } else {
                Ok(specs[1])
            }
        }
    }
}

fn collapse_bounded_constrained_1(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    use crate::event::EdgeCollapseType;
    use crate::numeric::determinant_poly_f64;
    use crate::wavefront::edge::compute_edge_collapse_f64;

    let c_idx = (0..3).find(|&s| t.wavefronts[s].is_some()).unwrap();
    let Some(eid) = t.wavefronts[c_idx] else {
        return Ok(CollapseSpec::never(t.component));
    };
    let Some(e) = kt.edges.get(eid) else {
        return Ok(CollapseSpec::never(t.component));
    };
    let (Some(v0), Some(v1)) = (
        kt.vertices.get(e.endpoints[0]),
        kt.vertices.get(e.endpoints[1]),
    ) else {
        return Ok(CollapseSpec::never(t.component));
    };
    // After a late flip the opposite vertex can already sit on the new
    // constraint while a later det root hides the split. Catch up only
    // when the hit is on-segment and at most 1e-4 s in the past.
    if missed_on_segment_split(kt, t, c_idx, time_now) {
        return Ok(CollapseSpec::with_edge(
            t.component,
            CollapseType::SplitOrFlipRefine,
            time_now,
            c_idx as i8,
        ));
    }
    // C++ WavefrontEdge::parallel_endpoints: ALWAYS or NEVER (collinear velocities).
    let edge =
        compute_edge_collapse_f64(v0.pos_zero, v0.velocity, v1.pos_zero, v1.velocity, time_now);

    let mut ps = [[0.0; 2]; 3];
    let mut vs = [[0.0; 2]; 3];
    for i in 0..3 {
        let vid = t.vertices[i].unwrap();
        let v = &kt.vertices[vid];
        ps[i] = [v.pos_zero.x, v.pos_zero.y];
        vs[i] = [v.velocity.x, v.velocity.y];
    }
    let det = determinant_poly_f64(ps[0], vs[0], ps[1], vs[1], ps[2], vs[2]);

    let parallel = matches!(
        edge.type_,
        EdgeCollapseType::Always | EdgeCollapseType::Never
    );
    if parallel {
        if edge.type_ == EdgeCollapseType::Always
            || leftover_constraint_already_a_point(v0, v1, time_now)
        {
            return Ok(CollapseSpec::with_edge(
                t.component,
                CollapseType::ConstraintCollapse,
                time_now,
                c_idx as i8,
            ));
        }
        // NEVER: only a linear area polynomial can still be a split/flip.
        // Degree 0 (already-flat leftover) or 2 → C++ emits NEVER, not a
        // now-time SplitOrFlipRefine on the zero-length constraint.
        if det.degree() == 1 {
            return split_or_flip_from_det(t, c_idx, det, time_now);
        }
        return Ok(CollapseSpec::never(t.component));
    }

    let candidate = if edge.type_ == EdgeCollapseType::Future {
        Some(CollapseSpec::with_edge(
            t.component,
            CollapseType::ConstraintCollapse,
            edge.time,
            c_idx as i8,
        ))
    } else {
        None
    };

    if det.degree() == 2 {
        if let Some(cand) = candidate {
            let a = det.c[2];
            let b = det.c[1];
            let d = 2.0 * a * cand.time + b;
            if a < 0.0 || d <= 0.0 {
                return Ok(cand);
            }
        }
        return split_or_flip_from_det(t, c_idx, det, time_now);
    }

    if let Some(cand) = candidate {
        return Ok(cand);
    }
    split_or_flip_from_det(t, c_idx, det, time_now)
}

/// Leftover 1-const after a same-time collapse: both endpoints sit on one
/// point with the same velocity. C++ `compute_collapse` sees matching
/// `pos_zero` (ALWAYS). Back-projected `pos_zero` can miss that by ULPs,
/// so the edge looks like NEVER and would otherwise become a now-time
/// split on a zero-length constraint.
fn leftover_constraint_already_a_point(
    v0: &WavefrontVertex,
    v1: &WavefrontVertex,
    time_now: f64,
) -> bool {
    if v0.velocity != v1.velocity {
        return false;
    }
    let p0 = v0.position_at(time_now);
    let p1 = v1.position_at(time_now);
    p0.x == p1.x && p0.y == p1.y
}

/// C++ `get_time_vertex_on_supporting_line`.
fn time_vertex_on_supporting_line(v: &WavefrontVertex, e: SupportingLine) -> Option<f64> {
    let nx = -e.direction.y;
    let ny = e.direction.x;
    let pqx = v.pos_zero.x - e.point.x;
    let pqy = v.pos_zero.y - e.point.y;
    let scaled_distance = pqx * nx + pqy * ny;
    let scaled_edge_speed = e.weight * nx.hypot(ny);
    let scaled_vertex_speed = v.velocity.x * nx + v.velocity.y * ny;
    let approach = scaled_edge_speed - scaled_vertex_speed;
    if approach == 0.0 {
        return (scaled_distance == 0.0).then_some(f64::NEG_INFINITY);
    }
    Some(scaled_distance / approach)
}

fn line_hit_is_recent(tc: f64, time_now: f64, slop: f64) -> bool {
    tc.is_finite() && tc <= time_now && time_now - tc <= slop
}

/// Opposite vertex is on the constraint segment and the supporting-line
/// hit was only a few microseconds earlier (lake south-reflex after the
/// long-spoke flip).
fn missed_on_segment_split(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    c_idx: usize,
    time_now: f64,
) -> bool {
    const LINE_HIT_SLOP: f64 = 1e-4;
    let Some(vid) = t.vertices[c_idx] else {
        return false;
    };
    let Some(v) = kt.vertices.get(vid) else {
        return false;
    };
    if !v.is_reflex_or_straight() {
        return false;
    }
    let Some(eid) = t.wavefronts[c_idx] else {
        return false;
    };
    let Some(e) = kt.edges.get(eid) else {
        return false;
    };
    let Some(tc) = time_vertex_on_supporting_line(v, e.supporting_line) else {
        return false;
    };
    // Always-on-line is `NEG_INFINITY` (non-finite). That is infinitely
    // old, not "at most LINE_HIT_SLOP seconds late".
    if !line_hit_is_recent(tc, time_now, LINE_HIT_SLOP) {
        return false;
    }
    let Some(va) = t.vertices[ccw(c_idx)] else {
        return false;
    };
    let Some(vb) = t.vertices[cw(c_idx)] else {
        return false;
    };
    let pos = v.position_at(time_now);
    let posa = kt.vertices[va].position_at(time_now);
    let posb = kt.vertices[vb].position_at(time_now);
    let abx = posb.x - posa.x;
    let aby = posb.y - posa.y;
    let ab2 = abx * abx + aby * aby;
    if !(ab2 > 0.0) {
        return false;
    }
    let tparam = ((pos.x - posa.x) * abx + (pos.y - posa.y) * aby) / ab2;
    (0.0..=1.0).contains(&tparam)
}

fn split_or_flip_from_det(
    t: &KineticTriangle,
    c_idx: usize,
    det: crate::numeric::Poly2,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    let Some(split_time) = det.next_root_from(time_now) else {
        return Ok(CollapseSpec::never(t.component));
    };
    Ok(CollapseSpec::with_edge(
        t.component,
        CollapseType::SplitOrFlipRefine,
        split_time,
        c_idx as i8,
    ))
}

fn signed_area_at(kt: &KineticTriangulation, t: &KineticTriangle, time: f64) -> f64 {
    let [Some(a), Some(b), Some(c)] = t.vertices else {
        return 0.0;
    };
    let pa = kt.vertices[a].position_at(time);
    let pb = kt.vertices[b].position_at(time);
    let pc = kt.vertices[c].position_at(time);
    (pb.x - pa.x) * (pc.y - pa.y) - (pb.y - pa.y) * (pc.x - pa.x)
}

fn constraint_collapse_time(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    side: usize,
    now: f64,
) -> Option<f64> {
    use crate::event::EdgeCollapseType;
    use crate::wavefront::edge::compute_edge_collapse_f64;
    let eid = t.wavefronts[side]?;
    let e = kt.edges.get(eid)?;
    let v0 = kt.vertices.get(e.endpoints[0])?;
    let v1 = kt.vertices.get(e.endpoints[1])?;
    // Absolute formula uses pos_zero + velocity (linear motions).
    let spec = compute_edge_collapse_f64(v0.pos_zero, v0.velocity, v1.pos_zero, v1.velocity, now);
    match spec.type_ {
        EdgeCollapseType::Future | EdgeCollapseType::Always => Some(spec.time.max(now)),
        _ => None,
    }
}

fn compute_flip_event_fast(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    let could_flip = t.vertices.iter().flatten().any(|&vid| {
        kt.vertices
            .get(vid)
            .map(|v| v.is_reflex_or_straight())
            .unwrap_or(false)
    });
    if !could_flip {
        return Ok(CollapseSpec::never(t.component));
    }
    let result = generic_collapse_fast(kt, t, time_now)?;
    if result.type_ == CollapseType::VertexMovesOverSpoke {
        let i = result.relevant_edge as usize;
        let reflex = t.vertices[i]
            .and_then(|vid| kt.vertices.get(vid))
            .map(|v| v.is_reflex_or_straight())
            .unwrap_or(false);
        if !reflex {
            return Ok(CollapseSpec::never(t.component));
        }
    }
    Ok(result)
}

fn generic_collapse_fast(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    use crate::numeric::determinant_poly_f64;

    let mut ps = [[0.0; 2]; 3];
    let mut vs = [[0.0; 2]; 3];
    for i in 0..3 {
        let vid = t.vertices[i].ok_or_else(|| PropagationError::TopologyInvariant {
            detail: "generic collapse missing vertex".into(),
        })?;
        let v = kt
            .vertices
            .get(vid)
            .ok_or_else(|| PropagationError::TopologyInvariant {
                detail: "generic collapse dangling vertex".into(),
            })?;
        // Position polynomials are relative to t=0: pos_zero + t * vel
        ps[i] = [v.pos_zero.x, v.pos_zero.y];
        vs[i] = [v.velocity.x, v.velocity.y];
    }
    let det = determinant_poly_f64(ps[0], vs[0], ps[1], vs[1], ps[2], vs[2]);
    let Some(collapse_time) = det.next_root_from(time_now) else {
        return Ok(CollapseSpec::never(t.component));
    };

    // Classify at collapse time
    classify_at_time(kt, t, collapse_time)
}

fn classify_at_time(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time: f64,
) -> Result<CollapseSpec, PropagationError> {
    let mut pts = [Point2::new(0.0, 0.0); 3];
    for i in 0..3 {
        let vid = t.vertices[i].unwrap();
        pts[i] = kt.vertices[vid].position_at(time);
    }
    let mut zero_edges = 0;
    let mut zero_side = 0;
    let mut lengths = [0.0; 3];
    for s in 0..3 {
        let a = pts[ccw(s)];
        let b = pts[cw(s)];
        let len = a.distance_to(b);
        lengths[s] = len;
    }
    let max_l = lengths[0].max(lengths[1]).max(lengths[2]);
    // Fast kernel: det root is not exact; treat a collapsed spoke as zero
    // when it is tiny vs the other sides (C++ uses exact NT == 0).
    let eps = 1e-9 * (1.0 + max_l);
    for s in 0..3 {
        if lengths[s] <= eps {
            zero_edges += 1;
            zero_side = s;
        }
    }
    if zero_edges == 2 {
        zero_edges = 3;
    }
    match zero_edges {
        3 => Ok(CollapseSpec::new(
            t.component,
            CollapseType::TriangleCollapse,
            time,
        )),
        1 => {
            if t.wavefronts[zero_side].is_some() {
                Ok(CollapseSpec::with_edge(
                    t.component,
                    CollapseType::ConstraintCollapse,
                    time,
                    zero_side as i8,
                ))
            } else {
                Ok(CollapseSpec::with_edge(
                    t.component,
                    CollapseType::SpokeCollapse,
                    time,
                    zero_side as i8,
                ))
            }
        }
        0 => {
            // Longest edge
            let mut best = 0;
            for s in 1..3 {
                if lengths[s] > lengths[best] {
                    best = s;
                }
            }
            Ok(CollapseSpec::with_edge_secondary(
                t.component,
                CollapseType::VertexMovesOverSpoke,
                time,
                best as i8,
                lengths[best] * lengths[best],
            ))
        }
        _ => Ok(CollapseSpec::with_edge(
            t.component,
            CollapseType::SplitOrFlipRefine,
            time,
            zero_side as i8,
        )),
    }
}

fn compute_unbounded_fast(
    kt: &KineticTriangulation,
    t: &KineticTriangle,
    time_now: f64,
) -> Result<CollapseSpec, PropagationError> {
    let Some(inf) = kt.infinite_vertex else {
        return Ok(CollapseSpec::never(t.component));
    };
    let Some(idx) = t.vertices.iter().position(|v| *v == Some(inf)) else {
        return Ok(CollapseSpec::never(t.component));
    };

    let edge_collapse = if t.wavefronts[idx].is_some() {
        match constraint_collapse_time(kt, t, idx, time_now) {
            Some(tc) => CollapseSpec::with_edge(
                t.component,
                CollapseType::ConstraintCollapse,
                tc,
                idx as i8,
            ),
            None => CollapseSpec::never(t.component),
        }
    } else {
        CollapseSpec::never(t.component)
    };

    let Some(nid) = t.neighbors[cw(idx)] else {
        return Ok(edge_collapse);
    };
    let n = match kt.triangles.get(nid) {
        Some(n) if n.live() => n,
        _ => return Ok(edge_collapse),
    };
    let Some(nidx) = n.vertices.iter().position(|v| *v == Some(inf)) else {
        return Ok(edge_collapse);
    };

    let this_c = t.wavefronts[idx].is_some();
    let n_c = n.wavefronts[nidx].is_some();
    if this_c && n_c {
        return Ok(edge_collapse);
    }
    if this_c || n_c {
        let (eid, vid) = if this_c {
            (t.wavefronts[idx], n.vertices[ccw(nidx)])
        } else {
            (n.wavefronts[nidx], t.vertices[cw(idx)])
        };
        let (Some(eid), Some(vid)) = (eid, vid) else {
            return Ok(edge_collapse);
        };
        if Some(vid) == kt.infinite_vertex {
            return Ok(edge_collapse);
        }
        let e = &kt.edges[eid];
        let v = &kt.vertices[vid];
        let leaves = vertex_leaves_ch(v, &e.supporting_line, time_now);
        let vertex_spec = match leaves {
            Some(tc) => {
                CollapseSpec::with_edge(t.component, CollapseType::CcwVertexLeavesCh, tc, idx as i8)
            }
            None => CollapseSpec::never(t.component),
        };
        return Ok(
            if vertex_spec.cmp_priority(&edge_collapse) == std::cmp::Ordering::Less {
                vertex_spec
            } else {
                edge_collapse
            },
        );
    }

    let (Some(u), Some(v), Some(w)) = (
        t.vertices[cw(idx)],
        t.vertices[ccw(idx)],
        n.vertices[ccw(nidx)],
    ) else {
        return Ok(CollapseSpec::never(t.component));
    };
    if [u, v, w].iter().any(|x| kt.infinite_vertex == Some(*x)) {
        return Ok(CollapseSpec::never(t.component));
    }
    let vu = &kt.vertices[u];
    let vv = &kt.vertices[v];
    let vw = &kt.vertices[w];
    if vu.velocity == vv.velocity && vv.velocity == vw.velocity {
        return Ok(CollapseSpec::never(t.component));
    }
    match generic_collapse_time_3(vu, vv, vw, time_now) {
        Some(tc) => Ok(CollapseSpec::with_edge(
            t.component,
            CollapseType::CcwVertexLeavesCh,
            tc,
            idx as i8,
        )),
        None => Ok(CollapseSpec::never(t.component)),
    }
}

fn vertex_leaves_ch(
    v: &WavefrontVertex,
    e: &crate::wavefront::supporting_line::SupportingLine,
    time_now: f64,
) -> Option<f64> {
    let approach = e.weight - (v.velocity.x * e.normal_unit.x + v.velocity.y * e.normal_unit.y);
    if approach > 0.0 {
        return None;
    }
    let dist =
        (v.pos_zero.x - e.point.x) * e.normal_unit.x + (v.pos_zero.y - e.point.y) * e.normal_unit.y;
    if approach == 0.0 {
        return None;
    }
    let tc = dist / approach;
    // `-0.0 >= 0.0` is true; already-on-line is not a future leave (C++ NEVER).
    if tc > time_now {
        Some(tc)
    } else {
        None
    }
}

fn generic_collapse_time_3(
    u: &WavefrontVertex,
    v: &WavefrontVertex,
    w: &WavefrontVertex,
    time_now: f64,
) -> Option<f64> {
    use crate::numeric::determinant_poly_f64;
    let det = determinant_poly_f64(
        [u.pos_zero.x, u.pos_zero.y],
        [u.velocity.x, u.velocity.y],
        [v.pos_zero.x, v.pos_zero.y],
        [v.velocity.x, v.velocity.y],
        [w.pos_zero.x, w.pos_zero.y],
        [w.velocity.x, w.velocity.y],
    );
    if det.degree() == 0 {
        // Always-colinear CH triple: they are not leaving (C++ NEVER).
        return None;
    }
    det.next_root_from(time_now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn south_edge_line_hit_is_6us_before_det_root() {
        // Input 445 vs south constraint 1386–1387. C++ line-hit is
        // 23.173298; the triangle det root is 23.173305.
        let sl = SupportingLine::from_segment(
            Point2::new(539555.978609, 7037179.372629),
            Point2::new(539580.372538, 7037179.372523),
            1.0,
        )
        .unwrap();
        let mut v = WavefrontVertex::new_kinetic(
            Point2::new(539554.352973, 7037225.719239),
            Point2::new(-0.08196974587509578, -0.9999999180436134),
            0.0,
            InfiniteSpeedType::None,
        );
        v.angle = crate::wavefront::vertex::AngleType::Reflex;
        let tc = time_vertex_on_supporting_line(&v, sl).unwrap();
        assert!((tc - 23.173298).abs() < 1e-5, "line-hit {tc}");
        assert!(tc < 23.173305);
        let pos = v.position_at(23.173305306854);
        let a = Point2::new(539552.841229, 7037202.545948);
        let b = Point2::new(539584.573643, 7037202.545810);
        let ab2 = (b.x - a.x).hypot(b.y - a.y).powi(2);
        let tparam = ((pos.x - a.x) * (b.x - a.x) + (pos.y - a.y) * (b.y - a.y)) / ab2;
        assert!(
            tparam < 0.0,
            "first event stays off-segment tparam={tparam}"
        );
    }

    #[test]
    fn always_on_line_is_not_a_recent_hit() {
        let sl = SupportingLine::from_segment(Point2::new(0.0, 0.0), Point2::new(2.0, 0.0), 1.0)
            .unwrap();
        // On the line at t=0 and matching the supporting-line speed, so
        // C++ reports the vertex as always-on-line (`-∞`).
        let v = WavefrontVertex::new_kinetic(
            Point2::new(0.5, 0.0),
            Point2::new(0.0, 1.0),
            0.0,
            InfiniteSpeedType::None,
        );
        let tc = time_vertex_on_supporting_line(&v, sl).unwrap();
        assert!(
            !tc.is_finite() && tc.is_sign_negative(),
            "expected -∞, got {tc}"
        );
        assert!(!line_hit_is_recent(tc, 0.0, 1e-4));
        assert!(!line_hit_is_recent(tc, 10.0, 1e-4));
        assert!(line_hit_is_recent(23.173298, 23.173305, 1e-4));
        assert!(!line_hit_is_recent(23.0, 23.173305, 1e-4));
    }
}
