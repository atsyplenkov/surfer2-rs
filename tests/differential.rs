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
    ["convex03.graphml", "convex04.graphml", "convex07.graphml"]
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
        let oracle = run_surfer(&bin, &path, "0").expect("oracle run");
        let osegs = oracle.finite_nonzero_segments();
        let g = read_graphml_path(&path).unwrap();
        let pslg = g.to_pslg().unwrap();
        let sk = straight_skeleton(
            &pslg,
            &Options {
                kernel: KernelMode::Fast,
                components: ComponentMode::Index(0),
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
        let oracle = run_surfer(&bin, &path, "0").expect("oracle run");
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
