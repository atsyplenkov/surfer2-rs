# Handoff: surfer2 Rust port (headless)

Date: 2026-08-18
Repo: `/home/ats/Development/surfer2` — last commit `893df11` on `master`. **Working tree is dirty** (Robust deleted, lake10623 exact OBJ + gates, API no longer has `KernelMode`). Do not assume `23e9ce0`.
Upstream: `cgalab/surfer2` @ `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`
Ponytail **off** is the standing user rule for coding. This session often ran **off** (`/ponytail off`).

Plan file `local://surfer2-rust-port-plan.md` is **not on disk**. Do not invent it.

## Goal

Native Rust crate `surfer2` (GPL-3.0-or-later): headless computational semantics of Surfer2 — PSLG + polygon-with-holes, weighted wavefronts, component modes, kinetic propagation, in-memory DCEL/offsets. **One `f64` engine** (C++ `-DNT_USE_DOUBLE=ON` analogue). No GraphML/OBJ/CGAL/Boost/C++ at runtime.

Public API: `interior_straight_skeleton(&polygon)` — no kernel argument. `Options` is only `components`. `KernelMode` / Robust / `algebraics` / `src/numeric/{exact,interval}.rs` are **gone**. The `robust` crate stays (orientation predicates).

## Oracle

| Bin | Path | Notes |
|---|---|---|
| C++ **double** | `/tmp/surfer2/build-double/cc/surfer` | `-DNT_USE_DOUBLE=ON`. lake9706 interior OBJ. **Segfaults** on lake10623 in `create_remaining_skeleton_dcel` after `All done.` (event #17889). Empty OBJ. This crash is old (2026-07-20 Magadan session), not new. |
| C++ **exact** | `/tmp/surfer2/build-exact/cc/surfer` | `NT_USE_DOUBLE` off. lake10623 interior. ~12 s, 17889 events, writes OBJ. |

C++ **source** is `/tmp/surfer2-src`. `KineticTriangulation.cpp` → `/tmp/surfer2-src/surf/src/KineticTriangulation.cpp`. Test-data: `/tmp/surfer2-src/test-data`.

Local `/tmp` patches only (not in the Rust repo): skip Qt GUI (`subdirs (surf cc)`), CGAL 6 empty `CGAL_USE_FILE`, `std::get_if` for intersections, `os.precision(17)` in `SkeletonDCEL::write_obj`. Homebrew CGAL 6 + Boost 1.92. easyloggingpp via `git submodule update --init` in `/tmp/surfer2-src`. Rebuild both with `-DCMAKE_PREFIX_PATH=/home/linuxbrew/.linuxbrew -DCMAKE_BUILD_TYPE=Release -DDEBUG_OUTPUT=OFF`.

`--component=r` **is** `--component=1` (interior for these lakes). `--component=0`/`l` is exterior. `-1` is interior+exterior.

Frozen OBJs in-repo:

- `fixtures/gis/lake9706-cpp-double.obj` — 4372 segs, 4344 nodes, len 342587.19567 (`--component=1`).
- `fixtures/gis/lake10623-cpp-exact.obj` — **2.1 MB**, 31330 segs, 31130 unique xy, len 1007737.382887 (`--component=r`). Regenerated this session. Keep it.

Magadan recipe that produced the original 10623 exact skeleton: `/home/ats/Projects/magadan/extract_surfer2_10623.R` (`surfer <- "/tmp/surfer2/build-exact/cc/surfer"`). GPKG restored this session at `/home/ats/Projects/magadan/hydrography_10623_surfer2_skeleton.gpkg` (5.4 MB). GraphML/OBJ from that script are `tempfile()` — the frozen OBJ in this repo is the keep.

July Magadan used exact, **not** double. A later double `--component=1` on 10623 is a replay of a crash they already had.

## What already landed (do not re-read C++ for this)

| SHA | What |
|---|---|
| `68c34bc` … `b25b290` | split fan, sector KVs, bevels, opposing handler, convex locks |
| `68db52e` / `3706692` | unbounded hull for **constrained** CH (convex only) |
| `8730e7c` | U per **all** point-set hull edges; neighbor finite↔U only if unconstrained; pair **unconstrained** twins only |
| `0a3b81f` | lock spoke-1=22, iso-simple-3=13, issue2-minimal1=24; later spoke-2/3 =22 |
| `104b64c` | leave-CH: `tc > time_now` (not `>=`); degree-0 det → Never |
| `42800b8` | `raw_flip` writes `vertices[]` directly |
| `40fb804` | 3-const always `TriangleCollapse`; inverted 2-const with no future edge dies at now |
| `8081d75` | DCEL linear time |
| `5473f86` | `lake9706_matches_cpp_double_interior` |
| `bb26b52` | snap degenerate collapse endpoint onto partner (31.7 m ridge) |
| `9022586` | skip dummy finish rays |
| `5697966` | delete stitch + 5 mm grid; `spawn_child_to_neighbor`; chain-walk nodes |
| `740718a` | stop invariant (`seal_stops` + `fill_missing_next`) |
| `e04ee3f` | first triangulation linear time |
| `23e9ce0` | Robust kernel wired (now **deleted** in the dirty tree) |
| `893df11` | last commit: handoff after spawn / stop invariant / linear init / Robust |

Uncommitted this session (do not treat as landed until committed):

- Delete Robust / `KernelMode` / `algebraics` / `exact.rs` / `interval.rs`.
- `interior_straight_skeleton(&poly)` only.
- lake10623 live gates + frozen exact OBJ.
- `lake10623_matches_cpp_exact_interior` (99 % / 1 cm, 98 % / 1 mm, max ≤ 3 m).

Hull attach does **not** affect lakes. Lakes use `from_polygon` / `interior_straight_skeleton` (interior only, no U).

C++ OBJ 26 on spoke-1 = 22 kinetic + 4 skip-∞ face chords. Do not emit those.

`C++ split_vertex` is still **bevel-only**. Split *events* are `handle_split_event`.

Do not commit rustfmt-only `tests/support/{graph_match,graphml,oracle}.rs` (dirty since before this work).

## Current numbers

| Case | Rust | C++ | Note |
|---|---|---|---|
| convex03/04/07/19 | 3/5/11/35 | same | locked |
| spoke-1 interior / AllSequential | 16 / 22 | 16 / 26 OBJ | 4 skip-∞ chords |
| spoke-2 / spoke-3 | 22 / 22 | 26 OBJ | locked |
| iso-simple-3 | 13 | 13 | locked; **1-const collapse-now broke this — do not retry blindly** |
| issue2-minimal1 | 24 | 25 remembered | do not chase +1 without a double bin |
| lake9706 | **4372** / **4344** / **342587.671** | **4372** / **4344** / **342587.196** (double) | **gate green**; cyclomatic 0; graph max 0.13 m |
| lake10623 | **31327** / **31128** / **1007632.725** | **exact 31330** / **31130** / **1007737.383** | live; cyclomatic **−1**; 31113/31130 within 1 mm; **max 2.33 m** |

`lake9706_load_and_skeleton`, `lake9706_matches_cpp_double_interior`, `lake10623_load_and_skeleton`, `lake10623_matches_cpp_exact_interior` are live.

Release wall: lake9706 ~47–51 ms; lake10623 ~3.8 s. Debug lake10623 ~2 min.

## Honest algorithm status

1. **31.7 m ridge (`stop_collapse_pair`)** — keep.
2. **0.40 m ridge (`spawn_child_to_neighbor`)** — keep. Parent still dies ~0.05 s later than C++ on 9706 (`tparam ≈ -0.012` flip then later triangle-collapse). Do not hunt that 9706 flip unless asked.
3. **lake10623 3-ridge / 2-node gap** — **diagnosed, not fixed.** See next section.
4. Robust — deleted on purpose. C++ exact is a different binary, not a second Rust engine.

## lake10623 gap (read this before touching spawn or collapse)

The graphs already agree almost everywhere. 17 C++ nodes sit >1 mm from Rust; only **two** are real misses. The rest are 1–8 cm jitter (exact vs f64).

| C++ extra node | Dist | C++ time | What it is |
|---|---:|---:|---|
| `(554196.067, 6982052.692)` | **2.33 m** | **55.9355** | Degree-3 junction. All three neighbours exist in Rust. |
| `(550628.310, 7004760.297)` | **2.26 m** | **41.4672** | Degree-3 with a 2.26 m same-time stub to a Rust node. |
| pair at `y=7037202` | 5.4 cm | ~23.17 | Known 31.7 m + 0.40 m 9706 pattern. Leave it. |

C++ exact also has events at those times (log `/tmp/lake10623-exact.stderr` if still present): #14922/#14923 at 41.4672, #16531/#16532 at 55.9355 (two events each). Rust **does** take `SplitOrFlipRefine` then `handle_split_event` at both times (`tparam` 0.126 and 0.35 — **on the segment**, not a flip).

The missing C++ node is **not** the splitter. It is a **constraint endpoint** (`ep0` at 41.467, `ep1` at 55.935) that stays `time_stop = None` after our split. C++’s **second same-time event** stops that endpoint. We never queue that event.

After the 41.467 split (measured):

- leftover edge `nea` (`ep0`–new child, 2.22 m): collapse **Past**, because the split child velocity is **`(1.55, −252513)` m/s**.
- neighbour `na` (2-const): **Never**.
- neighbour `nb` (3-const): TriangleCollapse at 44.2, not now.

`intersection_velocity` only rejects `|vel| > 1e6`; 252 km/s slips through. The leftover 2-const never dies, so `ep0` walks until 49.623.

After the 55.935 split: leftover `neb` 29.18 m, child velocity `(0,0)`, `o ≈ 0` classified **Past** (our `o == 0` falls into the LEFT_TURN quotient). C++ exact treats `orientation == 0` as **collinear → NEVER** (nonzero distance).

`src/propagator.rs` `handle_split_event` / `make_split_successor` and `src/wavefront/edge.rs` `compute_edge_collapse_f64` / `src/wavefront/supporting_line.rs` `intersection_velocity` are the files.

**Intended next cut (not tried):** if a **just-created split child** is ill-conditioned (huge velocity or zero velocity next to a live endpoint a few metres away), treat **that leftover face only** as infinitely-fast / collapse-now — C++’s second event. Do **not** apply this to every 2-const Never on the lake.

Loop: `lake10623_matches_cpp_exact_interior` (want segs 31330, max ≪ 2.33 m) **and** `lake9706_*` **and** `iso_simple3_and_issue2_counts`. One guard on the new child, not a lake special case.

## What was tried and failed (do not repeat blindly)

Hull era (still valid):

1. Pairing any same-endpoint missing neighbors, including two constrained sides: glues interior to exterior. Only pair **unconstrained** twins.
2. Constrained-CH unbounded only, no bail-out: spoke-1 18 segs.
3. Processing unbounded triangles first: not the leak.

Lake era:

4. Per-event refine: `NonAdvancingLoop` at t≈52.
5. End-only refine with C++ wavefront-chain guards: zero flips.
6. 1-const “split/collapse at now” when hunt returns Never: `NonAdvancingLoop` + iso-simple-3 13→9.
7. 2-const Never → TriangleCollapse **always** (even positive area): iso-simple-3 broke; lake worse. Only kill when **signed area < 0**.
8. Degrade non-meeting 2-const TriangleCollapse to ConstraintCollapse of the short edge: lake length −230 m. Spawn the DCEL child; do not `part2` that edge.
9. Leave isolated triangle-collapse verts live: `NonAdvancingLoop` at t≈23.226.

This session (10623 hunt, all **reverted**, tree should match pre-hunt algorithm):

10. **Late `spawn_child_to_neighbor`** (ignore time, take closest in-gap neighbour): +9.06 m bogus child at the *late* parent. 31327→31328, **max still 2.33 m**. Wrong ridge.
11. **Cap `|vel| > 1e3`** in `intersection_velocity` (was 1e6): 9706 4372→4368, 10623 max **32 m**.
12. **`o <= 0` as collinear** in `compute_edge_collapse_f64` (C++-shaped): 9706 stayed green; leftover 2-const at 55.935 became TriangleCollapse-now; 10623 **31325 / max 3.29 m**. Too many faces died.

Do not re-apply 10–12 globally. The next change must be **local to the new split child**.

## Next session

**Do first if continuing 10623:** the leftover-edge-after-split guard above. Instrumentation prefix used this session was `[DEBUG-evt]` / `[DEBUG-a4f2]` — none should remain in the tree.

**Do not do first unless asked:** 9706 0.05 s south-reflex flip, U faces, skip-∞ chords, 1-const collapse-now, per-event refine, additive weights, revive Robust, global `o` / `|vel|` knobs, late spawn.

## Critical files

- `src/propagator.rs` — `handle_split_event`, `make_split_successor`, `spawn_child_to_neighbor`, `stop_collapse_pair`, `seal_stops` / `fill_missing_next`.
- `src/wavefront/edge.rs` — `compute_edge_collapse_f64` (`o < 0` → Past; `o == 0` currently falls through to LEFT_TURN quotient — C++ would call that collinear).
- `src/wavefront/supporting_line.rs` — `intersection_velocity` (`|vel| > 1e6` → treat as no intersection).
- `src/triangulation/kinetic.rs` — `compute_collapse` is the only (f64) path.
- `src/dcel.rs` — `EndpointUf` from `next_vertex`/`prev_vertex`.
- Tests: `tests/gis.rs` (9706 + 10623 + exact OBJ overlay); `tests/differential.rs`.

## Constraints

Single crate, GPL-3.0-or-later, `Point2` f64, FFI-friendly, deps: slotmap 1.1.1, spade 2.15.1, robust 1.2.0. No `algebraics`. IEEE bits stay; no origin shift. `CollapseType` source order is priority. No triangle-ID tiebreak. DCEL from arc records + chain identity. Additive weights omitted. No `KernelMode`.

## Commands

```bash
cd /home/ats/Development/surfer2
cargo test --release --test gis -- --nocapture
cargo test --release --test gis lake10623 -- --nocapture
cargo test --release --test differential -- --nocapture
# debug lake10623 is ~2 min; skip it in debug if you only need 9706
cargo test --offline -- --skip lake10623_load_and_skeleton --skip lake10623_matches_cpp_exact_interior
```

Env: `SURFER2_TEST_DATA` default `/tmp/surfer2/test-data`; `SURFER2_GIS_DATA` default `fixtures/gis`.
`SURFER2_DOUBLE_BIN` / `SURFER2_EXACT_BIN` default to the `/tmp` bins above.

Rebuild C++ if a bin is gone:

```bash
cmake -S /tmp/surfer2-src -B /tmp/surfer2/build-double \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH=/home/linuxbrew/.linuxbrew \
  -DNT_USE_DOUBLE=ON -DDEBUG_OUTPUT=OFF
cmake --build /tmp/surfer2/build-double --target surfer

cmake -S /tmp/surfer2-src -B /tmp/surfer2/build-exact \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH=/home/linuxbrew/.linuxbrew \
  -DNT_USE_DOUBLE=OFF -DDEBUG_OUTPUT=OFF
cmake --build /tmp/surfer2/build-exact --target surfer

# 10623 interior (same as --component=1):
/tmp/surfer2/build-exact/cc/surfer --component=r /tmp/lake10623.graphml /tmp/out.obj
```

`/tmp/lake10623.graphml` was built from `fixtures/gis/lake10623.json` (15365 verts after dropping closing ring vertices; 201 holes).

## Suggested skills

- **ponytail** (full) — standing user rule for coding unless they said off.
- **commit** — if asked; split by meaning (Robust deletion ≠ 10623 OBJ/tests). Do not commit rustfmt-only `tests/support/*`.
- **tdd** — only if asked; `lake10623_matches_cpp_exact_interior` is the lock for the gap.
- **diagnosing-bugs** — leftover-edge-after-split is the open bug. Do **not** invoke design/execute-plan unless asked to re-plan the whole port.

## Voice for the next agent

You are not stuck. Robust is gone; the product is one f64 engine. lake9706 is green. lake10623 Fast finishes and matches C++ exact on 31113/31130 nodes to 1 mm. The 2.33 m / 3-seg gap is **after a successful split**: the constraint endpoint stays live; C++ kills it with a second same-time event. Next change is a **local** infinitely-fast / collapse-now on the just-born split child when its velocity is degenerate — not spawn, not global `o`, not a 1e3 velocity cap. Re-run 9706 and iso-simple-3 after any such guard.
