//! Magadan hydrography Criterion benches.
//!
//! - `unit_square` — tiny smoke path
//! - `lake9706` — ~2k-vertex lake (C++ double oracle peer)
//! - `lake10623` — ~15k-vertex lake (heavier; fewer samples / longer wall)

use std::path::PathBuf;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use surfer2::{interior_straight_skeleton, Point2, Polygon};

fn gis_json(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/gis")
        .join(name)
}

/// Load a GeoJSON FeatureCollection Polygon (exterior + holes), same shape as
/// the Magadan fixtures and the throwaway bakeoff Instant harness.
fn load_geojson_polygon(path: &std::path::Path) -> Option<Polygon> {
    if !path.exists() {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let geom = v.get("features")?.as_array()?.first()?.get("geometry")?;
    let gtype = geom.get("type")?.as_str()?;
    let coords = geom.get("coordinates")?;
    let rings_raw: Vec<Vec<[f64; 2]>> = match gtype {
        "Polygon" => {
            let rings = coords.as_array()?;
            rings
                .iter()
                .map(|ring| {
                    ring.as_array()?
                        .iter()
                        .map(|p| {
                            let a = p.as_array()?;
                            Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
                        })
                        .collect()
                })
                .collect::<Option<_>>()?
        }
        "MultiPolygon" => {
            let mut out = Vec::new();
            for poly in coords.as_array()? {
                for ring in poly.as_array()? {
                    let pts: Vec<[f64; 2]> = ring
                        .as_array()?
                        .iter()
                        .map(|p| {
                            let a = p.as_array()?;
                            Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
                        })
                        .collect::<Option<_>>()?;
                    out.push(pts);
                }
            }
            out
        }
        _ => return None,
    };
    if rings_raw.is_empty() {
        return None;
    }
    let mut rings: Vec<Vec<Point2>> = rings_raw
        .into_iter()
        .map(|r| r.into_iter().map(Point2::from_array).collect())
        .collect();
    let exterior = rings.remove(0);
    Polygon::new(exterior, rings).ok()
}

fn bench_skeleton(c: &mut Criterion, id: &str, poly: &Polygon) {
    c.bench_function(id, |b| {
        b.iter(|| match interior_straight_skeleton(poly) {
            Ok(sk) => std::hint::black_box(sk.segment_count()),
            Err(_) => 0,
        });
    });
}

fn bench_unit_square(c: &mut Criterion) {
    let poly =
        Polygon::from_flat(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], &[0]).unwrap();
    bench_skeleton(c, "unit_square", &poly);
}

fn bench_lake9706_if_present(c: &mut Criterion) {
    let Some(poly) = load_geojson_polygon(&gis_json("lake9706.json")) else {
        return;
    };
    // ~17 ms/iter on the bakeoff machine — default Criterion sample budget is fine.
    bench_skeleton(c, "lake9706", &poly);
}

fn bench_lake10623_if_present(c: &mut Criterion) {
    let Some(poly) = load_geojson_polygon(&gis_json("lake10623.json")) else {
        return;
    };
    // ~0.45–0.5 s/iter: fewer samples, longer wall, so a full group still finishes.
    // Group so we can give the heavy lake a longer sample budget; filter with
    // `lake10623` (Criterion id is `lake10623/skeleton`).
    let mut group = c.benchmark_group("lake10623");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("skeleton", |b| {
        b.iter(|| match interior_straight_skeleton(&poly) {
            Ok(sk) => std::hint::black_box(sk.segment_count()),
            Err(_) => 0,
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_unit_square,
    bench_lake9706_if_present,
    bench_lake10623_if_present
);
criterion_main!(benches);
