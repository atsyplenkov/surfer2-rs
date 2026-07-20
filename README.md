# surfer2 (Rust)

Headless straight-skeleton library: a source-derived Rust port of
[cgalab/surfer2](https://github.com/cgalab/surfer2) revision
`c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`.

## License

GPL-3.0-or-later. See [`LICENSE`](LICENSE) and [`COPYRIGHT`](COPYRIGHT).

This crate is a translation of Surfer2's headless computational engine.
GUI, CLI, GraphML/OBJ/Ipe I/O, Boost adapters, and CGAL are intentionally
out of scope for the library. Dev-only fixture tooling under `tests/` may
read GraphML for differential checks.

## Status

Work in progress toward source-level behavioral compatibility with the
upstream kinetic triangulation / wavefront propagation engine.

## Usage (planned public surface)

```rust
use surfer2::{interior_straight_skeleton, KernelMode, Polygon};

let polygon = Polygon::from_flat(&xy, &ring_offsets)?;
let sk = interior_straight_skeleton(&polygon, KernelMode::Robust)?;
for seg in sk.finite_segments() {
    let _ = (seg.start, seg.end, seg.start_time, seg.end_time);
}
```

## Upstream provenance

Translated modules carry notices pointing at the corresponding Surfer2
sources under `surf/src/`. Numeric and topological semantics follow that
codebase; serialization, logging, and pointer identity do not.
