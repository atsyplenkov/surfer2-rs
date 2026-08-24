//! Skip-∞ face-chord classification for C++ OBJ vs Rust kinetic accounting.
//!
//! C++ `SkeletonDCEL::write_obj` omits the infinite vertex from each face
//! cycle. That inserts a face-cycle edge between the two finite neighbors of
//! ∞. Those omit-gap edges are the structural skip-∞ chords: finite-nonzero
//! face-cycle edges that appear in exactly one OBJ face.
//!
//! Count gates compare Rust kinetic segments to C++ OBJ via
//! `kinetic == obj_finite_nonzero − classified_skip_inf`. Classification is
//! the OBJ−kinetic difference restricted to chords whose endpoints lie on
//! ∞-terminating faces (faces that contain a structural omit-gap edge). On
//! most fixtures that set equals the structural singles; on `srpg0000028` it
//! also includes two long arcs present in OBJ and in Rust `stored_segments`
//! but excluded from `segment_count` by the runaway length cutoff.

use std::collections::{HashMap, HashSet};

use surfer2::Point2;

use super::oracle::{ObjSkeleton, OracleSegment};

/// Endpoint match tolerance for OBJ↔kinetic identity (not an equation fudge).
const ENDPOINT_TOL: f64 = 1e-7;

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

fn near(a: Point2, b: Point2) -> bool {
    (a.x - b.x).abs() <= ENDPOINT_TOL && (a.y - b.y).abs() <= ENDPOINT_TOL
}

fn matches_kinetic(seg: &OracleSegment, kinetic: &[(Point2, Point2)]) -> bool {
    kinetic.iter().any(|(a, b)| {
        (near(seg.start, *a) && near(seg.end, *b)) || (near(seg.start, *b) && near(seg.end, *a))
    })
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

/// Faces that contain a structural write_obj ∞-omit gap edge.
pub fn infinite_terminating_faces(obj: &ObjSkeleton) -> HashSet<usize> {
    nonzero_edge_faces(obj)
        .into_iter()
        .filter(|(_, faces)| faces.len() == 1)
        .flat_map(|(_, faces)| faces)
        .collect()
}

/// Structural skip-∞ chords: finite-nonzero face-cycle edges in exactly one face.
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

/// Classified skip-∞ accounting chords for the kinetic count equation.
///
/// Returns OBJ finite-nonzero segments that do not match `kinetic` and whose
/// endpoints both lie on ∞-terminating faces (see module docs).
pub fn classify_skip_inf_chords(
    obj: &ObjSkeleton,
    kinetic: &[(Point2, Point2)],
) -> Vec<OracleSegment> {
    let inf_faces = infinite_terminating_faces(obj);
    let mut verts_on_inf: HashSet<(u64, u64, u64)> = HashSet::new();
    for &fi in &inf_faces {
        for &vi in &obj.faces[fi] {
            let v = obj.vertices[vi];
            verts_on_inf.insert((v[0].to_bits(), v[1].to_bits(), v[2].to_bits()));
        }
    }

    let mut out = Vec::new();
    for seg in obj.finite_nonzero_segments() {
        if matches_kinetic(&seg, kinetic) {
            continue;
        }
        let a_on = verts_on_inf.contains(&(
            seg.start.x.to_bits(),
            seg.start.y.to_bits(),
            seg.start_time.to_bits(),
        ));
        let b_on = verts_on_inf.contains(&(
            seg.end.x.to_bits(),
            seg.end.y.to_bits(),
            seg.end_time.to_bits(),
        ));
        if a_on && b_on {
            out.push(seg);
        }
    }
    out
}

/// Convenience: number of classified skip-∞ chords.
pub fn count_skip_inf_chords(obj: &ObjSkeleton, kinetic: &[(Point2, Point2)]) -> usize {
    classify_skip_inf_chords(obj, kinetic).len()
}
