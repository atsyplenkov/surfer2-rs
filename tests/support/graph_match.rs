//! Order-independent skeleton graph comparison.

use surfer2::{Point2, SkeletonSegment};

#[derive(Debug, Clone)]
pub struct MatchReport {
    pub ok: bool,
    pub detail: String,
    pub segment_count_a: usize,
    pub segment_count_b: usize,
    pub node_count_a: usize,
    pub node_count_b: usize,
}

/// Match two segment multisets by spatially pairing nodes then comparing edges.
pub fn match_segments(
    a: &[SkeletonSegment],
    b: &[(Point2, Point2, f64, f64)],
    tol: f64,
) -> MatchReport {
    let node_a = unique_nodes_seg(a);
    let node_b: Vec<Point2> = {
        let mut v = Vec::new();
        for (s, e, _, _) in b {
            push_unique(&mut v, *s, tol);
            push_unique(&mut v, *e, tol);
        }
        v
    };

    let mut report = MatchReport {
        ok: true,
        detail: String::new(),
        segment_count_a: a.len(),
        segment_count_b: b.len(),
        node_count_a: node_a.len(),
        node_count_b: node_b.len(),
    };

    if a.len() != b.len() {
        report.ok = false;
        report.detail = format!(
            "segment count {} vs {}",
            a.len(),
            b.len()
        );
        return report;
    }
    if node_a.len() != node_b.len() {
        report.ok = false;
        report.detail = format!(
            "node count {} vs {}",
            node_a.len(),
            node_b.len()
        );
        return report;
    }

    // Greedy spatial matching of nodes
    let mut map_b_of_a = vec![None; node_a.len()];
    let mut used = vec![false; node_b.len()];
    for (i, pa) in node_a.iter().enumerate() {
        let mut best = None;
        let mut best_d = f64::INFINITY;
        for (j, pb) in node_b.iter().enumerate() {
            if used[j] {
                continue;
            }
            let d = pa.distance_to(*pb);
            if d < best_d {
                best_d = d;
                best = Some(j);
            }
        }
        if best_d > tol {
            report.ok = false;
            report.detail = format!("node {i} unmatched (dist {best_d} > {tol})");
            return report;
        }
        let j = best.unwrap();
        used[j] = true;
        map_b_of_a[i] = Some(j);
    }

    // Compare edge multisets in matched node ids
    let mut edges_a = multiset_edges_seg(a, &node_a, tol);
    let mut edges_b = {
        let mut v = Vec::new();
        for (s, e, _, _) in b {
            let i = find_node(&node_b, *s, tol).unwrap();
            let j = find_node(&node_b, *e, tol).unwrap();
            let (u, w) = if i <= j { (i, j) } else { (j, i) };
            v.push((u, w));
        }
        v.sort_unstable();
        v
    };
    // Remap a edges through inverse map: a-node -> b-node
    let mut inv = vec![0; node_b.len()];
    for (i, m) in map_b_of_a.iter().enumerate() {
        inv[m.unwrap()] = i;
    }
    // Actually edges_a uses indices in node_a; convert to b indices
    let edges_a_mapped: Vec<(usize, usize)> = edges_a
        .iter()
        .map(|(i, j)| {
            let bi = map_b_of_a[*i].unwrap();
            let bj = map_b_of_a[*j].unwrap();
            if bi <= bj {
                (bi, bj)
            } else {
                (bj, bi)
            }
        })
        .collect();
    let mut edges_a_mapped = edges_a_mapped;
    edges_a_mapped.sort_unstable();
    edges_b.sort_unstable();
    if edges_a_mapped != edges_b {
        report.ok = false;
        report.detail = format!(
            "edge multiset differs ({} vs {} unique pairs after match)",
            edges_a_mapped.len(),
            edges_b.len()
        );
        let _ = edges_a;
        return report;
    }
    report.detail = "ok".into();
    report
}

fn unique_nodes_seg(segs: &[SkeletonSegment]) -> Vec<Point2> {
    let mut v = Vec::new();
    for s in segs {
        push_unique(&mut v, s.start, 0.0);
        push_unique(&mut v, s.end, 0.0);
    }
    v
}

fn push_unique(v: &mut Vec<Point2>, p: Point2, tol: f64) {
    if tol == 0.0 {
        if !v.iter().any(|q| q.x.to_bits() == p.x.to_bits() && q.y.to_bits() == p.y.to_bits()) {
            v.push(p);
        }
    } else if find_node(v, p, tol).is_none() {
        v.push(p);
    }
}

fn find_node(nodes: &[Point2], p: Point2, tol: f64) -> Option<usize> {
    nodes.iter().position(|q| q.distance_to(p) <= tol)
}

fn multiset_edges_seg(segs: &[SkeletonSegment], nodes: &[Point2], tol: f64) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    for s in segs {
        let i = find_node(nodes, s.start, tol.max(0.0)).unwrap_or(0);
        let j = find_node(nodes, s.end, tol.max(0.0)).unwrap_or(0);
        let (u, w) = if i <= j { (i, j) } else { (j, i) };
        v.push((u, w));
    }
    v.sort_unstable();
    v
}

/// Tolerance from plan: max(64 ULP, bbox_diagonal * 1e-10).
pub fn differential_tol(points: &[Point2]) -> f64 {
    if points.is_empty() {
        return 1e-9;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let diag = (max_x - min_x).hypot(max_y - min_y);
    let ulp_scale = points
        .iter()
        .map(|p| ulp(p.x).max(ulp(p.y)))
        .fold(0.0, f64::max)
        * 64.0;
    ulp_scale.max(diag * 1e-10).max(1e-12)
}

fn ulp(x: f64) -> f64 {
    if !x.is_finite() {
        return 1.0;
    }
    let bits = x.to_bits();
    let next = f64::from_bits(bits + 1);
    (next - x).abs()
}
