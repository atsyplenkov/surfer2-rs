<h1 align="center"><code>surfer2</code></h1>

<p align="center">
    <a href="https://github.com/atsyplenkov/surfer2/actions/workflows/rust-ci.yml">
        <img src="https://img.shields.io/github/actions/workflow/status/atsyplenkov/surfer2/rust-ci.yml?style=flat&labelColor=1C2C2E&color=dea584&logo=GitHub%20Actions&logoColor=white"></a>
    <a href="https://github.com/atsyplenkov/surfer2/actions/workflows/rust-cd.yml">
        <img src="https://img.shields.io/github/actions/workflow/status/atsyplenkov/surfer2/rust-cd.yml?style=flat&labelColor=1C2C2E&color=dea584&logo=GitHub%20Actions&logoColor=white&label=deploy"></a>
    <a href="https://crates.io/crates/surfer2/">
        <img src="https://img.shields.io/crates/v/surfer2?style=flat&labelColor=1C2C2E&color=dea584&logo=Rust&logoColor=white"></a>
    <a href="https://codecov.io/gh/atsyplenkov/surfer2">
        <img src="https://img.shields.io/codecov/c/gh/atsyplenkov/surfer2?style=flat&labelColor=1C2C2E&color=dea584&logo=Codecov&logoColor=white"></a>
    <a href="https://docs.rs/surfer2/">
        <img src="https://img.shields.io/docsrs/surfer2?style=flat&labelColor=1C2C2E&color=dea584&logo=Rust&logoColor=white"></a>
</p>

This is a Rust implementation of **Surfer2**, the straight-skeleton engine ([cgalab/surfer2](https://github.com/cgalab/surfer2)). The crate is a source-derived Rust port of the original C++/CGAL implementation (revision `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`). Only headless computational engine using `f64` arithmetic analogous to the C++ `NT_USE_DOUBLE` build is implemented. The GUI, CLI, GraphML/OBJ serializers, Boost adapters, and CGAL itself are intentionally out of scope.

> The straight skeleton of a polygon is the trace of its shrinking "wavefront", where every edge moves inward at a constant speed until the shape collapses — the triangulation-based algorithm of Aichholzer and Aurenhammer. Straight skeletons drive offsetting, centerline extraction, and polygon generalisation in GIS and computational geometry.

Offsetting large lake polygons with Surfer2 previously meant building the C++ toolchain against CGAL and shelling out to its CLI. Porting the headless engine to Rust makes the algorithm available as an ordinary library dependency — fast enough for batch processing of real-world water bodies (huge lake polygons used in the benchmarks) and safe to embed in larger applications.

## Features

* Interior straight skeletons of polygons with holes ([`Polygon`](https://docs.rs/surfer2/latest/surfer2/input/struct.Polygon.html)) and straight skeletons of general planar straight-line graphs (`Pslg`);
* Offset polylines at any wavefront time;
* Multiplicative edge weights (upstream's `edge-weight` attribute);
* Component selection: a single component, all components sequentially, or all simultaneously;
* Pure safe Rust — zero `unsafe` blocks, dependency-light (`slotmap`, `spade`, `robust`, `thiserror`, `rustc-hash`);

## Example usage

```rust
use surfer2::{interior_straight_skeleton, offset, Polygon};

// Unit square as one closed ring: flat coordinates plus ring offsets
let xy = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
let polygon = Polygon::from_flat(&xy, &[0])?;

// Interior straight skeleton of the polygon
let sk = interior_straight_skeleton(&polygon)?;

// Skeleton arcs as finite segments with wavefront times
for seg in sk.finite_segments() {
    println!("{:?} -> {:?}", seg.start, seg.end);
}

// Offset contours at wavefront time t = 0.25
let contours = offset(&sk, 0.25)?;
for c in &contours {
    println!("face {}: {} points, closed = {}", c.face, c.points.len(), c.closed);
}
```

## Status

Work in progress toward source-level behavioral compatibility with the upstream kinetic triangulation / wavefront propagation engine. The two lake fixtures match frozen C++ oracles (counts, topology), and the wider exact-conformance gates live in [`tests/gis.rs`](tests/gis.rs). Frozen-oracle provenance and known parity gaps are documented in [`fixtures/gis/README.txt`](fixtures/gis/README.txt).

## Performance

| Fixture | This crate | Upstream Surfer2 `c42f9a7`, quiet `NT_USE_DOUBLE` Release |
|---|---|---:|
| lake9706 | **~17.6 ms** (`interior_straight_skeleton`) | ~81 ms (full process: GraphML in, OBJ out) |
| lake10623 | **~0.47 s** | crashes while building the DCEL |

Timed on one host (Intel Core i7-10875H, 2026-08-23). Reproduce with:

```bash
cargo bench --bench magadan -- lake9706
cargo bench --bench magadan -- lake10623
```

## License and Attribution

GPL-3.0-or-later, matching upstream Surfer2. See [`LICENSE`](LICENSE) and [`COPYRIGHT`](COPYRIGHT). Translated modules carry notices pointing at the corresponding Surfer2 sources under `surf/src/`. Numeric and topological semantics follow that codebase; serialization, logging, and pointer identity do not.

## See also

* [cgalab/surfer2](https://github.com/cgalab/surfer2) — the original C++/CGAL implementation;
* [CGAL 2D Straight Skeletons](https://doc.cgal.org/latest/Straight_skeleton_2/index.html) — the production CGAL package for straight skeletons and offsets.
