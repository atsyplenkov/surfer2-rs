//! Magadan lake benchmarks (Milestone 7).
//!
//! Times CDT/setup/propagation/DCEL separately once the engine is complete.
//! Until then this benches the unit-square path as a smoke microbench.

use criterion::{criterion_group, criterion_main, Criterion};
use surfer2::{interior_straight_skeleton, KernelMode, Polygon};

fn bench_unit_square(c: &mut Criterion) {
    let poly =
        Polygon::from_flat(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]], &[0]).unwrap();
    c.bench_function("unit_square_fast", |b| {
        b.iter(|| {
            let sk = interior_straight_skeleton(&poly, KernelMode::Fast).unwrap();
            std::hint::black_box(sk.segment_count())
        });
    });
}

fn bench_lake9706_if_present(c: &mut Criterion) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/gis/lake9706.json");
    if !path.exists() {
        return;
    }
    // Load once outside the timer via a simple parse helper duplicated lightly.
    let text = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let geom = &v["features"][0]["geometry"];
    let rings = geom["coordinates"].as_array().unwrap();
    let mut pts_rings: Vec<Vec<surfer2::Point2>> = rings
        .iter()
        .map(|ring| {
            ring.as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    let a = p.as_array().unwrap();
                    surfer2::Point2::new(a[0].as_f64().unwrap(), a[1].as_f64().unwrap())
                })
                .collect()
        })
        .collect();
    let exterior = pts_rings.remove(0);
    let poly = Polygon::new(exterior, pts_rings).unwrap();

    c.bench_function("lake9706_fast", |b| {
        b.iter(|| {
            match interior_straight_skeleton(&poly, KernelMode::Fast) {
                Ok(sk) => std::hint::black_box(sk.segment_count()),
                Err(_) => 0,
            }
        });
    });
}

criterion_group!(benches, bench_unit_square, bench_lake9706_if_present);
criterion_main!(benches);
