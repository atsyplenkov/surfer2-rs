# surfer2 (Rust)

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

Headless straight-skeleton library: a source-derived Rust port of
[cgalab/surfer2](https://github.com/cgalab/surfer2) revision
`c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`.

## License

GPL-3.0-or-later. See [`LICENSE`](LICENSE) and [`COPYRIGHT`](COPYRIGHT).

This crate is a translation of Surfer2's headless computational engine.
GUI, CLI, GraphML/OBJ/Ipe I/O, Boost adapters, and CGAL are intentionally
out of scope for the library. The port provides one `f64` engine analogous
to C++ `NT_USE_DOUBLE`; an exact/algebraic kernel is not a goal. Dev-only
fixture tooling under `tests/` may read GraphML for differential checks.

## Status

Work in progress toward source-level behavioral compatibility with the
upstream kinetic triangulation / wavefront propagation engine.

Known parity gaps:

- `lake9706` and `lake10623` each differ from the frozen C++ oracle by one
  south-reflex ridge (same 1/1 topology delta). Segment counts, node counts,
  and cyclomatic numbers match. A global “near-miss is a split” rule is
  unsafe: lake10623 has 100+ other `tparam` near-misses that C++ flips.
- `issue2-minimal1` is 24 kinetic segments vs C++ double 25 (reproduced;
  another split/flip timing difference).
- Additive GraphML `edge-weight-additive` is unsupported (upstream parses
  it and never applies it). Multiplicative `edge-weight` is implemented.
- Offset contours walk C++-style face cycles and are gated on a unit
  square and a rectangle-with-hole. Broad C++ offset comparison (many
  times, topology changes) is still open.

The progress and ignored exact-conformance gates are in `tests/gis.rs`.
Frozen C++ oracle provenance is documented in
[`fixtures/gis/README.txt`](fixtures/gis/README.txt).

## Usage

```rust
use surfer2::{interior_straight_skeleton, Polygon};

let polygon = Polygon::from_flat(&xy, &ring_offsets)?;
let sk = interior_straight_skeleton(&polygon)?;
for seg in sk.finite_segments() {
    let _ = (seg.start, seg.end, seg.start_time, seg.end_time);
}
```

## Upstream provenance

Translated modules carry notices pointing at the corresponding Surfer2
sources under `surf/src/`. Numeric and topological semantics follow that
codebase; serialization, logging, and pointer identity do not.
