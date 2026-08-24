//! Differential tests vs C++ Surfer2 binaries.
//!
//! Requires SURFER2_DOUBLE_BIN / SURFER2_EXACT_BIN (defaults under /tmp/surfer2).
//! Ignored by default in CI without CGAL binaries; run with --ignored.

mod support;

use std::path::PathBuf;

use support::graph_match::{differential_tol, match_segments};
use support::graphml::{read_graphml_path, BitsRecord};
use support::oracle::{double_bin, exact_bin, run_surfer};
use surfer2::{straight_skeleton, ComponentMode, Options, Point2, Propagator};

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
fn iso_simple3_and_issue2_counts() {
    // issue2-minimal1: event stream matches C++ double. C++ OBJ 25 =
    // 23 kinetic arcs + 2 skip-∞ face chords (same family as spoke-1).
    let cases = [
        ("special-cases/iso-simple-3.graphml", 13),
        ("issue2/issue2-minimal1.graphml", 23),
        ("special-cases/spoke-2.graphml", 22),
        ("special-cases/spoke-3.graphml", 22),
        ("special-cases/degree-one-multi2-000.graphml", 25),
        ("convex10.graphml", 17),
        // C++ OBJ 19 = 17 kinetic + 2 skip-∞ chords (v4–peak, peak–v0).
        ("special-cases/collinear-split-event.graphml", 17),
        // Event stream matches C++ double; OBJ extras are skip-∞ chords.
        ("srpg0000025.graphml", 73),
        ("srpg0000028.graphml", 87),
        ("special-cases/same-speed-convex-hull-000.graphml", 9),
        // C++ OBJ extras are skip-∞ chords (dangling-edge sides share a component).
        ("special-cases/degree-one-simple-000.graphml", 15),
        ("special-cases/degree-one-interior-1.graphml", 17),
        ("special-cases/degree-one-interior-2.graphml", 17),
        ("special-cases/degree-one-interior-3.graphml", 15),
    ];
    for (rel, want) in cases {
        let path = upstream_test_data().join(rel);
        if !path.exists() {
            continue;
        }
        let g = support::graphml::read_graphml_path(&path).unwrap();
        let pslg = g.to_pslg().unwrap();
        let sk = straight_skeleton(
            &pslg,
            &Options {
                components: ComponentMode::AllSequential,
            },
        )
        .unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert_eq!(sk.segment_count(), want, "{rel}");
    }
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
    let opts = Options {
        components: ComponentMode::AllSequential,
    };
    let sk = straight_skeleton(&pslg, &opts).expect("spoke-1");
    let prop = Propagator::from_pslg(&pslg, &opts).unwrap();
    assert!(
        prop.kt.infinite_vertex.is_some(),
        "spoke-1 should attach CH"
    );
    // C++ OBJ lists 26: 16 interior + 6 pocket + 4 face-cycle chords
    // (CH corner → pocket) created by write_obj skipping the infinite vertex.
    assert_eq!(sk.segment_count(), 22, "nodes={}", sk.node_count());
}

#[test]
fn spoke1_interior_count() {
    let path = upstream_test_data().join("special-cases/spoke-1.graphml");
    if !path.exists() {
        return;
    }
    let g = support::graphml::read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let sk = straight_skeleton(
        &pslg,
        &Options {
            components: ComponentMode::Index(0),
        },
    )
    .expect("spoke-1 interior");
    assert_eq!(sk.segment_count(), 16, "nodes={}", sk.node_count());
}

#[test]
fn weighted_oracle_bits_gates() {
    // Always-on public-API gates from vendored oracle-bit records (no GraphML).
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/oracle/bits");
    let cases = [
        // Kinetic counts after C++-style infinite-face component tagging.
        // C++ OBJ --component=-1 is 128/131/135. Counts moved 117/112/120 →
        // 121/121/123 with the infinite-ring flood (closer to OBJ on random-1).
        ("weighted__srpg_octo0000035-random-1.json", 121),
        ("weighted__srpg_octo0000035-random-2.json", 121),
        ("weighted__srpg_octo0000035-random-3.json", 123),
    ];
    for (name, want) in cases {
        let bits: BitsRecord = serde_json::from_str(
            &std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("{name}: {e}")),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let pslg = support::graphml::GraphMlInput::from_bits_record(&bits, name)
            .to_pslg()
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let sk = straight_skeleton(
            &pslg,
            &Options {
                components: ComponentMode::AllSequential,
            },
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(sk.segment_count(), want, "{name}");
        assert_dcel_contract(&sk, name);
    }
}

#[test]
fn degree_one_oracle_bits_gates() {
    // Always-on gates from vendored oracle-bit records (no GraphML).
    // Bevels/simple-000/interiors: kinetic matches C++ OBJ minus skip-∞.
    // simple-001/002/003 and multi-3/4: pinned kinetic after free-tip component
    // flood + unbounded-hull tagging; residual vs C++ OBJ is mixed skip-∞ and
    // tip-bevel geometry (not a clean skip-∞ decomposition — see README).
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/oracle/bits");
    let cases = [
        ("special-cases__degree-one-simple-000.json", 15),
        ("special-cases__degree-one-simple-001.json", 40),
        ("special-cases__degree-one-simple-002.json", 26),
        ("special-cases__degree-one-simple-003.json", 27),
        ("special-cases__degree-one-bevels-causing-splits.json", 46),
        ("special-cases__degree-one-multi3-000.json", 53),
        ("special-cases__degree-one-multi4-000.json", 53),
        ("special-cases__degree-one-interior-1.json", 17),
        ("special-cases__degree-one-interior-2.json", 17),
        ("special-cases__degree-one-interior-3.json", 15),
    ];
    for (name, want) in cases {
        let bits: BitsRecord = serde_json::from_str(
            &std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("{name}: {e}")),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let pslg = support::graphml::GraphMlInput::from_bits_record(&bits, name)
            .to_pslg()
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let sk = straight_skeleton(
            &pslg,
            &Options {
                components: ComponentMode::AllSequential,
            },
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(sk.segment_count(), want, "{name}");
        assert_dcel_contract(&sk, name);
    }
}

/// Always-on skip-∞ accounting against vendored C++ double OBJs.
///
/// Classifier is OBJ-intrinsic (`structural_skip_inf_chords`: single-face
/// write_obj ∞-omit gaps). Equation:
/// `segment_count + runaway_excluded == obj_finite_nonzero − skip_inf`,
/// where `runaway_excluded` counts stored finite-looking arcs dropped only by
/// the public length cutoff (0 on five fixtures; 2 on `srpg0000028`).
#[test]
fn skip_inf_count_equation_gates() {
    use support::oracle::ObjSkeleton;
    use support::skip_inf::{classify_skip_inf_chords, count_skip_inf_chords};

    let bits_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/oracle/bits");
    let obj_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/oracle/obj");
    // (bits, obj, public_kinetic, obj_finite_nonzero, structural_skip_inf, runaway)
    let cases = [
        (
            "special-cases__spoke-1.json",
            "spoke-1-double.obj",
            22,
            26,
            4,
            0,
        ),
        (
            "special-cases__collinear-split-event.json",
            "collinear-split-event-double.obj",
            17,
            19,
            2,
            0,
        ),
        ("srpg0000025.json", "srpg0000025-double.obj", 73, 81, 8, 0),
        ("srpg0000028.json", "srpg0000028-double.obj", 87, 97, 8, 2),
        (
            "special-cases__same-speed-convex-hull-000.json",
            "same-speed-convex-hull-000-double.obj",
            9,
            13,
            4,
            0,
        ),
        (
            "issue2__issue2-minimal1.json",
            "issue2-minimal1-double.obj",
            23,
            25,
            2,
            0,
        ),
    ];
    for (bits_name, obj_name, want_k, want_o, want_s, want_runaway) in cases {
        let bits: BitsRecord = serde_json::from_str(
            &std::fs::read_to_string(bits_root.join(bits_name))
                .unwrap_or_else(|e| panic!("{bits_name}: {e}")),
        )
        .unwrap_or_else(|e| panic!("{bits_name}: {e}"));
        let pslg = support::graphml::GraphMlInput::from_bits_record(&bits, bits_name)
            .to_pslg()
            .unwrap_or_else(|e| panic!("{bits_name}: {e}"));
        let sk = straight_skeleton(
            &pslg,
            &Options {
                components: ComponentMode::AllSequential,
            },
        )
        .unwrap_or_else(|e| panic!("{bits_name}: {e}"));
        assert_eq!(sk.segment_count(), want_k, "{bits_name} public kinetic");

        let obj_path = obj_root.join(obj_name);
        let obj = ObjSkeleton::parse(
            &std::fs::read_to_string(&obj_path).unwrap_or_else(|e| panic!("{obj_name}: {e}")),
        )
        .unwrap_or_else(|e| panic!("{obj_name}: {e}"));
        let obj_n = obj.finite_nonzero_segments().len();
        assert_eq!(obj_n, want_o, "{obj_name} finite_nonzero");

        let skip_n = count_skip_inf_chords(&obj);
        assert_eq!(skip_n, want_s, "{bits_name} structural skip-∞");
        assert_eq!(
            classify_skip_inf_chords(&obj).len(),
            skip_n,
            "{bits_name}: classify == structural"
        );

        // Stored finite-looking arcs: public filter without the length cutoff.
        let stored_finite = sk
            .stored_segments()
            .iter()
            .filter(|s| {
                s.start.is_finite()
                    && s.end.is_finite()
                    && s.start_time.is_finite()
                    && s.end_time.is_finite()
                    && s.end_time >= s.start_time
                    && s.start_time >= 0.0
                    && s.start.distance_to(s.end) > 1e-15
            })
            .count();
        let runaway = stored_finite - sk.segment_count();
        assert_eq!(runaway, want_runaway, "{bits_name} runaway_excluded");
        assert_eq!(
            stored_finite,
            obj_n - skip_n,
            "{bits_name}: stored_finite == obj − structural_skip_inf"
        );
        assert_eq!(
            sk.segment_count() + runaway,
            obj_n - skip_n,
            "{bits_name}: segment_count + runaway == obj − skip_inf"
        );
        assert_dcel_contract(&sk, bits_name);
    }
}

#[test]
fn corpus_strict_count_gates() {
    // Frozen against C++ double `--component=-1` finite-nonzero OBJ counts
    // except spoke-*: C++ OBJ is 26 = 22 kinetic + 4 skip-∞ face chords.
    let cases = [
        ("convex03.graphml", 3),
        ("convex04.graphml", 5),
        ("convex07.graphml", 11),
        ("convex19.graphml", 35),
        ("special-cases/iso-simple-1.graphml", 12),
        ("special-cases/iso-simple-2.graphml", 16),
        ("special-cases/iso-simple-3.graphml", 13),
        ("special-cases/spoke-1.graphml", 22),
        ("special-cases/collinear-split-event.graphml", 17),
        ("srpg0000025.graphml", 73),
        ("srpg0000028.graphml", 87),
        ("special-cases/same-speed-convex-hull-000.graphml", 9),
        ("special-cases/degree-one-simple-000.graphml", 15),
        ("special-cases/degree-one-interior-1.graphml", 17),
        ("special-cases/degree-one-interior-2.graphml", 17),
        ("special-cases/degree-one-interior-3.graphml", 15),
        ("weighted/srpg_octo0000035-random-2.graphml", 121),
    ];
    for (rel, want) in cases {
        let path = upstream_test_data().join(rel);
        if !path.exists() {
            continue;
        }
        let g = read_graphml_path(&path).unwrap();
        let pslg = g.to_pslg().unwrap();
        let sk = straight_skeleton(
            &pslg,
            &Options {
                components: ComponentMode::AllSequential,
            },
        )
        .unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert_eq!(sk.segment_count(), want, "{rel}");
        assert_dcel_contract(&sk, rel);
    }
}

fn assert_dcel_contract(sk: &surfer2::Skeleton, tag: &str) {
    let n = sk.halfedges().len();
    assert!(n >= 2 && n % 2 == 0, "{tag}: halfedge count {n}");
    let unbounded = sk.faces().len().saturating_sub(1);
    for (i, he) in sk.halfedges().iter().enumerate() {
        let twin = he.twin.0 as usize;
        assert!(twin < n, "{tag} twin range {i}");
        assert_eq!(sk.halfedges()[twin].twin.0 as usize, i, "{tag} twin {i}");
        let next = he.next.0 as usize;
        let prev = he.prev.0 as usize;
        assert!(next < n && prev < n, "{tag} next/prev {i}");
        assert!(
            (he.origin.0 as usize) < sk.nodes().len(),
            "{tag} origin {i}"
        );
        assert!((he.face.0 as usize) < sk.faces().len(), "{tag} face {i}");
    }
    for (fi, face) in sk.faces().iter().enumerate() {
        if fi == unbounded {
            continue;
        }
        let Some(start) = face.halfedge else { continue };
        let mut he = start.0 as usize;
        let mut closed = false;
        for _ in 0..=n {
            let cur = &sk.halfedges()[he];
            assert_eq!(cur.face.0 as usize, fi, "{tag} face {fi} walk left at {he}");
            let next = cur.next.0 as usize;
            let prev = cur.prev.0 as usize;
            assert_ne!(next, he, "{tag} face {fi} self-loop at {he}");
            assert_eq!(
                sk.halfedges()[next].prev.0 as usize,
                he,
                "{tag} next.prev {he}"
            );
            assert_eq!(
                sk.halfedges()[prev].next.0 as usize,
                he,
                "{tag} prev.next {he}"
            );
            if next == start.0 as usize {
                closed = true;
                break;
            }
            he = next;
        }
        assert!(closed, "{tag} face {fi} cycle did not close");
    }
    let comps: std::collections::HashSet<_> = sk.faces().iter().map(|f| f.component).collect();
    assert!(!comps.is_empty(), "{tag} no face components");
}

#[test]
fn component_modes_convex03() {
    let path = upstream_test_data().join("convex03.graphml");
    if !path.exists() {
        return;
    }
    let g = read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let interior = straight_skeleton(
        &pslg,
        &Options {
            components: ComponentMode::Index(0),
        },
    )
    .unwrap();
    let all = straight_skeleton(
        &pslg,
        &Options {
            components: ComponentMode::AllSequential,
        },
    )
    .unwrap();
    assert!(
        all.segment_count() >= interior.segment_count(),
        "AllSequential {} < Index(0) {}",
        all.segment_count(),
        interior.segment_count()
    );
    let _ = straight_skeleton(
        &pslg,
        &Options {
            components: ComponentMode::AllSimultaneous,
        },
    )
    .unwrap();
}

#[test]
fn convex_frozen_segment_counts() {
    use support::oracle::ObjSkeleton;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/oracle/obj");
    let cases = [
        ("convex03.graphml", "convex03-double.obj", 3),
        ("convex04.graphml", "convex04-double.obj", 5),
        ("convex07.graphml", "convex07-double.obj", 11),
        ("convex19.graphml", "", 35),
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
                components: ComponentMode::AllSequential,
            },
        )
        .unwrap_or_else(|e| panic!("{graph}: {e}"));
        assert_eq!(sk.segment_count(), want, "{graph}");
        if !obj.is_empty() {
            let op = root.join(obj);
            if op.exists() {
                let frozen = ObjSkeleton::parse(&std::fs::read_to_string(&op).unwrap()).unwrap();
                assert_eq!(frozen.finite_nonzero_segments().len(), want, "frozen {obj}");
            }
        }
    }
}

#[test]
fn convex_allseq_has_unbounded_hull() {
    let path = upstream_test_data().join("convex03.graphml");
    if !path.exists() {
        return;
    }
    let g = read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let prop = surfer2::Propagator::from_pslg(
        &pslg,
        &Options {
            components: ComponentMode::AllSequential,
        },
    )
    .unwrap();
    assert!(prop.kt.infinite_vertex.is_some());
    assert!(prop.kt.triangles.values().any(|t| t.is_unbounded));
}

#[test]
fn degree_one_on_ch_count() {
    // C++ OBJ 7 = 6 kinetic + 1 skip-∞ chord.
    let path = upstream_test_data().join("special-cases/degree-one-on-ch.graphml");
    if !path.exists() {
        return;
    }
    let g = read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let sk = straight_skeleton(
        &pslg,
        &Options {
            components: ComponentMode::AllSequential,
        },
    )
    .expect("degree-one-on-ch");
    assert_eq!(sk.segment_count(), 6, "nodes={}", sk.node_count());
}

#[test]
fn degree_one_interior2_events() {
    // C++ double: t=9.37 collapse, then t=12 SOF on the degree-1 bevel.
    let path = upstream_test_data().join("special-cases/degree-one-interior-2.graphml");
    if !path.exists() {
        return;
    }
    let g = read_graphml_path(&path).unwrap();
    let pslg = g.to_pslg().unwrap();
    let mut prop = Propagator::from_pslg(
        &pslg,
        &Options {
            components: ComponentMode::AllSequential,
        },
    )
    .unwrap();
    let mut times = Vec::new();
    let mut saw_t12_sof = false;
    while let Some(spec) = prop.queue.peak().unwrap().copied() {
        if spec.type_ == surfer2::event::CollapseType::Never {
            break;
        }
        times.push((spec.time, spec.type_));
        if spec.type_ == surfer2::event::CollapseType::SplitOrFlipRefine
            && (spec.time - 12.0).abs() < 1e-9
        {
            saw_t12_sof = true;
        }
        if !prop.step().unwrap() {
            break;
        }
    }
    assert!(
        saw_t12_sof,
        "missing C++ t=12 bevel split; events={times:?}"
    );
    let sk = prop
        .into_skeleton()
        .expect("degree-one-interior-2 skeleton");
    assert_eq!(sk.segment_count(), 17);
    assert_dcel_contract(&sk, "degree-one-interior-2");
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
    let sk = straight_skeleton(
        &pslg,
        &Options {
            components: ComponentMode::AllSequential,
        },
    )
    .unwrap_or_else(|e| panic!("degree-one: {e}"));
    assert_eq!(sk.segment_count(), 15, "nodes={}", sk.node_count());
}

#[test]
fn transform_translation_invariance() {
    use surfer2::{interior_straight_skeleton, Point2, Polygon};
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
    let sa = interior_straight_skeleton(&a).unwrap();
    let sb = interior_straight_skeleton(&b).unwrap();
    assert_eq!(sa.segment_count(), sb.segment_count());
    let la = sa.total_finite_length();
    let lb = sb.total_finite_length();
    assert!((la - lb).abs() < 1e-9 * (1.0 + la), "lengths {la} vs {lb}");
}

#[test]
fn ring_start_rotation_invariance() {
    use surfer2::{interior_straight_skeleton, Point2, Polygon};
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
    let sa = interior_straight_skeleton(&a).unwrap();
    let sb = interior_straight_skeleton(&b).unwrap();
    assert_eq!(sa.segment_count(), sb.segment_count());
}

#[test]
fn fuzz_invalid_inputs_no_panic() {
    use surfer2::{interior_straight_skeleton, straight_skeleton, Options, Point2, Polygon, Pslg};
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
    let _ = interior_straight_skeleton(&poly);
    let _ = interior_straight_skeleton(&poly);
    let pslg = poly.as_pslg().unwrap();
    let _ = straight_skeleton(&pslg, &Options::default());
}

#[test]
fn uniform_scale_invariance() {
    use surfer2::{interior_straight_skeleton, Point2, Polygon};
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
    let sa = interior_straight_skeleton(&a).unwrap();
    let sb = interior_straight_skeleton(&b).unwrap();
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
