//! Parse Surfer2 OBJ skeleton output and invoke the C++ oracle binaries.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use surfer2::Point2;

#[derive(Debug, Clone)]
pub struct ObjSkeleton {
    /// Vertices as (x, y, z=time).
    pub vertices: Vec<[f64; 3]>,
    /// 0-based face index loops.
    pub faces: Vec<Vec<usize>>,
}

#[derive(Debug, Clone)]
pub struct OracleSegment {
    pub start: Point2,
    pub end: Point2,
    pub start_time: f64,
    pub end_time: f64,
}

impl ObjSkeleton {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            match parts.next() {
                Some("v") => {
                    let x: f64 = parts
                        .next()
                        .ok_or("v missing x")?
                        .parse()
                        .map_err(|e| format!("v x: {e}"))?;
                    let y: f64 = parts
                        .next()
                        .ok_or("v missing y")?
                        .parse()
                        .map_err(|e| format!("v y: {e}"))?;
                    let z: f64 = parts
                        .next()
                        .ok_or("v missing z")?
                        .parse()
                        .map_err(|e| format!("v z: {e}"))?;
                    vertices.push([x, y, z]);
                }
                Some("f") => {
                    let mut idx = Vec::new();
                    for p in parts {
                        let head = p.split('/').next().unwrap();
                        let i: isize = head.parse().map_err(|e| format!("f idx: {e}"))?;
                        let i = if i < 0 {
                            (vertices.len() as isize + i) as usize
                        } else {
                            (i as usize).checked_sub(1).ok_or("f idx 0")?
                        };
                        idx.push(i);
                    }
                    if idx.len() >= 2 {
                        faces.push(idx);
                    }
                }
                _ => {}
            }
        }
        Ok(Self { vertices, faces })
    }

    /// Non-input finite skeleton arcs: edges with both endpoints having z > 0,
    /// or mixed z where max z > 0 (drop pure boundary z≈0 edges as upstream R scripts do).
    pub fn finite_nonzero_segments(&self) -> Vec<OracleSegment> {
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for face in &self.faces {
            for w in face.windows(2).chain(std::iter::once(
                [face[face.len() - 1], face[0]].as_slice(),
            )) {
                let i = w[0];
                let j = w[1];
                let a = self.vertices[i];
                let b = self.vertices[j];
                // Drop pure input edges at time 0
                if a[2] == 0.0 && b[2] == 0.0 {
                    continue;
                }
                if a[2].is_infinite() || b[2].is_infinite() {
                    continue;
                }
                let key = if (a[0], a[1], a[2]) <= (b[0], b[1], b[2]) {
                    (a[0].to_bits(), a[1].to_bits(), a[2].to_bits(), b[0].to_bits(), b[1].to_bits(), b[2].to_bits())
                } else {
                    (b[0].to_bits(), b[1].to_bits(), b[2].to_bits(), a[0].to_bits(), a[1].to_bits(), a[2].to_bits())
                };
                if !seen.insert(key) {
                    continue;
                }
                if a[0] == b[0] && a[1] == b[1] {
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

    pub fn finite_node_count(&self) -> usize {
        let mut set = std::collections::BTreeSet::new();
        for seg in self.finite_nonzero_segments() {
            set.insert((seg.start.x.to_bits(), seg.start.y.to_bits()));
            set.insert((seg.end.x.to_bits(), seg.end.y.to_bits()));
        }
        set.len()
    }
}

pub fn double_bin() -> Option<PathBuf> {
    std::env::var_os("SURFER2_DOUBLE_BIN").map(PathBuf::from).or_else(|| {
        let p = PathBuf::from("/tmp/surfer2/build-double/cc/surfer");
        p.exists().then_some(p)
    })
}

pub fn exact_bin() -> Option<PathBuf> {
    std::env::var_os("SURFER2_EXACT_BIN").map(PathBuf::from).or_else(|| {
        let p = PathBuf::from("/tmp/surfer2/build-exact/cc/surfer");
        p.exists().then_some(p)
    })
}

pub fn run_surfer(
    bin: &Path,
    graphml: &Path,
    component: &str,
) -> Result<ObjSkeleton, String> {
    let out = tempfile_obj_path();
    let status = Command::new(bin)
        .arg(format!("--component={component}"))
        .arg(graphml)
        .arg(&out)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("surfer failed: {status}"));
    }
    let text = fs::read_to_string(&out).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&out);
    ObjSkeleton::parse(&text)
}

fn tempfile_obj_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surfer2-oracle-{}-{}.obj",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    p
}
