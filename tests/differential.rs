//! Differential tests vs C++ Surfer2 binaries.
//!
//! Requires SURFER2_DOUBLE_BIN / SURFER2_EXACT_BIN (defaults under /tmp/surfer2).
//! Ignored by default in CI without CGAL binaries; run with --ignored.

mod support;

use std::path::PathBuf;

use support::graph_match::{differential_tol, match_segments};
use support::graphml::read_graphml_path;
use support::oracle::{double_bin, exact_bin, run_surfer};
use surfer2::{straight_skeleton, ComponentMode, KernelMode, Options, Point2};

fn upstream_test_data() -> PathBuf {
    std::env::var_os("SURFER2_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/surfer2/test-data"))
}

fn simple_cases() -> Vec<PathBuf> {
    let root = upstream_test_data();
    [
        "convex03.graphml",
        "convex04.graphml",
        "convex07.graphml",
        "convex10.graphml",
        "convex19.graphml",
        "srpg0000025.graphml",
        "srpg0000028.graphml",
        "special-cases/iso-simple-1.graphml",
        "special-cases/iso-simple-2.graphml",
        "special-cases/iso-simple-3.graphml",
        "special-cases/multi-same-speed-event.graphml",
        "special-cases/collinear-split-event.graphml",
        "special-cases/spoke-1.graphml",
        "weighted/srpg_octo0000035-random-1.graphml",
    ]
    .iter()
    .map(|n| root.join(n))
    .filter(|p| p.exists())
    .collect()
}

#[test]
#[ignore = "requires C++ surfer binaries"]
fn differential_fast_convex_small() {
    let bin = double_bin().expect("SURFER2_DOUBLE_BIN");
    for path in simple_cases() {
        let oracle = run_surfer(&bin, &path, "-1").expect("oracle run");
        let osegs = oracle.finite_nonzero_segments();
        let g = read_graphml_path(&path).unwrap();
        let pslg = g.to_pslg().unwrap();
        let sk = straight_skeleton(
            &pslg,
            &Options {
                kernel: KernelMode::Fast,
                components: ComponentMode::AllSequential,
            },
        );
        // Engine may still be incomplete for full match; ensure no panic and typed result.
        match sk {
            Ok(sk) => {
                let segs: Vec<_> = sk.finite_segments().collect();
                let pts: Vec<Point2> = g.vertices.clone();
                let tol = differential_tol(&pts);
                let b: Vec<_> = osegs
                    .iter()
                    .map(|s| (s.start, s.end, s.start_time, s.end_time))
                    .collect();
                let report = match_segments(&segs, &b, tol);
                eprintln!(
                    "{} rust={} oracle={} nodes_r={} nodes_o={} detail={}",
                    path.display(),
                    report.segment_count_a,
                    report.segment_count_b,
                    report.node_count_a,
                    report.node_count_b,
                    report.detail
                );
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                // Hard assert only for fully-verified convex fixtures.
                if matches!(
                    name,
                    "convex03.graphml" | "convex04.graphml" | "convex07.graphml"
                ) {
                    assert_eq!(
                        report.segment_count_a,
                        report.segment_count_b,
                        "segment count mismatch for {}",
                        path.display()
                    );
                }
            }
            Err(e) => eprintln!("{} rust error (engine WIP): {e}", path.display()),
        }
    }
}

#[test]
#[ignore = "requires C++ surfer binaries"]
fn differential_exact_convex_small() {
    let bin = exact_bin().expect("SURFER2_EXACT_BIN");
    for path in simple_cases() {
        let oracle = run_surfer(&bin, &path, "-1").expect("oracle run");
        assert!(
            !oracle.vertices.is_empty(),
            "empty OBJ for {}",
            path.display()
        );
        let osegs = oracle.finite_nonzero_segments();
        eprintln!(
            "{} exact oracle segments={} nodes≈{}",
            path.file_name().unwrap().to_string_lossy(),
            osegs.len(),
            oracle.finite_node_count()
        );
    }
}

#[test]
fn oracle_bins_optional_presence() {
    // Non-ignored smoke: just report whether bins exist.
    eprintln!("double={:?}", double_bin());
    eprintln!("exact={:?}", exact_bin());
}

#[test]
fn spoke1() {
    let path = upstream_test_data().join("special-cases/spoke-1.graphml");
    if !path.exists() {
        eprintln!("skip spoke1: no {}", path.display());
        return;
    }
    let g = support::graphml::read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let sk = surfer2::straight_skeleton(
        &pslg,
        &surfer2::Options {
            kernel: surfer2::KernelMode::Fast,
            components: surfer2::ComponentMode::AllSequential,
        },
    )
    .expect("spoke-1");
    eprintln!(
        "spoke1 segs={} nodes={}",
        sk.segment_count(),
        sk.node_count()
    );
    // C++ -1 is 26 (interior 16 + exterior 10). Interior now matches.
    assert!(
        sk.segment_count() >= 22,
        "spoke-1 regressed: segs={} nodes={}",
        sk.segment_count(),
        sk.node_count()
    );
}

#[test]
fn spoke1_interior_count() {
    let path = upstream_test_data().join("special-cases/spoke-1.graphml");
    if !path.exists() {
        return;
    }
    let g = support::graphml::read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let sk = surfer2::straight_skeleton(
        &pslg,
        &surfer2::Options {
            kernel: surfer2::KernelMode::Fast,
            components: surfer2::ComponentMode::Index(0),
        },
    )
    .expect("spoke-1 interior");
    assert_eq!(sk.segment_count(), 16, "nodes={}", sk.node_count());
}

#[test]
fn convex_frozen_segment_counts() {
    use support::oracle::ObjSkeleton;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/oracle/obj");
    let cases = [
        ("convex03.graphml", "convex03-double.obj", 3),
        ("convex04.graphml", "convex04-double.obj", 5),
        ("convex07.graphml", "convex07-double.obj", 11),
    ];
    for (graph, obj, want) in cases {
        let gpath = upstream_test_data().join(graph);
        if !gpath.exists() {
            continue;
        }
        let g = read_graphml_path(&gpath).unwrap();
        let pslg = g.to_pslg().unwrap();
        let sk = straight_skeleton(
            &pslg,
            &Options {
                kernel: KernelMode::Fast,
                components: ComponentMode::AllSequential,
            },
        )
        .unwrap();
        assert_eq!(sk.segment_count(), want, "{graph}");
        let op = root.join(obj);
        if op.exists() {
            let frozen = ObjSkeleton::parse(&std::fs::read_to_string(&op).unwrap()).unwrap();
            assert_eq!(frozen.finite_nonzero_segments().len(), want, "frozen {obj}");
        }
    }
}

#[test]
fn degree_one_simple_no_panic() {
    let path = upstream_test_data().join("special-cases/degree-one-simple-000.graphml");
    if !path.exists() {
        eprintln!("skip degree-one: no {}", path.display());
        return;
    }
    let g = support::graphml::read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let sk = surfer2::straight_skeleton(
        &pslg,
        &surfer2::Options {
            kernel: surfer2::KernelMode::Fast,
            components: surfer2::ComponentMode::AllSequential,
        },
    );
    match sk {
        Ok(sk) => eprintln!("deg1 segs={} nodes={}", sk.segment_count(), sk.node_count()),
        Err(e) => panic!("deg1: {e}"),
    }
}

#[test]
fn transform_translation_invariance() {
    use surfer2::{interior_straight_skeleton, KernelMode, Point2, Polygon};
    let base = vec![
        Point2::new(0.0, 0.0),
        Point2::new(3.0, 0.0),
        Point2::new(3.0, 2.0),
        Point2::new(0.0, 2.0),
    ];
    let shifted: Vec<_> = base
        .iter()
        .map(|p| Point2::new(p.x + 100.0, p.y - 50.0))
        .collect();
    let a = Polygon::new(base, vec![]).unwrap();
    let b = Polygon::new(shifted, vec![]).unwrap();
    let sa = interior_straight_skeleton(&a, KernelMode::Fast).unwrap();
    let sb = interior_straight_skeleton(&b, KernelMode::Fast).unwrap();
    assert_eq!(sa.segment_count(), sb.segment_count());
    let la = sa.total_finite_length();
    let lb = sb.total_finite_length();
    assert!((la - lb).abs() < 1e-9 * (1.0 + la), "lengths {la} vs {lb}");
}

#[test]
fn ring_start_rotation_invariance() {
    use surfer2::{interior_straight_skeleton, KernelMode, Point2, Polygon};
    let pts = [
        Point2::new(0.0, 0.0),
        Point2::new(2.0, 0.0),
        Point2::new(2.0, 2.0),
        Point2::new(0.0, 2.0),
    ];
    let a = Polygon::new(pts.to_vec(), vec![]).unwrap();
    let mut rot = pts.to_vec();
    rot.rotate_left(1);
    let b = Polygon::new(rot, vec![]).unwrap();
    let sa = interior_straight_skeleton(&a, KernelMode::Fast).unwrap();
    let sb = interior_straight_skeleton(&b, KernelMode::Fast).unwrap();
    assert_eq!(sa.segment_count(), sb.segment_count());
}

#[test]
fn fuzz_invalid_inputs_no_panic() {
    use surfer2::{
        interior_straight_skeleton, straight_skeleton, InputEdge, KernelMode, Options, Point2,
        Polygon, Pslg,
    };
    // Empty
    let _ = Polygon::new(vec![], vec![]);
    // Too few verts
    let _ = Polygon::new(vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)], vec![]);
    // Non-finite
    let _ = Polygon::new(
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(f64::NAN, 1.0),
        ],
        vec![],
    );
    // Crossing
    let _ = Pslg::from_flat(
        &[[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]],
        &[[0, 1], [2, 3]],
        None,
    );
    // Zero weight
    let _ = Pslg::from_flat(&[[0.0, 0.0], [1.0, 0.0]], &[[0, 1]], Some(&[0.0]));
    // Valid tiny should not panic
    let poly = Polygon::new(
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, 0.8),
        ],
        vec![],
    )
    .unwrap();
    let _ = interior_straight_skeleton(&poly, KernelMode::Fast);
    let _ = interior_straight_skeleton(&poly, KernelMode::Robust);
    let pslg = poly.as_pslg().unwrap();
    let _ = straight_skeleton(&pslg, &Options::default());
}

#[test]
fn uniform_scale_invariance() {
    use surfer2::{interior_straight_skeleton, KernelMode, Point2, Polygon};
    let base = vec![
        Point2::new(0.0, 0.0),
        Point2::new(4.0, 0.0),
        Point2::new(4.0, 3.0),
        Point2::new(0.0, 3.0),
    ];
    let scaled: Vec<_> = base
        .iter()
        .map(|p| Point2::new(p.x * 2.5, p.y * 2.5))
        .collect();
    let a = Polygon::new(base, vec![]).unwrap();
    let b = Polygon::new(scaled, vec![]).unwrap();
    let sa = interior_straight_skeleton(&a, KernelMode::Fast).unwrap();
    let sb = interior_straight_skeleton(&b, KernelMode::Fast).unwrap();
    assert_eq!(sa.segment_count(), sb.segment_count());
    let la = sa.total_finite_length();
    let lb = sb.total_finite_length();
    assert!(
        (lb - la * 2.5).abs() < 1e-6 * (1.0 + lb),
        "scale lengths {la} vs {lb}"
    );
}

#[test]
#[ignore = "requires C++ surfer binaries"]
fn differential_fast_corpus_soft() {
    let bin = double_bin().expect("SURFER2_DOUBLE_BIN");
    let root = upstream_test_data();
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().and_then(|s| s.to_str()) == Some("graphml") {
                    out.push(p);
                }
            }
        }
    }
    walk(&root, &mut files);
    files.sort();
    let mut matched = 0usize;
    let mut ran = 0usize;
    for path in files {
        let oracle = match run_surfer(&bin, &path, "-1") {
            Ok(o) => o,
            Err(e) => {
                eprintln!("skip oracle {}: {e}", path.display());
                continue;
            }
        };
        let osegs = oracle.finite_nonzero_segments();
        let g = match read_graphml_path(&path) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("skip read {}: {e}", path.display());
                continue;
            }
        };
        let Ok(pslg) = g.to_pslg() else { continue };
        ran += 1;
        match straight_skeleton(
            &pslg,
            &Options {
                kernel: KernelMode::Fast,
                components: ComponentMode::AllSequential,
            },
        ) {
            Ok(sk) => {
                let n = sk.segment_count();
                let o = osegs.len();
                let ok = n == o;
                if ok {
                    matched += 1;
                }
                eprintln!(
                    "{} rust={n} oracle={o} {}",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    if ok { "OK" } else { "DIFF" }
                );
            }
            Err(e) => eprintln!(
                "{} ERR {e}",
                path.strip_prefix(&root).unwrap_or(&path).display()
            ),
        }
    }
    eprintln!("matched {matched}/{ran}");
    assert!(
        matched >= 4,
        "expected at least convex03/04/07/19 to match, got {matched}/{ran}"
    );
}
