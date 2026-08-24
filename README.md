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
  cyclomatic 0, topology). Late-flip SOF (ticket 04): upstream
  `compute_split_or_flip_event_bounded_constrained_1` (c42f9a7) emits SOF
  only for future / exact-now (accept/sign) / ALWAYS on-line — past line
  hits are NEVER, with no catch-up window. Rust keeps a narrow f64-only
  on-segment ≤1e-4 s catch-up (`missed_on_segment_split`) so late flips
  that leave the vertex already on-segment still emit now-time SOF; a
  global “near-miss is a split” rule stays disabled (widen only if a
  fixture proves need and lakes+corpus stay green).
- Junction snap is a pinned scale-aware decision (not an open gap): metre GIS
  (footprint diagonal ≥ 1 km) snaps at 5 mm; unit GraphML uses near-exact eps
  so C++ double's ~1e-11 hub chords are not merged. `convex10` is gated at 17
  kinetic segments on the unit branch (`convex10_oracle_bits_gate`).
- Skip-∞ accounting is closed for the corpus fixtures below (test-support
  structural classifier: single-face write_obj ∞-omit chords; engine still
  returns kinetic only). Gated in `skip_inf_count_equation_gates` against
  vendored `*-double.obj`:
  - `spoke-1`: 22 = 26 − 4
  - `collinear-split-event`: 17 = 19 − 2
  - `srpg0000025`: 73 = 81 − 8
  - `srpg0000028`: 87 public + 2 runaway-filtered = 97 − 8
  - `same-speed-convex-hull-000`: 9 = 13 − 4
  - `issue2-minimal1`: 23 = 25 − 2
- Additive GraphML `edge-weight-additive` is unsupported (upstream parses
  it and never applies it). Multiplicative `edge-weight` is implemented.
- Offset contours walk C++-style face cycles. Current gates are named
  property / API-contract smokes only (`unit_square_interior_skeleton_smoke`,
  `rectangle_with_hole_skeleton_and_offset_smoke`,
  `offset_rejects_bad_time_and_does_not_panic`) — no C++ offset artifacts are
  vendored under `fixtures/` yet, so there are zero additional C++-backed
  offset gates. Broader C++ offset conformance (many times, topology changes,
  corpus geometries) is **blocked-on-oracle-generation**; see
  `fixtures/gis/README.txt` “Producing C++ offset oracles”.
- Do not force `tc ≥ now` CH-leave to chase OBJ extras on
  `same-speed-convex-hull-000` (`spoke-3` NonAdvancingLoop); skip-∞
  accounting already closes that fixture’s count equation.
- Degree-1 family: free-tip component flood matches C++ (both sides of a
  dangling edge share one component; infinite CDT faces participate). Oracle-bit
  gates pin AllSequential kinetic counts. Exact vs OBJ−skip-∞: interiors
  `1/2/3` 17/17/15 (OBJ 19), `simple-000` 15 (OBJ 19), `bevels-causing-splits`
  46 (OBJ 53). Still mixed residual (not clean skip-∞): `simple-001/002/003`
  40/26/27 (OBJ 49/33/35) and `multi-3/4` 53/53 (OBJ 65/66) — tip-bevel
  geometry still differs.
- Weighted infinitely-fast events use the dedicated weighted handler
  (oracle-bit gates for `srpg_octo0000035-random-{1,2,3}` at 121/121/123
  kinetic after infinite-face tagging; C++ double OBJ is 128/131/135).
  Remaining kinetic↔OBJ gap is geometry/event-mix, not a stub to restore.

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
| This crate | Criterion `lake9706` (`interior_straight_skeleton`) | **~17.6 ms** |
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
| This crate | Criterion `lake10623/skeleton` | **~0.82 s** |

Instant and Criterion can disagree here when the machine is warm or busy; treat
both as ballpark wall times, not a tight SLA.

Criterion targets (load fixtures from `fixtures/gis/` when present):

```bash
cargo bench --bench magadan -- unit_square
cargo bench --bench magadan -- lake9706
cargo bench --bench magadan -- lake10623
```

Re-run notes for the C++ double bakeoff live under
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
