//! Magadan lake GIS acceptance (Milestone 7 gates).

mod support;

use std::path::{Path, PathBuf};

use support::graph_match::{
    bidirectional_proximity, edges_on_nodes, match_graphs, match_indexed_graphs, unique_points_bits,
};
use support::oracle::ObjSkeleton;
use surfer2::{interior_straight_skeleton, NodeId, Point2, Polygon, Skeleton};

fn skeleton_graph(poly: &Polygon) -> Skeleton {
    // Validated public API only — no IncompleteFaceCycle → raw-emit fallback.
    interior_straight_skeleton(poly).expect("skeleton")
}

fn gis_root() -> PathBuf {
    std::env::var_os("SURFER2_GIS_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/gis"))
}

fn load_geojson_polygon(path: &Path) -> Result<Polygon, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let feat = v
        .get("features")
        .and_then(|f| f.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "no features".to_string())?;
    let geom = feat
        .get("geometry")
        .ok_or_else(|| "no geometry".to_string())?;
    let gtype = geom.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let coords = geom
        .get("coordinates")
        .ok_or_else(|| "no coordinates".to_string())?;

    let rings_raw: Vec<Vec<[f64; 2]>> = match gtype {
        "Polygon" => {
            let rings = coords.as_array().ok_or("poly coords")?;
            rings.iter().map(parse_ring).collect::<Result<_, _>>()?
        }
        "MultiPolygon" => {
            let polys = coords.as_array().ok_or("mpoly")?;
            let mut out = Vec::new();
            for poly in polys {
                let rings = poly.as_array().ok_or("poly in mpoly")?;
                for ring in rings {
                    out.push(parse_ring(ring)?);
                }
            }
            out
        }
        other => return Err(format!("unsupported geometry {other}")),
    };

    if rings_raw.is_empty() {
        return Err("empty rings".into());
    }
    let mut rings_pts: Vec<Vec<Point2>> = rings_raw
        .into_iter()
        .map(|r| r.into_iter().map(Point2::from_array).collect())
        .collect();
    let exterior = rings_pts.remove(0);
    Polygon::new(exterior, rings_pts).map_err(|e| e.to_string())
}

fn parse_ring(v: &serde_json::Value) -> Result<Vec<[f64; 2]>, String> {
    let arr = v.as_array().ok_or("ring not array")?;
    let mut pts = Vec::with_capacity(arr.len());
    for p in arr {
        let xy = p.as_array().ok_or("pt")?;
        if xy.len() < 2 {
            return Err("pt short".into());
        }
        let x = xy[0].as_f64().ok_or("x")?;
        let y = xy[1].as_f64().ok_or("y")?;
        pts.push([x, y]);
    }
    Ok(pts)
}

fn cyclomatic(sk: &Skeleton, holes: usize) -> i64 {
    let e = sk.segment_count() as i64;
    let v = sk.node_count() as i64;
    e - v + 1 - holes as i64
}

/// DCEL node IDs for edge identity; incident finite-segment endpoints for
/// position (C++ OBJ vertices are those endpoints). `Node.point` can be a
/// different UF representative and is not the overlay coordinate.
fn rust_graph(sk: &Skeleton) -> (Vec<Point2>, Vec<(usize, usize)>) {
    use std::collections::HashMap;
    let mut index = HashMap::new();
    let mut pts = Vec::new();
    let mut edges = Vec::new();
    for s in sk.finite_segments() {
        let i = rust_node_ix(&mut index, &mut pts, s.start_node(), s.start);
        let j = rust_node_ix(&mut index, &mut pts, s.end_node(), s.end);
        edges.push(if i <= j { (i, j) } else { (j, i) });
    }
    (pts, edges)
}

fn rust_node_ix(
    index: &mut std::collections::HashMap<u32, usize>,
    pts: &mut Vec<Point2>,
    id: NodeId,
    endpoint: Point2,
) -> usize {
    *index.entry(id.0).or_insert_with(|| {
        let k = pts.len();
        pts.push(endpoint);
        k
    })
}

fn obj_ends(obj: &ObjSkeleton) -> (Vec<(Point2, Point2)>, Vec<Point2>, f64) {
    let segs = obj.finite_nonzero_segments();
    let len: f64 = segs.iter().map(|s| s.start.distance_to(s.end)).sum();
    let nodes = unique_points_bits(segs.iter().flat_map(|s| [s.start, s.end]));
    let ends = segs.iter().map(|s| (s.start, s.end)).collect();
    (ends, nodes, len)
}

#[test]
fn unit_square_interior_skeleton_smoke() {
    // Provenance: documented internal geometric property of a 2×2 axis-aligned
    // square — offset(0.5) is the closed [0.5,1.5]² inset. No C++ offset
    // oracle is vendored for this geometry yet (see fixtures/gis/README.txt
    // “Producing C++ offset oracles”); broader C++ conformance is
    // blocked-on-oracle-generation.
    let poly = Polygon::from_flat(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], &[0]).unwrap();
    let sk = interior_straight_skeleton(&poly).unwrap();
    assert_eq!(sk.segment_count(), 4);
    assert_eq!(sk.node_count(), 5);
    assert_eq!(cyclomatic(&sk, 0), 0);
    let offs = sk.offset(0.5).unwrap();
    let contour = offs
        .iter()
        .find(|p| p.closed && p.points.len() >= 4)
        .expect("unit square offset(0.5) should be a closed inset");
    for q in &contour.points {
        assert!(q.is_finite());
        assert!(
            q.x >= 0.5 - 1e-9 && q.x <= 1.5 + 1e-9 && q.y >= 0.5 - 1e-9 && q.y <= 1.5 + 1e-9,
            "offset point {q:?} is not on the 0.5-inset of the 2×2 square"
        );
    }
}

#[test]
fn offset_rejects_bad_time_and_does_not_panic() {
    // Provenance: documented public API contract — negative / non-finite times
    // reject; finite t ≥ 0 returns finite polylines tagged with that time. Not
    // derived from a C++ offset artifact (none vendored; see
    // fixtures/gis/README.txt “Producing C++ offset oracles”).
    let poly = Polygon::from_flat(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], &[0]).unwrap();
    let sk = interior_straight_skeleton(&poly).unwrap();
    assert!(sk.offset(-1.0).is_err());
    assert!(sk.offset(f64::NAN).is_err());
    assert!(sk.offset(f64::INFINITY).is_err());
    for t in [0.0, 0.25, 0.5, 1.0, 2.0] {
        let offs = sk.offset(t).expect("offset");
        for p in &offs {
            assert!(p.time == t);
            assert!(p.points.len() >= 2);
            assert!(p.points.iter().all(|q| q.is_finite()));
        }
    }
}

#[test]
fn rectangle_with_hole_skeleton_and_offset_smoke() {
    // Provenance: documented internal geometric property — at t=0.4 wavefronts
    // have not met, so offset yields a closed outer 0.4-inset of the 6×6 and a
    // closed 0.4-outset of the [2,4]² hole. No C++ offset oracle is vendored
    // for this geometry yet (see fixtures/gis/README.txt “Producing C++ offset
    // oracles”); broader C++ conformance is blocked-on-oracle-generation.
    let poly = Polygon::new(
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(6.0, 0.0),
            Point2::new(6.0, 6.0),
            Point2::new(0.0, 6.0),
        ],
        vec![vec![
            Point2::new(2.0, 2.0),
            Point2::new(2.0, 4.0),
            Point2::new(4.0, 4.0),
            Point2::new(4.0, 2.0),
        ]],
    )
    .unwrap();
    let sk = interior_straight_skeleton(&poly).unwrap();
    assert!(sk.segment_count() > 4);
    assert_dcel_contract(&sk);
    let offs = sk.offset(0.4).unwrap();
    let closed: Vec<_> = offs
        .iter()
        .filter(|p| p.closed && p.points.len() >= 4)
        .collect();
    assert!(
        !closed.is_empty(),
        "rectangle-with-hole offset(0.4) should yield a closed contour"
    );
    // Wavefronts have not met: outer inset of the 6×6 and hole outset of [2,4]².
    assert!(
        closed.len() >= 2,
        "expected outer inset and hole outset, got {}",
        closed.len()
    );
    let mut saw_outer = false;
    let mut saw_hole = false;
    for p in &closed {
        for q in &p.points {
            assert!(q.is_finite());
            assert!(q.x >= 0.0 && q.x <= 6.0 && q.y >= 0.0 && q.y <= 6.0);
        }
        let max_c = p
            .points
            .iter()
            .map(|q| q.x.max(q.y))
            .fold(f64::NEG_INFINITY, f64::max);
        let min_c = p
            .points
            .iter()
            .map(|q| q.x.min(q.y))
            .fold(f64::INFINITY, f64::min);
        if (max_c - 5.6).abs() < 1e-6 && (min_c - 0.4).abs() < 1e-6 {
            saw_outer = true;
        }
        if (max_c - 4.4).abs() < 1e-6 && (min_c - 1.6).abs() < 1e-6 {
            saw_hole = true;
        }
    }
    assert!(saw_outer, "missing 0.4-inset of the outer 6×6");
    assert!(saw_hole, "missing 0.4-outset of the 2–4 hole");
}

fn assert_dcel_contract(sk: &Skeleton) {
    let n = sk.halfedges().len();
    assert!(n >= 2, "empty DCEL");
    assert_eq!(n % 2, 0, "halfedges come in twin pairs");
    let unbounded = sk.faces().len().saturating_sub(1);
    for (i, he) in sk.halfedges().iter().enumerate() {
        let twin = he.twin.0 as usize;
        assert!(twin < n, "twin out of range at {i}");
        assert_eq!(
            sk.halfedges()[twin].twin.0 as usize,
            i,
            "twin reciprocity {i}"
        );
        let next = he.next.0 as usize;
        let prev = he.prev.0 as usize;
        assert!(next < n && prev < n, "next/prev out of range at {i}");
        assert!((he.face.0 as usize) < sk.faces().len(), "face id {i}");
        assert!((he.origin.0 as usize) < sk.nodes().len(), "origin id {i}");
        if he.face.0 as usize == unbounded {
            continue;
        }
        assert_ne!(next, i, "bounded self-loop {i}");
        assert_eq!(sk.halfedges()[next].prev.0 as usize, i, "next.prev {i}");
        assert_eq!(sk.halfedges()[prev].next.0 as usize, i, "prev.next {i}");
        assert_eq!(sk.halfedges()[next].face.0, he.face.0, "next face {i}");
    }
    let mut claimed = vec![false; n];
    for (fi, face) in sk.faces().iter().enumerate() {
        if fi == unbounded {
            continue;
        }
        let Some(start) = face.halfedge else { continue };
        let mut he = start.0 as usize;
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard <= n + 2, "face {fi} cycle");
            assert_eq!(sk.halfedges()[he].face.0 as usize, fi, "face {fi} at {he}");
            assert!(!claimed[he], "bounded halfedge {he} reached twice");
            claimed[he] = true;
            he = sk.halfedges()[he].next.0 as usize;
            if he == start.0 as usize {
                break;
            }
        }
    }
    for (i, he) in sk.halfedges().iter().enumerate() {
        if he.face.0 as usize == unbounded {
            continue;
        }
        assert!(claimed[i], "bounded halfedge {i} not reachable");
    }
    for s in sk.finite_segments() {
        assert!(s.start_time <= s.end_time);
        assert!(s.start.is_finite() && s.end.is_finite());
    }
}

#[test]
fn dcel_contract_unit_square_and_hole() {
    let sq = Polygon::from_flat(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], &[0]).unwrap();
    assert_dcel_contract(&interior_straight_skeleton(&sq).unwrap());
    let hole = Polygon::new(
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(5.0, 0.0),
            Point2::new(5.0, 5.0),
            Point2::new(0.0, 5.0),
        ],
        vec![vec![
            Point2::new(2.0, 2.0),
            Point2::new(2.0, 3.0),
            Point2::new(3.0, 3.0),
            Point2::new(3.0, 2.0),
        ]],
    )
    .unwrap();
    assert_dcel_contract(&interior_straight_skeleton(&hole).unwrap());
}

#[test]
fn public_api_surface_smoke() {
    let poly = Polygon::from_flat(
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
        &[0],
    )
    .unwrap();
    assert_eq!(poly.vertices().len(), 4);
    let sk = interior_straight_skeleton(&poly).unwrap();
    assert_eq!(sk.finite_segments().count(), 4);
}

#[test]
fn graph_matcher_permuted_triangle() {
    let p = |x, y| Point2::new(x, y);
    let a = [
        (p(0.0, 0.0), p(1.0, 0.0)),
        (p(1.0, 0.0), p(0.0, 1.0)),
        (p(0.0, 1.0), p(0.0, 0.0)),
    ];
    let b = [
        (p(0.0, 1.0), p(0.0, 0.0)),
        (p(0.0, 0.0), p(1.0, 0.0)),
        (p(1.0, 0.0), p(0.0, 1.0)),
    ];
    let r = match_graphs(&a, &b, 1e-9);
    assert!(r.ok, "{}", r.detail);
}

#[test]
fn graph_matcher_rejects_missing_edge_and_one_sided_proximity() {
    let p = |x, y| Point2::new(x, y);
    let a = [(p(0.0, 0.0), p(1.0, 0.0)), (p(1.0, 0.0), p(0.0, 1.0))];
    let b = [(p(0.0, 0.0), p(1.0, 0.0))];
    let r = match_graphs(&a, &b, 1e-9);
    assert!(!r.ok);
    assert!(r.detail.contains("segment count"));

    let prox = bidirectional_proximity(&[p(0.0, 0.0), p(10.0, 0.0)], &[p(0.0, 0.0)]);
    assert!(prox.max_a_to_b > 9.0);
    assert!(prox.max_b_to_a < 1e-12);
}

#[test]
fn graph_matcher_finds_nongreedy_bijection() {
    // Greedy shortest-pair takes 0.11→0.1 and leaves 0 unmatched.
    // Augmenting paths pair 0→0.1 and 0.11→0.2.
    let a_nodes = [Point2::new(0.0, 0.0), Point2::new(0.11, 0.0)];
    let b_nodes = [Point2::new(0.1, 0.0), Point2::new(0.2, 0.0)];
    let edges = [(0, 1)];
    let r = match_indexed_graphs(&a_nodes, &edges, &b_nodes, &edges, 0.11);
    assert!(r.ok, "{}", r.detail);
}

#[test]
fn graph_matcher_rejects_ambiguous_unmatchable_and_rewired() {
    let a_nodes = [Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)];
    let b_nodes = [Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)];
    let edges = [(0, 1)];
    let r = match_indexed_graphs(&a_nodes, &edges, &b_nodes, &edges, 0.11);
    assert!(!r.ok);
    assert!(r.detail.contains("correspondence"), "{}", r.detail);

    // Same four nodes and four edges; last ridge is rewired.
    let nodes = [
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ];
    let square = [(0, 1), (1, 2), (2, 3), (3, 0)];
    let rewired = [(0, 1), (1, 2), (2, 3), (0, 2)];
    let r = match_indexed_graphs(&nodes, &square, &nodes, &rewired, 1e-9);
    assert!(!r.ok);
    assert_eq!(r.edges_only_a, 1);
    assert_eq!(r.edges_only_b, 1);

    let segs = [(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))];
    let near = [Point2::new(0.0, 0.0), Point2::new(0.0, 1.0)];
    assert!(edges_on_nodes(&segs, &near, 0.1).is_err());
}

#[test]
fn graph_matcher_accepts_ambiguous_star_self() {
    let o = Point2::new(0.0, 0.0);
    let nodes = [o, o, o];
    let star = [(0, 1), (0, 2)];
    let r = match_indexed_graphs(&nodes, &star, &nodes, &star, 0.1);
    assert!(r.ok, "{}", r.detail);
}

#[test]
fn graph_matcher_relabels_equivalent_stars() {
    let nodes = [
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(2.0, 0.0),
    ];
    let star0 = [(0, 1), (0, 2)];
    let star1 = [(1, 0), (1, 2)];
    let r = match_indexed_graphs(&nodes, &star0, &nodes, &star1, 1.1);
    assert!(r.ok, "{}", r.detail);
}

#[test]
fn graph_matcher_couples_spatial_components_linked_by_edges() {
    // Two spatial pairs (tol 0.15). Identity on the first pair makes the
    // three cross edges disagree; swapping only that pair matches.
    let nodes = [
        Point2::new(0.0, 0.0),
        Point2::new(0.1, 0.0),
        Point2::new(100.0, 0.0),
        Point2::new(100.1, 0.0),
    ];
    let a = [(0, 2), (0, 3), (1, 2)];
    let b = [(0, 2), (1, 2), (1, 3)];
    let r = match_indexed_graphs(&nodes, &a, &nodes, &b, 0.15);
    assert!(r.ok, "{}", r.detail);
}

#[test]
fn graph_matcher_solves_independent_ambiguous_stars() {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for g in 0..3 {
        let o = Point2::new(g as f64 * 100.0, 0.0);
        let i = nodes.len();
        nodes.extend([o, o, o]);
        edges.push((i, i + 1));
        edges.push((i, i + 2));
    }
    let r = match_indexed_graphs(&nodes, &edges, &nodes, &edges, 0.1);
    assert!(r.ok, "{}", r.detail);
}

#[test]
fn graph_matcher_nearest_is_unbounded() {
    let far = [Point2::new(10_000.0, 0.0)];
    let near = [Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)];
    let prox = bidirectional_proximity(&far, &near);
    assert!((prox.max_a_to_b - 9999.0).abs() < 1e-6);
}

#[test]
fn lake9706_load_and_skeleton() {
    let p = gis_root().join("lake9706.json");
    let poly = load_geojson_polygon(&p).expect("load 9706");
    eprintln!(
        "lake9706 vertices={} holes={}",
        poly.vertices().len(),
        poly.hole_count()
    );
    assert_eq!(poly.hole_count(), 29);
    assert!(poly.vertices().len() >= 2144 && poly.vertices().len() <= 2174);

    let sk = skeleton_graph(&poly);
    assert_dcel_contract(&sk);
    let nseg = sk.segment_count();
    let nnode = sk.node_count();
    let len = sk.total_finite_length();
    eprintln!("lake9706 segs={nseg} nodes={nnode} len={len}");
    assert_eq!(nseg, 4372);
    assert_eq!(nnode, 4344);
    assert!((len - 342587.19566660019).abs() < 1.0);
    assert_eq!(cyclomatic(&sk, 29), 0);
}

/// Progress gate: same node set, counts, and topology as C++ double interior.
#[test]
fn lake9706_cpp_double_node_progress() {
    let obj_path = gis_root().join("lake9706-cpp-double.obj");
    let text = std::fs::read_to_string(&obj_path).expect("cpp obj");
    let obj = ObjSkeleton::parse(&text).expect("parse cpp obj");
    let (cpp, cpp_nodes, cpp_len) = obj_ends(&obj);
    assert_eq!(cpp.len(), 4372);
    assert_eq!(cpp_nodes.len(), 4344);
    assert!((cpp_len - 342587.19566660019).abs() < 1.0);

    let poly = load_geojson_polygon(&gis_root().join("lake9706.json")).expect("load");
    let sk = skeleton_graph(&poly);
    let (rust_nodes, rust_edges) = rust_graph(&sk);
    let rust_len = sk.total_finite_length();

    assert_eq!(
        rust_edges.len(),
        4372,
        "rust segs {} vs cpp 4372",
        rust_edges.len()
    );
    assert!(
        (rust_len - 342587.19566660019).abs() < 1.0,
        "rust len {rust_len} vs cpp {cpp_len}"
    );

    let prox = bidirectional_proximity(&cpp_nodes, &rust_nodes);
    eprintln!(
        "lake9706 graph rust_segs={} rust_xy={} cpp_segs={} cpp_xy={} cpp_to_rust_1mm={}/{} rust_to_cpp_1mm={}/{} max_cpp_to_rust={:.4} max_rust_to_cpp={:.4}",
        rust_edges.len(),
        rust_nodes.len(),
        cpp.len(),
        cpp_nodes.len(),
        prox.a_within_1mm,
        prox.n_a,
        prox.b_within_1mm,
        prox.n_b,
        prox.max_a_to_b,
        prox.max_b_to_a,
    );
    assert!(
        prox.max_a_to_b <= 1.0,
        "a C++ node is {} m from every Rust node",
        prox.max_a_to_b
    );
    assert!(
        prox.max_b_to_a <= 1.0,
        "a Rust node is {} m from every C++ node",
        prox.max_b_to_a
    );
    assert!(
        prox.a_within_1cm * 100 >= cpp_nodes.len() * 99,
        "only {}/{} C++ nodes within 1 cm of Rust",
        prox.a_within_1cm,
        cpp_nodes.len()
    );
    assert!(
        prox.a_within_1mm * 100 >= cpp_nodes.len() * 98,
        "only {}/{} C++ nodes within 1 mm of Rust",
        prox.a_within_1mm,
        cpp_nodes.len()
    );
    let cpp_edges = edges_on_nodes(&cpp, &cpp_nodes, 1e-9).expect("cpp endpoints on unique-xy");
    let report = match_indexed_graphs(&rust_nodes, &rust_edges, &cpp_nodes, &cpp_edges, 0.15);
    eprintln!("lake9706 topology {}", report.detail);
    assert!(report.ok, "{}", report.detail);
}

/// Exact 9706 topology vs C++ double.
#[test]
fn lake9706_matches_cpp_double_interior() {
    let obj_path = gis_root().join("lake9706-cpp-double.obj");
    let text = std::fs::read_to_string(&obj_path).expect("cpp obj");
    let obj = ObjSkeleton::parse(&text).expect("parse cpp obj");
    let (cpp, cpp_nodes, _) = obj_ends(&obj);
    let poly = load_geojson_polygon(&gis_root().join("lake9706.json")).expect("load");
    let sk = skeleton_graph(&poly);
    let (rust_nodes, rust_edges) = rust_graph(&sk);
    assert_eq!(cyclomatic(&sk, 29), 0);
    let cpp_edges = edges_on_nodes(&cpp, &cpp_nodes, 1e-9).expect("cpp endpoints on unique-xy");
    let report = match_indexed_graphs(&rust_nodes, &rust_edges, &cpp_nodes, &cpp_edges, 0.15);
    assert!(report.ok, "9706 graph topology: {}", report.detail);
}

/// Progress gate: C++ exact is 31330/31130 Euler 0. Counts, cyclomatic,
/// proximity, and topology match through the public validated skeleton API.
#[test]
fn lake10623_load_and_skeleton() {
    let obj_path = gis_root().join("lake10623-cpp-exact.obj");
    let text = std::fs::read_to_string(&obj_path).expect("cpp exact obj");
    let obj = ObjSkeleton::parse(&text).expect("parse cpp exact obj");
    let (cpp, cpp_nodes, cpp_len) = obj_ends(&obj);
    assert_eq!(cpp.len(), 31330);
    assert_eq!(cpp_nodes.len(), 31130);
    assert!((cpp_len - 1007737.3828873356).abs() < 1.0);

    let poly = load_geojson_polygon(&gis_root().join("lake10623.json")).expect("load 10623");
    eprintln!(
        "10623 vertices={} holes={}",
        poly.vertices().len(),
        poly.hole_count()
    );
    assert_eq!(poly.hole_count(), 201);
    assert!(poly.vertices().len() >= 15365 && poly.vertices().len() <= 15567);

    let sk = skeleton_graph(&poly);
    assert_dcel_contract(&sk);
    let (rust_nodes, rust_edges) = rust_graph(&sk);
    let rust_len = sk.total_finite_length();
    eprintln!(
        "10623 segs={} nodes={} len={rust_len}",
        rust_edges.len(),
        rust_nodes.len()
    );
    assert_eq!(rust_edges.len(), 31330);
    assert_eq!(rust_nodes.len(), 31130);
    assert!(
        (rust_len - 1007737.3828873356).abs() < 1.0,
        "rust len {rust_len} vs cpp {cpp_len}"
    );
    assert_eq!(cyclomatic(&sk, 201), 0);

    let prox = bidirectional_proximity(&cpp_nodes, &rust_nodes);
    eprintln!(
        "lake10623 progress rust_segs={} rust_xy={} rust_len={rust_len:.6} cpp_segs={} cpp_xy={} cpp_len={cpp_len:.6} cpp_to_rust_1mm={}/{} rust_to_cpp_1mm={}/{} max_cpp_to_rust={:.4} max_rust_to_cpp={:.4}",
        rust_edges.len(),
        rust_nodes.len(),
        cpp.len(),
        cpp_nodes.len(),
        prox.a_within_1mm,
        prox.n_a,
        prox.b_within_1mm,
        prox.n_b,
        prox.max_a_to_b,
        prox.max_b_to_a,
    );
    assert!(
        prox.max_a_to_b <= 0.15,
        "a C++ node is {} m from every Rust node",
        prox.max_a_to_b
    );
    assert!(
        prox.max_b_to_a <= 0.15,
        "a Rust node is {} m from every C++ node",
        prox.max_b_to_a
    );
    assert!(
        prox.a_within_1cm * 100 >= cpp_nodes.len() * 99,
        "only {}/{} C++ nodes within 1 cm of Rust",
        prox.a_within_1cm,
        cpp_nodes.len()
    );
    assert!(
        prox.a_within_1mm * 100 >= cpp_nodes.len() * 98,
        "only {}/{} C++ nodes within 1 mm of Rust",
        prox.a_within_1mm,
        cpp_nodes.len()
    );
    let cpp_edges = edges_on_nodes(&cpp, &cpp_nodes, 1e-9).expect("cpp endpoints on unique-xy");
    let report = match_indexed_graphs(&rust_nodes, &rust_edges, &cpp_nodes, &cpp_edges, 0.15);
    eprintln!("lake10623 topology {}", report.detail);
    assert!(report.ok, "{}", report.detail);
}

/// Exact conformance: 31330 segments, cyclomatic 0, bidirectional graph match.
#[test]
fn lake10623_matches_cpp_exact_interior() {
    let obj_path = gis_root().join("lake10623-cpp-exact.obj");
    let text = std::fs::read_to_string(&obj_path).expect("cpp exact obj");
    let obj = ObjSkeleton::parse(&text).expect("parse cpp exact obj");
    let (cpp, cpp_nodes, cpp_len) = obj_ends(&obj);
    assert_eq!(cpp.len(), 31330);
    assert_eq!(cpp_nodes.len(), 31130);
    assert!((cpp_len - 1007737.3828873356).abs() < 1.0);

    let poly = load_geojson_polygon(&gis_root().join("lake10623.json")).expect("load");
    let sk = skeleton_graph(&poly);
    assert_dcel_contract(&sk);
    let (rust_nodes, rust_edges) = rust_graph(&sk);
    let rust_len = sk.total_finite_length();
    assert_eq!(
        rust_edges.len(),
        31330,
        "rust segs {} vs cpp 31330",
        rust_edges.len()
    );
    assert_eq!(rust_nodes.len(), 31130);
    assert_eq!(cyclomatic(&sk, 201), 0);
    assert!(
        (rust_len - 1007737.3828873356).abs() < 1.0,
        "rust len {rust_len} vs cpp {cpp_len}"
    );
    let cpp_edges = edges_on_nodes(&cpp, &cpp_nodes, 1e-9).expect("cpp endpoints on unique-xy");
    let report = match_indexed_graphs(&rust_nodes, &rust_edges, &cpp_nodes, &cpp_edges, 0.15);
    assert!(report.ok, "10623 graph topology: {}", report.detail);
}
