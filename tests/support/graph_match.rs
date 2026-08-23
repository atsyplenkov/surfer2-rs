//! Order-independent skeleton graph comparison.

use std::collections::HashMap;

use surfer2::{Point2, SkeletonSegment};

#[derive(Debug, Clone)]
pub struct MatchReport {
    pub ok: bool,
    pub detail: String,
    pub segment_count_a: usize,
    pub segment_count_b: usize,
    pub node_count_a: usize,
    pub node_count_b: usize,
    pub edges_only_a: usize,
    pub edges_only_b: usize,
}

#[derive(Debug, Clone)]
pub struct ProximityReport {
    pub n_a: usize,
    pub n_b: usize,
    pub max_a_to_b: f64,
    pub max_b_to_a: f64,
    pub a_within_1mm: usize,
    pub a_within_1cm: usize,
    pub b_within_1mm: usize,
    pub b_within_1cm: usize,
}

/// Bit-exact unique points (set-based; not quadratic scan).
pub fn unique_points_bits(pts: impl IntoIterator<Item = Point2>) -> Vec<Point2> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in pts {
        if seen.insert((p.x.to_bits(), p.y.to_bits())) {
            out.push(p);
        }
    }
    out
}

/// Nearest-neighbor distances from each point in `from` to `to` via a grid.
pub fn nearest_each(from: &[Point2], to: &[Point2]) -> Vec<f64> {
    if to.is_empty() {
        return vec![f64::INFINITY; from.len()];
    }
    let grid = PointGrid::build(to, grid_cell(to));
    from.iter().map(|p| grid.nearest(*p).1).collect()
}

/// Bidirectional node proximity (indexed; does not require equal counts).
pub fn bidirectional_proximity(a: &[Point2], b: &[Point2]) -> ProximityReport {
    let d_ab = nearest_each(a, b);
    let d_ba = nearest_each(b, a);
    ProximityReport {
        n_a: a.len(),
        n_b: b.len(),
        max_a_to_b: d_ab.iter().copied().fold(0.0, f64::max),
        max_b_to_a: d_ba.iter().copied().fold(0.0, f64::max),
        a_within_1mm: d_ab.iter().filter(|d| **d <= 0.001).count(),
        a_within_1cm: d_ab.iter().filter(|d| **d <= 0.01).count(),
        b_within_1mm: d_ba.iter().filter(|d| **d <= 0.001).count(),
        b_within_1cm: d_ba.iter().filter(|d| **d <= 0.01).count(),
    }
}

/// Match two segment graphs: tolerant node correspondence both ways, then
/// undirected edge multisets in both directions.
pub fn match_graphs(a: &[(Point2, Point2)], b: &[(Point2, Point2)], tol: f64) -> MatchReport {
    let node_a = unique_points_bits(a.iter().flat_map(|(s, e)| [*s, *e]));
    let node_b = unique_points_bits(b.iter().flat_map(|(s, e)| [*s, *e]));
    match_graphs_with_nodes(a, &node_a, b, &node_b, tol)
}

/// Like [`match_graphs`], but uses caller-supplied node identities
/// (DCEL nodes vs bit-unique OBJ endpoints) instead of bit-unique
/// segment endpoints.
pub fn match_graphs_with_nodes(
    a: &[(Point2, Point2)],
    node_a: &[Point2],
    b: &[(Point2, Point2)],
    node_b: &[Point2],
    tol: f64,
) -> MatchReport {
    let id = identity_map(node_a.len().max(node_b.len()));
    let Some(edges_a) = undirected_edges(a, node_a, &id, tol) else {
        let mut report = blank_report(a.len(), b.len(), node_a.len(), node_b.len());
        report.ok = false;
        report.detail = "A endpoint outside node tolerance".into();
        return report;
    };
    let Some(edges_b) = undirected_edges(b, node_b, &id, tol) else {
        let mut report = blank_report(a.len(), b.len(), node_a.len(), node_b.len());
        report.ok = false;
        report.detail = "B endpoint outside node tolerance".into();
        return report;
    };
    match_indexed_graphs(node_a, &edges_a, node_b, &edges_b, tol)
}

/// Compare two already-indexed undirected graphs after pairing `node_a` to `node_b`.
pub fn match_indexed_graphs(
    node_a: &[Point2],
    edges_a: &[(usize, usize)],
    node_b: &[Point2],
    edges_b: &[(usize, usize)],
    tol: f64,
) -> MatchReport {
    let mut report = blank_report(edges_a.len(), edges_b.len(), node_a.len(), node_b.len());
    if edges_a.len() != edges_b.len() {
        report.ok = false;
        report.detail = format!("segment count {} vs {}", edges_a.len(), edges_b.len());
        return report;
    }
    if node_a.len() != node_b.len() {
        report.ok = false;
        report.detail = format!("node count {} vs {}", node_a.len(), node_b.len());
        return report;
    }
    match correspond_for_edges(node_a, node_b, edges_a, edges_b, tol) {
        Ok(map) => {
            let a_mapped = remap_edges(edges_a, &map);
            let mut b_id = edges_b.to_vec();
            normalize_edges(&mut b_id);
            if let Some(detail) = apply_edge_delta(&mut report, "A→B", &a_mapped, &b_id) {
                report.ok = false;
                report.detail = detail;
                return report;
            }
            report.detail = "ok".into();
            report
        }
        Err(detail) => {
            report.ok = false;
            report.detail = detail.into();
            report
        }
    }
}

fn remap_edges(edges: &[(usize, usize)], map: &[usize]) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = edges
        .iter()
        .map(|&(i, j)| {
            let bi = map[i];
            let bj = map[j];
            if bi <= bj {
                (bi, bj)
            } else {
                (bj, bi)
            }
        })
        .collect();
    v.sort_unstable();
    v
}

fn normalize_edges(edges: &mut [(usize, usize)]) {
    for e in edges.iter_mut() {
        if e.0 > e.1 {
            *e = (e.1, e.0);
        }
    }
    edges.sort_unstable();
}

fn blank_report(
    segment_count_a: usize,
    segment_count_b: usize,
    node_count_a: usize,
    node_count_b: usize,
) -> MatchReport {
    MatchReport {
        ok: true,
        detail: String::new(),
        segment_count_a,
        segment_count_b,
        node_count_a,
        node_count_b,
        edges_only_a: 0,
        edges_only_b: 0,
    }
}

fn apply_edge_delta(
    report: &mut MatchReport,
    tag: &str,
    a: &[(usize, usize)],
    b: &[(usize, usize)],
) -> Option<String> {
    if a == b {
        return None;
    }
    let mut i = 0;
    let mut j = 0;
    let mut only_a = 0usize;
    let mut only_b = 0usize;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => {
                only_a += 1;
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                only_b += 1;
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
        }
    }
    only_a += a.len() - i;
    only_b += b.len() - j;
    report.edges_only_a = only_a;
    report.edges_only_b = only_b;
    Some(format!(
        "{tag} edge multiset differs (only_a={only_a} only_b={only_b})"
    ))
}

/// Match two segment multisets by spatially pairing nodes then comparing edges.
pub fn match_segments(
    a: &[SkeletonSegment],
    b: &[(Point2, Point2, f64, f64)],
    tol: f64,
) -> MatchReport {
    let aa: Vec<(Point2, Point2)> = a.iter().map(|s| (s.start, s.end)).collect();
    let bb: Vec<(Point2, Point2)> = b.iter().map(|(s, e, _, _)| (*s, *e)).collect();
    match_graphs(&aa, &bb, tol)
}

/// Distance from `p` to the interior of `a--b`. `None` if the nearest
/// point on the line is an endpoint.
pub fn interior_segment_distance(a: Point2, b: Point2, p: Point2) -> Option<f64> {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let ab2 = abx * abx + aby * aby;
    if !(ab2 > 0.0) {
        return None;
    }
    let t = ((p.x - a.x) * abx + (p.y - a.y) * aby) / ab2;
    if t <= 0.0 || t >= 1.0 {
        return None;
    }
    let q = Point2::new(a.x + t * abx, a.y + t * aby);
    Some(p.distance_to(q))
}

/// Locate segment endpoints onto `nodes` and return undirected index pairs.
/// Fails if any endpoint is farther than `tol` from every node.
pub fn edges_on_nodes(
    segs: &[(Point2, Point2)],
    nodes: &[Point2],
    tol: f64,
) -> Result<Vec<(usize, usize)>, String> {
    undirected_edges(segs, nodes, &identity_map(nodes.len()), tol)
        .ok_or_else(|| "endpoint outside node tolerance".into())
}

fn identity_map(n: usize) -> Vec<usize> {
    (0..n).collect()
}

const EXHAUSTIVE_CAP: usize = 8;

/// Tolerance-respecting bijection. Only degree-1 candidate-graph pairs are
/// locked. Leftover nodes are solved per connected component; topology
/// search may reassign any non-forced pair.
fn correspond_for_edges(
    a: &[Point2],
    b: &[Point2],
    edges_a: &[(usize, usize)],
    edges_b: &[(usize, usize)],
    tol: f64,
) -> Result<Vec<usize>, &'static str> {
    if a.len() != b.len() {
        return Err("node correspondence failed");
    }
    let n = a.len();
    let adj = candidate_adj(a, b, tol).ok_or("node correspondence failed")?;
    let mut map = vec![None; n];
    let mut used = vec![false; n];
    lock_forced_pairs(&adj, &mut map, &mut used);

    let spatial = leftover_components(&adj, &map, &used);
    for comp in couple_by_topology(spatial, &map, &used, &adj, edges_a, edges_b) {
        if comp.is_empty() {
            continue;
        }
        if comp.len() <= EXHAUSTIVE_CAP {
            if !search_preserving(&mut map, &mut used, &comp, 0, &adj, edges_a, edges_b)
                && assign_geometric_component(&mut map, &mut used, &comp, &adj).is_none()
            {
                return Err("node correspondence failed");
            }
        } else if assign_geometric_component(&mut map, &mut used, &comp, &adj).is_none() {
            return Err("node correspondence failed");
        } else if !partial_edges_agree(&map, edges_a, edges_b) {
            // Too large to search jointly; do not emit a fake edge delta.
            return Err("ambiguous node correspondence");
        }
    }
    finish_map(map)
}

fn finish_map(map: Vec<Option<usize>>) -> Result<Vec<usize>, &'static str> {
    map.into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("node correspondence failed")
}

fn candidate_adj(a: &[Point2], b: &[Point2], tol: f64) -> Option<Vec<Vec<usize>>> {
    let grid = PointGrid::build(b, (tol * 2.0).max(1e-12));
    let mut adj = vec![Vec::new(); a.len()];
    for (i, p) in a.iter().enumerate() {
        let mut cand = Vec::new();
        for j in grid.neighbors(*p, tol) {
            let d = p.distance_to(b[j]);
            if d <= tol {
                cand.push((d, j));
            }
        }
        if cand.is_empty() {
            return None;
        }
        cand.sort_by(|x, y| {
            x.0.partial_cmp(&y.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(x.1.cmp(&y.1))
        });
        adj[i] = cand.into_iter().map(|(_, j)| j).collect();
    }
    Some(adj)
}

fn lock_forced_pairs(adj: &[Vec<usize>], map: &mut [Option<usize>], used: &mut [bool]) {
    let n = adj.len();
    loop {
        let mut changed = false;
        for (i, cand) in adj.iter().enumerate() {
            if map[i].is_some() {
                continue;
            }
            let avail: Vec<usize> = cand.iter().copied().filter(|&j| !used[j]).collect();
            if avail.len() == 1 {
                map[i] = Some(avail[0]);
                used[avail[0]] = true;
                changed = true;
            }
        }
        let mut owners = vec![Vec::new(); n];
        for (i, cand) in adj.iter().enumerate() {
            if map[i].is_some() {
                continue;
            }
            for &j in cand {
                if !used[j] {
                    owners[j].push(i);
                }
            }
        }
        for (j, from) in owners.iter().enumerate() {
            if used[j] || from.len() != 1 {
                continue;
            }
            let i = from[0];
            if map[i].is_some() {
                continue;
            }
            map[i] = Some(j);
            used[j] = true;
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

fn leftover_components(
    adj: &[Vec<usize>],
    map: &[Option<usize>],
    used: &[bool],
) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut rev = vec![Vec::new(); n];
    for (i, cand) in adj.iter().enumerate() {
        if map[i].is_some() {
            continue;
        }
        for &j in cand {
            if !used[j] {
                rev[j].push(i);
            }
        }
    }
    let mut seen = vec![false; n];
    let mut comps = Vec::new();
    for start in 0..n {
        if map[start].is_some() || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut comp = Vec::new();
        while let Some(i) = stack.pop() {
            if seen[i] {
                continue;
            }
            seen[i] = true;
            comp.push(i);
            for &j in &adj[i] {
                if used[j] {
                    continue;
                }
                for &k in &rev[j] {
                    if map[k].is_none() && !seen[k] {
                        stack.push(k);
                    }
                }
            }
        }
        comps.push(comp);
    }
    comps
}

/// Skeleton edges can join leftover nodes that sit in different spatial
/// candidate components. Those groups must be assigned together.
fn couple_by_topology(
    comps: Vec<Vec<usize>>,
    map: &[Option<usize>],
    used: &[bool],
    adj: &[Vec<usize>],
    edges_a: &[(usize, usize)],
    edges_b: &[(usize, usize)],
) -> Vec<Vec<usize>> {
    if comps.len() <= 1 {
        return comps;
    }
    let n = map.len();
    let mut comp_of = vec![None; n];
    for (ci, comp) in comps.iter().enumerate() {
        for &i in comp {
            comp_of[i] = Some(ci);
        }
    }
    let mut b_comp = vec![None; n];
    for (i, cand) in adj.iter().enumerate() {
        let Some(ci) = comp_of[i] else {
            continue;
        };
        for &j in cand {
            if !used[j] {
                b_comp[j] = Some(ci);
            }
        }
    }
    let mut parent: Vec<usize> = (0..comps.len()).collect();
    for &(u, v) in edges_a {
        if let (Some(cu), Some(cv)) = (comp_of[u], comp_of[v]) {
            uf_union(&mut parent, cu, cv);
        }
    }
    for &(p, q) in edges_b {
        if let (Some(cp), Some(cq)) = (b_comp[p], b_comp[q]) {
            uf_union(&mut parent, cp, cq);
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (ci, comp) in comps.into_iter().enumerate() {
        groups
            .entry(uf_find(&mut parent, ci))
            .or_default()
            .extend(comp);
    }
    groups.into_values().collect()
}

fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn uf_union(parent: &mut [usize], x: usize, y: usize) {
    let (px, py) = (uf_find(parent, x), uf_find(parent, y));
    if px != py {
        parent[px] = py;
    }
}

fn assign_geometric_component(
    map: &mut [Option<usize>],
    used: &mut [bool],
    comp: &[usize],
    adj: &[Vec<usize>],
) -> Option<()> {
    let geo = complete_geometric(map, adj)?;
    for &i in comp {
        let j = geo[i];
        if used[j] && map[i] != Some(j) {
            return None;
        }
        map[i] = Some(j);
        used[j] = true;
    }
    Some(())
}

fn partial_edges_agree(
    map: &[Option<usize>],
    edges_a: &[(usize, usize)],
    edges_b: &[(usize, usize)],
) -> bool {
    let mut a_e = Vec::new();
    for &(i, j) in edges_a {
        if let (Some(bi), Some(bj)) = (map[i], map[j]) {
            a_e.push(if bi <= bj { (bi, bj) } else { (bj, bi) });
        }
    }
    a_e.sort_unstable();
    let assigned_b: std::collections::HashSet<usize> = map.iter().filter_map(|m| *m).collect();
    let mut b_e = Vec::new();
    for &(i, j) in edges_b {
        if assigned_b.contains(&i) && assigned_b.contains(&j) {
            b_e.push(if i <= j { (i, j) } else { (j, i) });
        }
    }
    b_e.sort_unstable();
    a_e == b_e
}

fn search_preserving(
    map: &mut [Option<usize>],
    used: &mut [bool],
    leftover: &[usize],
    k: usize,
    adj: &[Vec<usize>],
    edges_a: &[(usize, usize)],
    edges_b: &[(usize, usize)],
) -> bool {
    if k == leftover.len() {
        return partial_edges_agree(map, edges_a, edges_b);
    }
    let i = leftover[k];
    for &j in &adj[i] {
        if used[j] {
            continue;
        }
        map[i] = Some(j);
        used[j] = true;
        if search_preserving(map, used, leftover, k + 1, adj, edges_a, edges_b) {
            return true;
        }
        used[j] = false;
        map[i] = None;
    }
    false
}

fn complete_geometric(partial: &[Option<usize>], adj: &[Vec<usize>]) -> Option<Vec<usize>> {
    let n = partial.len();
    let mut pair_to = vec![None; n];
    for (i, m) in partial.iter().enumerate() {
        if let Some(j) = *m {
            pair_to[j] = Some(i);
        }
    }
    for i in 0..n {
        if partial[i].is_some() {
            continue;
        }
        let mut seen = vec![false; n];
        if !augment_free(i, adj, &mut pair_to, &mut seen, partial) {
            return None;
        }
    }
    let mut map = vec![0; n];
    for (j, oi) in pair_to.iter().enumerate() {
        map[(*oi)?] = j;
    }
    Some(map)
}

fn augment_free(
    i: usize,
    adj: &[Vec<usize>],
    pair_to: &mut [Option<usize>],
    seen: &mut [bool],
    locked: &[Option<usize>],
) -> bool {
    if locked[i].is_some() {
        return false;
    }
    for &j in &adj[i] {
        if seen[j] {
            continue;
        }
        seen[j] = true;
        let prev = pair_to[j];
        let can_take = match prev {
            None => true,
            Some(k) if locked[k].is_none() => augment_free(k, adj, pair_to, seen, locked),
            Some(_) => false,
        };
        if can_take {
            pair_to[j] = Some(i);
            return true;
        }
    }
    false
}

fn undirected_edges(
    segs: &[(Point2, Point2)],
    nodes: &[Point2],
    map: &[usize],
    tol: f64,
) -> Option<Vec<(usize, usize)>> {
    let grid = PointGrid::build(nodes, (tol * 2.0).max(1e-12));
    let mut v = Vec::with_capacity(segs.len());
    for (s, e) in segs {
        let i = locate(&grid, *s, tol)?;
        let j = locate(&grid, *e, tol)?;
        let bi = map[i];
        let bj = map[j];
        v.push(if bi <= bj { (bi, bj) } else { (bj, bi) });
    }
    v.sort_unstable();
    Some(v)
}

fn locate(grid: &PointGrid<'_>, p: Point2, tol: f64) -> Option<usize> {
    let (i, d) = grid.nearest(p);
    if d <= tol.max(0.0) {
        Some(i)
    } else {
        None
    }
}

fn grid_cell(pts: &[Point2]) -> f64 {
    if pts.len() < 2 {
        return 1.0;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for p in pts {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let diag = (max_x - min_x).hypot(max_y - min_y);
    (diag / (pts.len() as f64).sqrt()).max(1e-6)
}

struct PointGrid<'a> {
    pts: &'a [Point2],
    cell: f64,
    buckets: HashMap<(i64, i64), Vec<usize>>,
}

impl<'a> PointGrid<'a> {
    fn build(pts: &'a [Point2], cell: f64) -> Self {
        let cell = cell.max(1e-15);
        let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in pts.iter().enumerate() {
            buckets.entry(key(p, cell)).or_default().push(i);
        }
        Self { pts, cell, buckets }
    }

    fn key_of(&self, p: Point2) -> (i64, i64) {
        key(&p, self.cell)
    }

    fn nearest(&self, p: Point2) -> (usize, f64) {
        self.nearest_filtered(p, |_| true)
    }

    fn nearest_filtered(&self, p: Point2, ok: impl Fn(usize) -> bool) -> (usize, f64) {
        if self.pts.is_empty() {
            return (0, f64::INFINITY);
        }
        let (gx, gy) = self.key_of(p);
        let mut best = 0;
        let mut best_d = f64::INFINITY;
        let mut r = 0i64;
        loop {
            for dx in -r..=r {
                for dy in -r..=r {
                    if r > 0 && dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let Some(ids) = self.buckets.get(&(gx + dx, gy + dy)) else {
                        continue;
                    };
                    for &i in ids {
                        if !ok(i) {
                            continue;
                        }
                        let d = p.distance_to(self.pts[i]);
                        if d < best_d {
                            best_d = d;
                            best = i;
                        }
                    }
                }
            }
            let covered = (r as f64) * self.cell;
            if best_d.is_finite() && best_d <= covered {
                return (best, best_d);
            }
            r += 1;
            let cells = (2 * r + 1) * (2 * r + 1);
            if cells as usize >= self.pts.len() || r > 1_000_000 {
                return self.nearest_linear(p, ok);
            }
        }
    }

    fn nearest_linear(&self, p: Point2, ok: impl Fn(usize) -> bool) -> (usize, f64) {
        let mut best = 0;
        let mut best_d = f64::INFINITY;
        for (i, q) in self.pts.iter().enumerate() {
            if !ok(i) {
                continue;
            }
            let d = p.distance_to(*q);
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        (best, best_d)
    }

    fn neighbors(&self, p: Point2, radius: f64) -> Vec<usize> {
        let (gx, gy) = self.key_of(p);
        let r = ((radius / self.cell).ceil() as i64).max(1);
        let mut out = Vec::new();
        for dx in -r..=r {
            for dy in -r..=r {
                if let Some(ids) = self.buckets.get(&(gx + dx, gy + dy)) {
                    for &i in ids {
                        if p.distance_to(self.pts[i]) <= radius {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out
    }
}

fn key(p: &Point2, cell: f64) -> (i64, i64) {
    ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64)
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
