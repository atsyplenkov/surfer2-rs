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

- `lake9706` and `lake10623` match the frozen C++ oracles (counts, nodes,
  cyclomatic 0, topology). A global “near-miss is a split” rule is still
  unsafe; the remaining catch-up is a 1e-4 s on-segment SOF after a late flip.
- `convex10` is 17 (C++ double hub chords ~1e-11). Junction snap is 5 mm
  on metre-scale input and near-exact on unit GraphML.
- `issue2-minimal1` is 23 kinetic segments; C++ OBJ 25 is 23 arcs plus
  2 skip-∞ face chords (same family as spoke-1).
- Additive GraphML `edge-weight-additive` is unsupported (upstream parses
  it and never applies it). Multiplicative `edge-weight` is implemented.
- Offset contours walk C++-style face cycles and are gated on a unit
  square and a rectangle-with-hole. Broad C++ offset comparison (many
  times, topology changes) is still open.
- `collinear-split-event` is 17 kinetic; C++ OBJ 19 is 17 plus 2 skip-∞
  face chords (same family as spoke-1). `srpg0000025`/`srpg0000028` are
  73/87 kinetic (C++ OBJ 81/97 = kinetic plus skip-∞).
- `same-speed-convex-hull-000` is 9 kinetic; C++ OBJ 13 is 9 plus 4
  skip-∞ chords. Do not force `tc ≥ now` CH-leave to chase the OBJ
  extras (`spoke-3` NonAdvancingLoop).
- Degree-1 interiors `1/2/3` are 17/17/15 kinetic (C++ OBJ 19/19/19 =
  kinetic plus skip-∞). Both sides of a dangling edge share a component
  (C++ floods around the free tip before seeding the opposite half-edge).
  `degree-one-simple-000` is 15 kinetic (OBJ 19). Remaining degree-one
  gaps: simple-001/002/003, bevels-causing-splits, multi-3/4.
- Still open: weighted inf-fast handler (stub still calls opposing;
  random-1 117 vs OBJ 128 is not skip-∞ — kinetic geometry differs).

The progress and ignored exact-conformance gates are in `tests/gis.rs`.
Frozen C++ oracle provenance is documented in
[`fixtures/gis/README.txt`](fixtures/gis/README.txt).

## Performance (Magadan lakes)

Timed on host `oma` (Intel Core i7-10875H), 2026-08-23, Rust at
`72b8641`. Interior component only. These are wall-clock numbers on one
machine, not a CI guarantee.

**lake9706** (matches frozen C++ *double* oracle for topology/counts):

| Side | What was timed | Median |
|---|---|---:|
| This crate | Criterion `lake9706_fast` (`interior_straight_skeleton`) | **~17.6 ms** |
| This crate | Bare Instant around the same call | **~17 ms** |
| Upstream Surfer2 `c42f9a7`, quiet `NT_USE_DOUBLE` Release | Full process: GraphML in → OBJ out (`--component=1`) | **~81 ms** |

Rust Instant and Criterion agree. The C++ figure includes reading GraphML
and writing an OBJ, so it is not a pure “engine kernel vs engine kernel”
split — but on this box the Rust interior skeleton is clearly faster than
that quiet double build for lake9706.

**lake10623** (matches frozen C++ *exact* oracle; C++ double historically
crashes while building the DCEL and is not a fair speed opponent):

| Side | What was timed | Median |
|---|---|---:|
| This crate | Bare Instant around `interior_straight_skeleton` (3 runs after 1 warmup) | **~0.47 s** |

Re-run notes and helpers live under
[`.scratch/cpp-double-bakeoff/`](.scratch/cpp-double-bakeoff/) (throwaway;
not part of the published crate API).

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
