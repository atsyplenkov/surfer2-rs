# Handoff: surfer2 Rust port (headless)

Date: 2026-08-18
Repo: `/home/ats/Development/surfer2` — git `master` @ `5473f86`
Upstream: `cgalab/surfer2` @ `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`
Ponytail **full** is the user rule. This session ran **ultra**. Off only: `stop ponytail` / `normal mode`.

Plan file `local://surfer2-rust-port-plan.md` is **not on disk**. Do not invent it.

## Goal

Native Rust crate `surfer2` (GPL-3.0-or-later): headless computational semantics of Surfer2 — PSLG + polygon-with-holes, weighted wavefronts, component modes, kinetic propagation, in-memory DCEL/offsets, Fast/Robust kernels. No GraphML/OBJ/CGAL/Boost/C++ at runtime.

## Oracle

C++ double bin **is built** (this session): `/tmp/surfer2/build-double/cc/surfer` (`-DNT_USE_DOUBLE=ON`, Release, `REFINE_TRIANGULATION=ON`). Exact bin still missing.

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
| `8081d75` | DCEL `ensure_node` + `connect_face_cycles` are HashMaps — lake DCEL ~467ms → ~4.5ms |
| `5473f86` | `lake9706_matches_cpp_double_interior` vs frozen OBJ |

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
| lake9706 | **4371** segs / **4352** DCEL nodes / len **342587.272** | **4372** / **4344** / **342587.196** | **same ridge map**; 1 seg; length closed |
| lake10623 | ignored | 31330 | do not run yet |

Lake graph (test `lake9706_matches_cpp_double_interior`, not ignored): 4329/4344 C++ nodes within 1 mm of a Rust endpoint, 4338/4344 within 1 cm, max 13 cm. DCEL `node_count` is now skeleton-incident nodes on a 1 mm grid (10921 → 4352). Dummy unused input verts no longer emit finish rays.

Speed (release, 5-run median, same lake): C++ e2e GraphML+OBJ **53 ms**; Rust engine **89 ms** (init 83 + events 1.6 + DCEL 4.5). Event loop is not the bottleneck.

## What was tried and failed (do not repeat blindly)

Hull era (still valid):

1. **Pairing any same-endpoint missing neighbors**, including two constrained sides: glues interior to exterior. Only pair **unconstrained** twins.
2. **Constrained-CH unbounded only, no bail-out**: spoke-1 18 segs (regression from 22).
3. Processing unbounded triangles first so interior overwrites shared corners: not the leak.

Lake era (this session):

4. **Per-event refine** (C++ `REFINE_TRIANGULATION` after every event): same-time flip loop → `NonAdvancingLoop` at t≈52. Do not scan-all-and-flip after each event.
5. **End-only refine with C++ wavefront-chain guards**: leftover 1-reflex faces failed those guards; zero flips; no change.
6. **1-const “split/collapse at now” when hunt returns Never**: `NonAdvancingLoop` + iso-simple-3 13→9. Reverted.
7. **2-const Never → TriangleCollapse always** (even positive area): iso-simple-3 broke; lake segs/len got worse. Only kill when **signed area < 0**.

## Next session: last 1 lake ridge + 8 extra nodes

Interior path only. Same `fixtures/gis/lake9706.json`. Gate stays ignored until segs==4372, nodes==4344, `|len-342587.19566660019|<1`, cyclomatic 0.

| | Rust | C++ | Gap |
|---|---|---|---|
| finite segs | 4371 | 4372 | **1 short** (0.40 m stub at 539552.45–539552.84) |
| DCEL `node_count` | 4352 | 4344 | 1 mm grid + skeleton-incident only |
| length | 342587.272 | 342587.196 | **0.076 m** (closed) |
| events | 2482 | 2470 | close |

This session: leftover 1-const Never was a miss (live=0). The 31.7 m gap was a **newborn that died in place** 32 m from its collapse partner (C++ `va.pos_stop == vb.pos_stop`). `stop_collapse_pair` now jumps a ≤1 mm-degenerate endpoint onto the partner. `pos_start` is the DCEL start (C++). Dummy unused input verts skip finish rays. `node_count` ignores input-only vertices.

The remaining 0.40 m is C++ S1↔S0 at t=23.173 vs Rust split of 3467 at t=23.226 at S0 — 3470 has `next=[None,None]`. Do **not** add U faces. Do **not** start 10623. Do **not** emit skip-∞ chords. Do **not** revive 1-const collapse-now.

Likely next lever: give 3470 a successor into 4925 (0.40 m) when the south reflex dies without `next_vertex`. Then walk kinetic chains if 8 extra nodes still matter.

**Do not do first:** Robust kernel, lake 10623, benches, additive weights, per-event refine.

## Critical files

- `src/propagator.rs` — `from_polygon`, `create_kinetic_vertices`, `raw_flip` (no `set_vertex`), event handlers, `finish` (dummy leftover verts → rays). Hull: `attach_unbounded_hull` (not on lake path).
- `src/triangulation/kinetic.rs` — 3-const always triangle-collapse; inverted 2-const; `signed_area_at`
- `src/dcel.rs` — HashMap `ensure_node` (1 mm xy, no time); `node_count` is skeleton-incident only.
- Tests: `tests/differential.rs` (spoke/convex/iso/issue2 locks); `tests/gis.rs` `lake9706_matches_cpp_double_interior` (runs in CI); `lake9706_load_and_skeleton` (ignored until exact counts)

## Constraints (unchanged)

Single crate, GPL-3.0-or-later, `Point2` f64, FFI-friendly, deps: slotmap 1.1.1, spade 2.15.1, robust 1.2.0, algebraics 0.3.0. IEEE bits stay; no origin shift. `CollapseType` source order is priority. No triangle-ID tiebreak. DCEL from arc records. Additive weights omitted.

## Commands

```bash
cd /home/ats/Development/surfer2
cargo test
cargo test --release --test gis -- --nocapture
cargo test --release --test gis lake9706_load_and_skeleton -- --ignored --nocapture
cargo test --test differential spoke1 convex iso -- --nocapture
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

Fresh session: last 0.40 m stub (4371→4372) and 4352→4344 nodes. Length is done. Interior only. Graph test is green. Do not re-plan hull or refine-after-every-event.
