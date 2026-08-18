# Handoff: surfer2 Rust port (headless)

Date: 2026-08-18
Repo: `/home/ats/Development/surfer2` — git `master`
Upstream: `cgalab/surfer2` @ `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`
Ponytail **full** is on (user rule). Off only: `stop ponytail` / `normal mode`.

Plan file `local://surfer2-rust-port-plan.md` is **not on disk**. Do not invent it.

## Goal

Native Rust crate `surfer2` (GPL-3.0-or-later): headless computational semantics of Surfer2 — PSLG + polygon-with-holes, weighted wavefronts, component modes, kinetic propagation, in-memory DCEL/offsets, Fast/Robust kernels. No GraphML/OBJ/CGAL/Boost/C++ at runtime.

## Oracle

Oracle bins were **missing** at end of 2026-08-18 (only test-data under `/tmp/surfer2`). Rebuild if you need OBJ diffs. Test-data: `/tmp/surfer2-src/test-data` (symlink `/tmp/surfer2/test-data` if needed).

Bins when present:

- `/tmp/surfer2/build-double/cc/surfer` (`-DNT_USE_DOUBLE=ON`)
- `/tmp/surfer2/build-exact/cc/surfer`

Local `/tmp` patches only (not in the Rust repo): skip Qt GUI, CGAL 6 empty `CGAL_USE_FILE`, `std::get_if` for intersections. Built against Homebrew CGAL 6 + Boost 1.92.

`KineticTriangulation.cpp` lives at `/tmp/surfer2/surf/src/KineticTriangulation.cpp`.

## What already landed (do not re-read C++ for this)

| SHA | What |
|---|---|
| `68c34bc` … `b25b290` | split fan, sector KVs, bevels, opposing handler, convex locks |
| `68db52e` / `3706692` | unbounded hull for **constrained** CH (convex only) |
| `8730e7c` | U per **all** point-set hull edges; neighbor finite↔U only if unconstrained; pair **unconstrained** twins only |
| `0a3b81f` | lock spoke-1=22, iso-simple-3=13, issue2-minimal1=24; later spoke-2/3 =22 |
| `104b64c` | leave-CH: `tc > time_now` (not `>=`); degree-0 det → Never. Spoke-3 t=0 flip loop was `-0.0 >= 0` |

Hull attach does **not** affect lakes. Lakes use `from_polygon` / `interior_straight_skeleton` (interior component only, no U).

C++ OBJ 26 on spoke-1 = 22 kinetic + 4 skip-∞ face chords. Do not emit those.

`C++ split_vertex` is still **bevel-only**. Split *events* are `handle_split_event`.

Do not commit rustfmt-only `tests/support/{graph_match,graphml,oracle}.rs`.

## Current numbers (C++ double `--component=-1`)

| Case | Rust | C++ | Note |
|---|---|---|---|
| convex03/04/07/19 | 3/5/11/35 | same | locked; AllSequential now has unbounded faces (exterior is rays) |
| spoke-1 **interior** (`Index(0)`) | **16** | 16 | done |
| spoke-1 both (`AllSequential`) | **22** | 26 OBJ / 22 kinetic | 4 extra OBJ edges are skip-∞ face chords |
| spoke-2 / spoke-3 | **22** / **22** | 26 OBJ | leave-CH at `-0.0` was a flip loop; locked |
| iso-simple-3 | **13** | 13 | locked; old 19 was pre-attach |
| issue2-minimal1 | **24** | 25 remembered | 1 off; likely OBJ skip-∞. Interior 20 |
| lake9706 | **4181** segs / **10690** nodes / len **375880** | **4372** / **4344** / len **342587** | same file, both finish, **not the same skeleton** |
| lake10623 | ignored | 31330 | do not run yet |

C++ spoke-1 OBJ: 16 interior + 6 pocket + 4 skip-∞ chords = 26.

## What was tried and failed (do not repeat blindly)

1. **Pairing any same-endpoint missing neighbors**, including two constrained sides: glues interior to exterior across 1-2 / 2-3. 5 segs. Only pair **unconstrained** twins.
2. **Constrained-CH unbounded only, no bail-out** (spoke-1 still attached 6 U's, fragmented ring): 18 segs (regression from 22). Dummy endpoints remain on incomplete exterior fans; extra constraint/opposing events eat segs.
3. Processing unbounded triangles first so interior overwrites shared corners: not the leak. The 5-seg failure was pairing constrained twins.

## Next session (already decided): lake9706 match

Same fixture `fixtures/gis/lake9706.json`. 2144 verts, 29 holes. Rust `interior_straight_skeleton` / `from_polygon` Fast. Completes in ~10s (`cargo test --test gis lake9706_load_and_skeleton -- --ignored --nocapture`).

| | Rust | C++ |
|---|---|---|
| finite segs | 4181 | 4372 (**191 short**) |
| nodes | 10690 | 4344 (**~2.5× extra**) |
| total length | 375880 | 342587 (Rust *longer* despite fewer segs) |

Plain reading: Rust draws most of the interior ridge map but some ridges never form, and leftover corners stay alive (dummy/input KVs that never stop/merge). Extra length is those leftovers shooting the wrong way.

Gate is already in `tests/gis.rs` (ignored): accept when segs==4372, nodes==4344, `|len-342587.19566660019|<1`, cyclomatic 0. Do not un-ignore until those match.

This is **not** the unbounded-hull work. Do not add U faces on the interior path. Do not start 10623. Do not emit OBJ skip-∞ chords. Do not chase issue2’s +1 without a double bin.

**2. Do not do first:** Robust kernel, lake 10623, benches, additive weights.

## Critical files

- `src/propagator.rs` — **lake:** `from_polygon`, `create_kinetic_vertices`, event handlers, `finish` (leftover verts become rays). Hull: `attach_unbounded_hull` (not on lake path).
- `src/triangulation/around_vertex.rs` — infinite-vertex guard; cycle stop in `most_around`
- `src/triangulation/kinetic.rs` — `set_vertex` `is_unbounded`; `compute_unbounded_fast`; `vertex_leaves_ch`; `generic_collapse_time_3`
- Tests: `tests/differential.rs` (spoke-1/2/3 =22, interior 16, convex 3/5/11/35, iso-simple-3 =13, issue2 =24); `tests/gis.rs` `lake9706_load_and_skeleton` (ignored until 4372)

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
cargo test --test gis lake9706_load_and_skeleton -- --ignored --nocapture
```

Env: `SURFER2_TEST_DATA` default `/tmp/surfer2/test-data`; `SURFER2_GIS_DATA` default `fixtures/gis`.

## Suggested skills

- **ponytail** (full) — already required by user rule; keep the ladder; no extra modules.
- **commit** — if asked to commit; split by meaning; do not commit rustfmt-only `tests/support/*`.
- **tdd** — only if the user asks test-first; otherwise one gate test per logic change.
- Do **not** invoke design/execute-plan unless the user asks to re-plan the whole port.

## Voice for the next agent

Fresh session: make lake9706 match C++ (4181→4372 segs, 10690→4344 nodes). Interior path only. Same lake file; engine still incomplete — leftover vertices + missing ridges. User wants implementation, not a re-plan of the hull work.
