# Handoff: surfer2 Rust port (headless)

Date: 2026-08-17
Repo: `/home/ats/Development/surfer2` — git `master` (see latest commits below)
Upstream: `cgalab/surfer2` @ `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`
Ponytail **full** is on (user rule). Off only: `stop ponytail` / `normal mode`.

Plan file `local://surfer2-rust-port-plan.md` is **not on disk**. Do not invent it.

## Goal

Native Rust crate `surfer2` (GPL-3.0-or-later): headless computational semantics of Surfer2 — PSLG + polygon-with-holes, weighted wavefronts, component modes, kinetic propagation, in-memory DCEL/offsets, Fast/Robust kernels. No GraphML/OBJ/CGAL/Boost/C++ at runtime.

## Oracle

`/tmp/surfer2` cloned at the pinned rev. Bins:

- `/tmp/surfer2/build-double/cc/surfer` (`-DNT_USE_DOUBLE=ON`)
- `/tmp/surfer2/build-exact/cc/surfer`

Local `/tmp` patches only (not in the Rust repo): skip Qt GUI, CGAL 6 empty `CGAL_USE_FILE`, `std::get_if` for intersections. Built against Homebrew CGAL 6 + Boost 1.92.

`KineticTriangulation.cpp` lives at `/tmp/surfer2/surf/src/KineticTriangulation.cpp`.

## What landed this session (unbounded hull, convex only)

Do not re-read the whole C++ tree for work already landed.

Prior commits (still current):

| SHA | What |
|---|---|
| `68c34bc` | Split-event fan walk; one kinetic vertex per wavefront sector; degree-1 bevels |
| `575e0c5` | Lock convex03/04/07 = 3/5/11; spoke-1 / deg1 smoke |
| `81f51a0` | Successor `pos_zero` from line intersection |
| `b25b290` | Parallel opposite lines → `Opposing`; opposing-face handler; spoke-1 interior 16/16 |

This session:

- `attach_unbounded_hull` now emits one U per unique **point-set hull** edge (constrained and unconstrained). Unconstrained hull: neighbor finite ↔ U. Constrained hull: no neighbor (two wavefronts).
- Pair missing-neighbor sides only when **both unconstrained** (colinear CDT twins). Pairing two constrained sides glues interior/exterior across a PSLG edge → spoke-1 5 segs. That was the “sector leak,” not `step_around`.
- Unconstrained U inherit the finite triangle’s component; that component floods around the infinite ring (C++ `tag_components`).
- Spoke-1 AllSequential **attaches**. **22** kinetic segs (16 interior + 6 pocket). C++ OBJ 26 includes 4 face-cycle chords: `write_obj` skips the infinite vertex, so CH-corner rays sit next to the pocket in the cycle (`v0/v4→(96,608)`, `v5/v9→(320,608)`). Not real arcs. Do not emit them.

`C++ split_vertex` (360–416) is still **bevel-only**. Split *events* are `handle_split_event`.

## Current numbers (C++ double `--component=-1`)

| Case | Rust | C++ | Note |
|---|---|---|---|
| convex03/04/07/19 | 3/5/11/35 | same | locked; AllSequential now has unbounded faces (exterior is rays) |
| spoke-1 **interior** (`Index(0)`) | **16** | 16 | done |
| spoke-1 both (`AllSequential`) | **22** | 26 OBJ / 22 kinetic | 4 extra OBJ edges are skip-∞ face chords |
| spoke-2 / spoke-3 | **22** / **22** | 26 OBJ | leave-CH at `-0.0` was a flip loop; locked |
| iso-simple-3 | **13** | 13 | locked; old 19 was pre-attach |
| issue2-minimal1 | **24** | 25 remembered | 1 off; likely OBJ skip-∞. Interior 20 |
| lake9706 | **4181** / 10690 nodes | 4372 / 4344 | runs (~10s); 191 segs short, extra nodes |
| lake10623 | ignored | 31330 | do not run yet |

C++ spoke-1 OBJ: 16 interior + 6 pocket + 4 skip-∞ chords = 26.

## What was tried and failed (do not repeat blindly)

1. **Pairing any same-endpoint missing neighbors**, including two constrained sides: glues interior to exterior across 1-2 / 2-3. 5 segs. Only pair **unconstrained** twins.
2. **Constrained-CH unbounded only, no bail-out** (spoke-1 still attached 6 U's, fragmented ring): 18 segs (regression from 22). Dummy endpoints remain on incomplete exterior fans; extra constraint/opposing events eat segs.
3. Processing unbounded triangles first so interior overwrites shared corners: not the leak. The 5-seg failure was pairing constrained twins.

## Next implementation (already decided)

**1. lake9706 4181 → 4372**

Interior path (`from_polygon`, no U). Completes; short 191 segs, node count inflated (10690 vs 4344). Diagnose missing events / leftover dummy vertices. Do not start 10623.

**2. Do not do first:** Robust kernel wiring, lake 10623, benches, additive weights.

## Critical files

- `src/propagator.rs` — `attach_unbounded_hull`, `initialize_kinetic` (wavefronts over all triangles, skip orientation swap when opposite vertex is `∞`), `create_kinetic_vertices`, `handle_split_or_flip` (unbounded → `handle_ccw_leaves`), `rewrite_star_side`, `handle_inf_opposing`
- `src/triangulation/around_vertex.rs` — infinite-vertex guard; cycle stop in `most_around`
- `src/triangulation/kinetic.rs` — `set_vertex` `is_unbounded`; `compute_unbounded_fast`; `vertex_leaves_ch`; `generic_collapse_time_3`
- Tests: `tests/differential.rs` (`spoke1` =22, interior 16, convex 3/5/11/35, iso-simple-3 =13, issue2-minimal1 =24)

## Constraints (unchanged)

Single crate, GPL-3.0-or-later, `Point2` f64, FFI-friendly, deps: slotmap 1.1.1, spade 2.15.1, robust 1.2.0, algebraics 0.3.0. IEEE bits stay; no origin shift. `CollapseType` source order is priority. No triangle-ID tiebreak. DCEL from arc records. Additive weights omitted.

## Commands

```bash
cd /home/ats/Development/surfer2
cargo test
SURFER2_DOUBLE_BIN=/tmp/surfer2/build-double/cc/surfer \
SURFER2_EXACT_BIN=/tmp/surfer2/build-exact/cc/surfer \
  cargo test --test differential -- --ignored --nocapture
cargo test --test differential spoke1 convex -- --nocapture
```

Env: `SURFER2_TEST_DATA` default `/tmp/surfer2/test-data`; `SURFER2_GIS_DATA` default `fixtures/gis`.

## Suggested skills

- **ponytail** (full) — already required by user rule; keep the ladder; no extra modules.
- **commit** — if asked to commit; split by meaning; do not commit rustfmt-only `tests/support/*`.
- **tdd** — only if the user asks test-first; otherwise one gate test per logic change.
- Do **not** invoke design/execute-plan unless the user asks to re-plan the whole port.

## Voice for the next agent

User wants implementation. Next: lakes / rest of corpus. Spoke-1 22, iso-simple-3 13, issue2 24 locked.
