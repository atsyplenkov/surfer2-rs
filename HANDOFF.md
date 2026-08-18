# Handoff: surfer2 Rust port (headless)

Date: 2026-08-18
Repo: `/home/ats/Development/surfer2` — git `master` @ `23e9ce0`
Upstream: `cgalab/surfer2` @ `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`
Ponytail **full** is the user rule; this session ran **lite**. Off only: `stop ponytail` / `normal mode`.

Plan file `local://surfer2-rust-port-plan.md` is **not on disk**. Do not invent it.

## Goal

Native Rust crate `surfer2` (GPL-3.0-or-later): headless computational semantics of Surfer2 — PSLG + polygon-with-holes, weighted wavefronts, component modes, kinetic propagation, in-memory DCEL/offsets, Fast/Robust kernels. No GraphML/OBJ/CGAL/Boost/C++ at runtime.

## Oracle

C++ double bin **is built**: `/tmp/surfer2/build-double/cc/surfer` (`-DNT_USE_DOUBLE=ON`, Release, `REFINE_TRIANGULATION=ON`). Exact bin still missing.

C++ **source** is `/tmp/surfer2-src` (not `/tmp/surfer2/surf`). `KineticTriangulation.cpp` → `/tmp/surfer2-src/surf/src/KineticTriangulation.cpp`. Test-data: `/tmp/surfer2-src/test-data` (symlink `/tmp/surfer2/test-data` if needed).

Local `/tmp` patches only (not in the Rust repo): skip Qt GUI (`subdirs (surf cc)`), CGAL 6 empty `CGAL_USE_FILE`, `std::get_if` for intersections, `os.precision(17)` in `SkeletonDCEL::write_obj` (default 6 digits rounds UTM to ~metres and fake-mismatches graphs). Homebrew CGAL 6 + Boost 1.92. easyloggingpp via `git submodule update --init` in `/tmp/surfer2-src`.

Frozen interior C++ double OBJ (high precision): `fixtures/gis/lake9706-cpp-double.obj` — 4372 finite segs, 4344 nodes, len 342587.19567. Regenerated with `--component=1` (interior). `--component=0` is exterior.

## What already landed (do not re-read C++ for this)

| SHA | What |
|---|---|
| `68c34bc` … `b25b290` | split fan, sector KVs, bevels, opposing handler, convex locks |
| `68db52e` / `3706692` | unbounded hull for **constrained** CH (convex only) |
| `8730e7c` | U per **all** point-set hull edges; neighbor finite↔U only if unconstrained; pair **unconstrained** twins only |
| `0a3b81f` | lock spoke-1=22, iso-simple-3=13, issue2-minimal1=24; later spoke-2/3 =22 |
| `104b64c` | leave-CH: `tc > time_now` (not `>=`); degree-0 det → Never. Spoke-3 t=0 flip loop was `-0.0 >= 0` |
| `42800b8` | `raw_flip` writes `vertices[]` directly — `set_vertex` rewrote wavefront endpoints on sides about to move |
| `40fb804` | 3-const always `TriangleCollapse` (C++ never leaves them Never); inverted 2-const with no future edge dies at now |
| `8081d75` | DCEL `ensure_node` / face cycles HashMaps — lake DCEL ~467ms → ~4.5ms |
| `5473f86` | `lake9706_matches_cpp_double_interior` vs frozen OBJ |
| `bb26b52` | snap degenerate collapse endpoint onto partner (31.7 m ridge; C++ `pos_stop` invariant) |
| `9022586` | skip dummy finish rays; skeleton-incident nodes |
| `93b1d7b` | docs: length closed, 0.40 m stub left |
| `5697966` | **delete** `stitch_orphan_stops` + 5 mm `node_grid`. Spawn short child when a reflex dies into a same-time split neighbour. DCEL nodes by `next_vertex`/`prev_vertex` union-find |
| `740718a` | C++ stop invariant: every finite stop has `next_vertex` on both sides (`seal_stops` + `fill_missing_next`). Debug assert `unlinked_stops() == 0` |
| `e04ee3f` | first triangulation linear time: O(n) neighbor map, skip second PSLG validate on `as_pslg`, `tid_to_ti` / interior keep-bit |
| `23e9ce0` | Robust kernel **wired** for bounded collapses: interval-filtered `ExactValue` (algebraics if interval straddles 0). Fast unchanged |

Hull attach does **not** affect lakes. Lakes use `from_polygon` / `interior_straight_skeleton` (interior component only, no U).

C++ OBJ 26 on spoke-1 = 22 kinetic + 4 skip-∞ face chords. Do not emit those.

`C++ split_vertex` is still **bevel-only**. Split *events* are `handle_split_event`.

Do not commit rustfmt-only `tests/support/{graph_match,graphml,oracle}.rs`.

## Current numbers

C++ lake interior is `--component=1` (not `-1`). `-1` is interior+exterior.

| Case | Rust | C++ | Note |
|---|---|---|---|
| convex03/04/07/19 | 3/5/11/35 | same | locked |
| spoke-1 interior / AllSequential | 16 / 22 | 16 / 26 OBJ | 4 skip-∞ chords |
| spoke-2 / spoke-3 | 22 / 22 | 26 OBJ | locked |
| iso-simple-3 | 13 | 13 | locked; **1-const collapse-now broke this — do not retry blindly** |
| issue2-minimal1 | 24 | 25 remembered | do not chase +1 without a double bin |
| lake9706 Fast | **4372** segs / **4344** nodes / len **342587.671** | **4372** / **4344** / **342587.196** | **gate green**; cyclomatic 0; graph max 0.13 m |
| lake9706 Robust | not run | — | do not start unless asked |
| lake10623 | ignored | 31330 | do not run yet |

Both `lake9706_load_and_skeleton` and `lake9706_matches_cpp_double_interior` are live (Fast). `robust_matches_fast_convex03` and unit-square Robust lock Fast≡Robust on those small cases.

Speed (same machine, release): C++ e2e GraphML+OBJ logs off **~61 ms** median. Rust lake criterion `lake9706_fast` **~47 ms** median after `e04ee3f` (−88% vs stale ~409 ms bench). Split init (one run): `as_pslg` ~8 µs, CDT+neighbors **~2.7 ms**, kinetic setup **~1.4 ms**, seed ~0.2 ms. Event loop is not the bottleneck.

## Honest algorithm status

Kinetic loop is the same idea as C++ and almost the same events (2482 vs 2470). Closers:

1. **31.7 m ridge (`stop_collapse_pair`)** — real. C++ `pos_stop` equal. If one end barely moved (≤1 mm), snap onto the partner. Keep.

2. **0.40 m ridge (`spawn_child_to_neighbor`)** — real child vertex, not a finish-time stitch. Parent dies later than C++ (~0.05 s): we still flip at `tparam ≈ -0.012` then spawn at the later triangle-collapse. Do **not** hunt that earlier split/flip unless asked (reclassify ate ~230 m of ridges once).

3. **4344 nodes** — chain-walk identity + stop invariant (`JUNCTION_M` 5 mm is only “same collapse” for linking, not a grid on all endpoints). `node_count` walks UF roots on finite segments.

4. **Robust** — bounded triangles only (`compute_collapse_robust`). Unbounded hull and infinitely-fast faces still call Fast. Default `Options.kernel` is Robust; locked lake/spoke tests pass **Fast** explicitly.

## What was tried and failed (do not repeat blindly)

Hull era (still valid):

1. **Pairing any same-endpoint missing neighbors**, including two constrained sides: glues interior to exterior. Only pair **unconstrained** twins.
2. **Constrained-CH unbounded only, no bail-out**: spoke-1 18 segs (regression from 22).
3. Processing unbounded triangles first so interior overwrites shared corners: not the leak.

Lake era:

4. **Per-event refine** (C++ `REFINE_TRIANGULATION` after every event): same-time flip loop → `NonAdvancingLoop` at t≈52. Do not scan-all-and-flip after each event.
5. **End-only refine with C++ wavefront-chain guards**: leftover 1-reflex faces failed those guards; zero flips; no change.
6. **1-const “split/collapse at now” when hunt returns Never**: `NonAdvancingLoop` + iso-simple-3 13→9. Reverted.
7. **2-const Never → TriangleCollapse always** (even positive area): iso-simple-3 broke; lake segs/len got worse. Only kill when **signed area < 0**.
8. **Degrade non-meeting 2-const TriangleCollapse to ConstraintCollapse of the short edge**: rewired the just-born split child; lake length −230 m. Reverted. Spawn the DCEL child; do not `part2` that edge.
9. **Leave isolated triangle-collapse verts live** (do not stop the far reflex): leftover 2-const re-queued → `NonAdvancingLoop` at t≈23.226. Reverted.

## Next session

Lake9706 Fast interior gate is green. Init is no longer the 83 ms hole.

**Do not do first unless asked:** lake 10623, lake-on-Robust, chase the 0.05 s earlier south-reflex flip, U faces, skip-∞ chords, 1-const collapse-now, per-event refine, additive weights.

Reasonable next cuts if asked: run Robust on lake9706 (expect slower; compare graph), or leave it.

## Critical files

- `src/propagator.rs` — `stop_collapse_pair`; `spawn_child_to_neighbor`; `set_next_vertex` / `seal_stops` / `fill_missing_next`; dummy unused input verts skip rays. Hull: `attach_unbounded_hull` (not on lake path). `seed_queue` / `recompute_collapse` take `KernelMode`.
- `src/dcel.rs` — `EndpointUf` from `next_vertex`/`prev_vertex`; no `node_grid`.
- `src/triangulation/kinetic.rs` — `compute_collapse` dispatches Fast vs Robust; `compute_collapse_robust` + exact constraint times / det roots.
- `src/triangulation/basic.rs` — O(n) directed-edge neighbor map.
- `src/numeric/` — `ExactValue`, `Interval`, `determinant_poly_exact`.
- Tests: `tests/differential.rs` locks + `robust_matches_fast_convex03`; `tests/gis.rs` both lake9706 tests Fast.

## Constraints (unchanged)

Single crate, GPL-3.0-or-later, `Point2` f64, FFI-friendly, deps: slotmap 1.1.1, spade 2.15.1, robust 1.2.0, algebraics 0.3.0. IEEE bits stay; no origin shift. `CollapseType` source order is priority. No triangle-ID tiebreak. DCEL from arc records + chain identity. Additive weights omitted.

## Commands

```bash
cd /home/ats/Development/surfer2
cargo test
cargo test --release --test gis -- --nocapture
cargo test --release --test gis lake9706_load_and_skeleton -- --nocapture
cargo test --release --test differential -- --nocapture
SURFER2_DOUBLE_BIN=/tmp/surfer2/build-double/cc/surfer \
  cargo test --test differential -- --ignored --nocapture
```

Env: `SURFER2_TEST_DATA` default `/tmp/surfer2/test-data`; `SURFER2_GIS_DATA` default `fixtures/gis`.

Rebuild C++ double if the bin is gone:

```bash
# in /tmp/surfer2-src: submodule init, skip gui, empty CGAL_USE_FILE, std::get_if, precision(17)
cmake -S /tmp/surfer2-src -B /tmp/surfer2/build-double \
  -DCMAKE_BUILD_TYPE=Release -DNT_USE_DOUBLE=ON -DDEBUG_OUTPUT=OFF
cmake --build /tmp/surfer2/build-double --target surfer
# interior: --component=1
```

## Suggested skills

- **ponytail** (full) — user rule; keep the ladder; no extra modules.
- **commit** — if asked; split by meaning; do not commit rustfmt-only `tests/support/*`.
- **tdd** — only if the user asks test-first; the lake graph test is already the lock.
- Do **not** invoke design/execute-plan unless the user asks to re-plan the whole port.

## Voice for the next agent

Lake9706 Fast interior gate is green (4372 / 4344 / cyclomatic 0). Stitch and 5 mm grid are gone; child is spawned; nodes are chain-walk; stop invariant holds. Init is ~4 ms, e2e ~47 ms. Robust is live on bounded collapses only. Do not start 10623 or lake-Robust unless asked. Do not hunt the 0.05 s earlier reflex stop unless asked.
