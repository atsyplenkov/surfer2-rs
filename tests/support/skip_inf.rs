//! Skip-∞ face-chord classification for C++ OBJ vs Rust kinetic accounting.
//!
//! C++ `SkeletonDCEL::write_obj` omits the infinite vertex from each face
//! cycle. That inserts a face-cycle edge between the two finite neighbors of
//! ∞. Those omit-gap edges are the skip-∞ chords: finite-nonzero face-cycle
//! edges that appear in exactly one OBJ face (intrinsic OBJ structure; no
//! kinetic set-difference, no runaway-length arcs).
//!
//! Count gates use `structural_skip_inf_chords`. On most fixtures
//! `segment_count == obj_finite_nonzero − skip_inf`. On `srpg0000028`, two
//! stored arcs exceed the public runaway length cutoff, so
//! `segment_count + runaway_excluded == obj − skip_inf` with
//! `runaway_excluded` measured from `stored_segments` (not labeled skip-∞).

use std::collections::{HashMap, HashSet};

use surfer2::Point2;

use super::oracle::{ObjSkeleton, OracleSegment};

fn edge_key(a: [f64; 3], b: [f64; 3]) -> (u64, u64, u64, u64, u64, u64) {
    if (a[0], a[1], a[2]) <= (b[0], b[1], b[2]) {
        (
            a[0].to_bits(),
            a[1].to_bits(),
            a[2].to_bits(),
            b[0].to_bits(),
            b[1].to_bits(),
            b[2].to_bits(),
        )
    } else {
        (
            b[0].to_bits(),
            b[1].to_bits(),
            b[2].to_bits(),
            a[0].to_bits(),
            a[1].to_bits(),
            a[2].to_bits(),
        )
    }
}

/// Face-cycle edges keyed by endpoint coordinates, with the faces they appear on.
fn nonzero_edge_faces(obj: &ObjSkeleton) -> HashMap<(u64, u64, u64, u64, u64, u64), Vec<usize>> {
    let mut edge_faces: HashMap<(u64, u64, u64, u64, u64, u64), Vec<usize>> = HashMap::new();
    for (fi, face) in obj.faces.iter().enumerate() {
        let n = face.len();
        if n < 2 {
            continue;
        }
        for k in 0..n {
            let i = face[k];
            let j = face[(k + 1) % n];
            let a = obj.vertices[i];
            let b = obj.vertices[j];
            if a[2] == 0.0 && b[2] == 0.0 {
                continue;
            }
            if !a[2].is_finite() || !b[2].is_finite() {
                continue;
            }
            if a[0] == b[0] && a[1] == b[1] {
                continue;
            }
            edge_faces.entry(edge_key(a, b)).or_default().push(fi);
        }
    }
    edge_faces
}

/// Skip-∞ chords: finite-nonzero face-cycle edges in exactly one OBJ face.
pub fn structural_skip_inf_chords(obj: &ObjSkeleton) -> Vec<OracleSegment> {
    let edge_faces = nonzero_edge_faces(obj);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for face in &obj.faces {
        let n = face.len();
        if n < 2 {
            continue;
        }
        for k in 0..n {
            let i = face[k];
            let j = face[(k + 1) % n];
            let a = obj.vertices[i];
            let b = obj.vertices[j];
            if a[2] == 0.0 && b[2] == 0.0 {
                continue;
            }
            if !a[2].is_finite() || !b[2].is_finite() {
                continue;
            }
            if a[0] == b[0] && a[1] == b[1] {
                continue;
            }
            let key = edge_key(a, b);
            if edge_faces.get(&key).map(|f| f.len()).unwrap_or(0) != 1 {
                continue;
            }
            if !seen.insert(key) {
                continue;
            }
            out.push(OracleSegment {
                start: Point2::new(a[0], a[1]),
                end: Point2::new(b[0], b[1]),
                start_time: a[2],
                end_time: b[2],
            });
        }
    }
    out
}
