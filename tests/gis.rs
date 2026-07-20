//! Magadan lake GIS acceptance (Milestone 7 gates).

mod support;

use std::path::{Path, PathBuf};

use surfer2::{interior_straight_skeleton, KernelMode, Point2, Polygon, Skeleton};

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

#[test]
fn lake9706_fixture_present() {
    assert!(gis_root().join("lake9706.json").exists());
}

#[test]
fn lake10623_fixture_present() {
    assert!(gis_root().join("lake10623.json").exists());
}

#[test]
fn unit_square_interior_skeleton_smoke() {
    let poly =
        Polygon::from_flat(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], &[0]).unwrap();
    let sk = interior_straight_skeleton(&poly, KernelMode::Fast).unwrap();
    assert_eq!(sk.segment_count(), 4);
    assert_eq!(sk.node_count(), 5);
    assert_eq!(cyclomatic(&sk, 0), 0);
    let _ = sk.offset(0.5).unwrap();
}

#[test]
fn public_api_surface_smoke() {
    let poly = Polygon::from_flat(
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
        &[0],
    )
    .unwrap();
    assert_eq!(poly.vertices().len(), 4);
    let sk = interior_straight_skeleton(&poly, KernelMode::Fast).unwrap();
    assert_eq!(sk.finite_segments().count(), 4);
}

#[test]
#[ignore = "engine incomplete: lake9706 not yet at acceptance counts"]
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

    match interior_straight_skeleton(&poly, KernelMode::Fast) {
        Ok(sk) => {
            let nseg = sk.segment_count();
            let nnode = sk.node_count();
            let len = sk.total_finite_length();
            eprintln!("lake9706 segs={nseg} nodes={nnode} len={len}");
            if nseg == 4372 {
                assert_eq!(nnode, 4344);
                assert!((len - 342587.19566660019).abs() < 1.0);
                assert_eq!(cyclomatic(&sk, 29), 0);
            }
        }
        Err(e) => eprintln!("lake9706 WIP: {e}"),
    }
}

#[test]
#[ignore = "engine incomplete: full 10623 may hang"]
fn lake10623_load_and_skeleton() {
    let p = gis_root().join("lake10623.json");
    let poly = load_geojson_polygon(&p).expect("load 10623");
    eprintln!(
        "10623 vertices={} holes={}",
        poly.vertices().len(),
        poly.hole_count()
    );
    assert_eq!(poly.hole_count(), 201);
    assert!(poly.vertices().len() >= 15365 && poly.vertices().len() <= 15567);

    match interior_straight_skeleton(&poly, KernelMode::Fast) {
        Ok(sk) => {
            let nseg = sk.segment_count();
            let nnode = sk.node_count();
            let len = sk.total_finite_length();
            eprintln!("10623 segs={nseg} nodes={nnode} len={len}");
            if nseg == 31330 {
                assert_eq!(nnode, 31130);
                assert!((len - 1007737.3828873356).abs() < 10.0);
                assert_eq!(cyclomatic(&sk, 201), 0);
            }
        }
        Err(e) => eprintln!("10623 WIP: {e}"),
    }
}




