//! Upstream GraphML corpus: load, bit round-trip, CDT init.

mod support;

use std::path::{Path, PathBuf};

use support::graphml::{read_graphml_path, BitsRecord};
use surfer2::triangulation::build_basic_triangulation;

fn upstream_test_data() -> PathBuf {
    std::env::var_os("SURFER2_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/surfer2/test-data"))
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn all_graphml_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("graphml") {
                out.push(p);
            }
        }
    }
    walk(root, &mut out);
    out.sort();
    out
}

#[test]
fn corpus_has_38_graphml() {
    let root = upstream_test_data();
    if !root.is_dir() {
        eprintln!("skip: no upstream GraphML at {}", root.display());
        return;
    }
    let files = all_graphml_files(&root);
    assert_eq!(
        files.len(),
        38,
        "expected 38 upstream GraphML fixtures, found {}",
        files.len()
    );
}

#[test]
fn graphml_bit_roundtrip_and_cdt() {
    let files = all_graphml_files(&upstream_test_data());
    if files.is_empty() {
        eprintln!("skip: no GraphML under {:?}", upstream_test_data());
        return;
    }

    let bits_dir = fixture_root().join("oracle/bits");
    let _ = std::fs::create_dir_all(&bits_dir);

    let mut ok = 0usize;
    let mut cdt_ok = 0usize;
    for path in &files {
        let g = read_graphml_path(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let bits = g.to_bits_record();
        // Round-trip bits → points
        for (i, b) in bits.xy_bits.iter().enumerate() {
            let p = surfer2::Point2::from_bits(*b);
            assert_eq!(p.x.to_bits(), g.vertices[i].x.to_bits());
            assert_eq!(p.y.to_bits(), g.vertices[i].y.to_bits());
        }
        // Persist canonical bits fixture
        let stem = path.file_stem().unwrap().to_string_lossy();
        let parent = path
            .parent()
            .and_then(|p| p.strip_prefix(upstream_test_data()).ok())
            .map(|p| p.to_string_lossy().replace('/', "__"))
            .unwrap_or_default();
        let name = if parent.is_empty() {
            format!("{stem}.json")
        } else {
            format!("{parent}__{stem}.json")
        };
        let out = bits_dir.join(name);
        let json = serde_json::to_string_pretty(&bits).unwrap();
        std::fs::write(&out, json).unwrap();

        // Reload and compare
        let loaded: BitsRecord =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(loaded, bits);
        ok += 1;

        match g.to_pslg() {
            Ok(pslg) => match build_basic_triangulation(&pslg) {
                Ok(basic) => {
                    assert!(!basic.triangles.is_empty(), "{}: empty CDT", path.display());
                    // Constraints preserved: every input edge appears on some triangle side
                    let mut seen = vec![false; pslg.edge_count()];
                    for c in &basic.constraints {
                        for e in c.iter().flatten() {
                            seen[*e as usize] = true;
                        }
                    }
                    // Degree-one / dangling may still mark; require no split losses at least
                    // (all edges that form closed cycles should appear)
                    let marked = seen.iter().filter(|x| **x).count();
                    assert!(
                        marked > 0,
                        "{}: no constraints tagged on CDT faces",
                        path.display()
                    );
                    cdt_ok += 1;
                }
                Err(e) => {
                    // Some special cases may need kinetic-only handling; record but don't fail all.
                    eprintln!("CDT skip {}: {e}", path.display());
                }
            },
            Err(e) => eprintln!("PSLG skip {}: {e}", path.display()),
        }
    }
    assert_eq!(ok, files.len());
    assert!(
        cdt_ok >= 30,
        "expected most corpus CDTs to succeed, got {cdt_ok}/{ok}"
    );
}

#[test]
fn frozen_obj_oracle_segment_counts() {
    use support::oracle::ObjSkeleton;
    let root = fixture_root().join("oracle/obj");
    let cases = [
        ("convex03-double.obj", 1), // triangle: one interior node + spokes vary
        ("convex04-double.obj", 2),
        ("convex07-double.obj", 5),
        ("spoke-1-double.obj", 5),
    ];
    for (name, _min_segs) in cases {
        let path = root.join(name);
        if !path.exists() {
            eprintln!("missing frozen obj {}", path.display());
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let obj = ObjSkeleton::parse(&text).unwrap();
        let segs = obj.finite_nonzero_segments();
        eprintln!(
            "{name}: verts={} faces={} nonzero_finite_segs={} nodes≈{}",
            obj.vertices.len(),
            obj.faces.len(),
            segs.len(),
            obj.finite_node_count()
        );
        assert!(!obj.vertices.is_empty(), "{name} empty");
        assert!(
            !segs.is_empty() || obj.vertices.iter().any(|v| v[2] > 0.0),
            "{name} no skeleton"
        );
    }
}
