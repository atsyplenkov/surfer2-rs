# Handoff: surfer2 Rust port (headless)

Date: 2026-08-18
Repo: `/home/ats/Development/surfer2` — last commit `d4ba934` on `master`. Working tree holds the leftover-policy / matcher review fixes. Do not commit rustfmt-only `tests/support/{graphml,oracle}.rs` if they reappear.
Upstream: `cgalab/surfer2` @ `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`

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
- `fixtures/gis/lake10623-cpp-exact.obj` — **2.1 MB**, 31330 segs, 31130 unique xy, len 1007737.382887 (`--component=r`). Keep it.

Magadan recipe that produced the original 10623 exact skeleton: `/home/ats/Projects/magadan/extract_surfer2_10623.R` (`surfer <- "/tmp/surfer2/build-exact/cc/surfer"`). GPKG at `/home/ats/Projects/magadan/hydrography_10623_surfer2_skeleton.gpkg` (5.4 MB). GraphML/OBJ from that script are `tempfile()` — the frozen OBJ in this repo is the keep.

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
| `23e9ce0` | Robust kernel wired (later deleted) |
| `893df11` | handoff after spawn / stop invariant / linear init / Robust |
| `21f26f5` | delete Robust / `KernelMode` / `algebraics` / `exact.rs` / `interval.rs`. `interior_straight_skeleton(&poly)` only. |
| `ede6169` | lake10623 live gates + frozen exact OBJ |
| `348e53c` | handoff after Robust deletion and the leftover-edge diagnosis |
| `9463c01` | leftover-after-split guard; lake10623 31329 / 31130 / max ≤ 0.15 m |

Hull attach does **not** affect lakes. Lakes use `from_polygon` / `interior_straight_skeleton` (interior only, no U).

C++ OBJ 26 on spoke-1 = 22 kinetic + 4 skip-∞ face chords. Do not emit those.

`C++ split_vertex` is still **bevel-only**. Split *events* are `handle_split_event`.

`tests/support/graph_match.rs` is a real matcher (bipartite node correspondence + bidirectional edge multisets). Do not commit rustfmt-only `tests/support/{graphml,oracle}.rs`.

## Current numbers

| Case | Rust | C++ | Note |
|---|---|---|---|
| convex03/04/07/19 | 3/5/11/35 | same | locked |
| spoke-1 interior / AllSequential | 16 / 22 | 16 / 26 OBJ | 4 skip-∞ chords |
| spoke-2 / spoke-3 | 22 / 22 | 26 OBJ | locked |
| iso-simple-3 | 13 | 13 | locked; **1-const collapse-now broke this — do not retry blindly** |
| issue2-minimal1 | 24 | 25 remembered | do not chase +1 without a double bin |
| lake9706 | **4372** / **4344** / **342587.671** | **4372** / **4344** / **342587.196** (double) | cyclomatic 0; bidirectional max 0.13 m; **topology only_a=1 only_b=1** (south-reflex flip) |
| lake10623 | **31329** / **31130** / **1007709.386** | **exact 31330** / **31130** / **1007737.383** | cyclomatic **−1**; nodes match both ways; 31111/31130 within 1 mm; **max 0.080 m** |

Live gates: `lake9706_load_and_skeleton`, `lake9706_cpp_double_node_progress` (asserts the 1-ridge delta), `lake10623_load_and_skeleton` (one skeleton; bidirectional node proximity).

Ignored exact-conformance gates (require matching edge multisets): `lake9706_matches_cpp_double_interior`, `lake10623_matches_cpp_exact_interior`.

Release wall: lake9706 ~50 ms; lake10623 ~4–7 s **once** (the old pair of overlays is gone). Full `cargo test --release --test gis` ~6–7 s. Debug lake10623 ~2 min.

## Honest algorithm status

1. **31.7 m ridge (`stop_collapse_pair`)** — keep.
2. **0.40 m ridge (`spawn_child_to_neighbor`)** — keep. Parent still dies ~0.05 s later than C++ on 9706 (`tparam ≈ -0.012` flip then later triangle-collapse). Do not hunt that 9706 flip unless asked.
3. **lake10623 leftover endpoints** — **fixed this session.** Both C++ extra nodes (2.33 m / 2.26 m) now exist. Graph max 2.33 m → **0.079 m**.
4. **lake10623 1-ridge / 28 m length gap** — **still open.** Nodes match; segs 31329 vs 31330; len 1007709 vs 1007737. Cyclomatic still −1. Residual max is centimetre-scale (exact vs f64 + the known 5.4 cm pair at `y=7037202`).
5. Robust — deleted on purpose. C++ exact is a different binary, not a second Rust engine.

## Leftover-after-split guard (`9463c01`)

C++ exact has two same-time events at 41.4672 (#14922/#14923) and 55.9355 (#16531/#16532). Rust takes `SplitOrFlipRefine` then `handle_split_event` at both (`tparam` 0.126 and 0.35 — on the segment). The missing C++ node was **not** the splitter: it was the leftover constraint endpoint (`ep0` / `ep1`) that stayed `time_stop = None`. C++'s second event stops that endpoint.

Measured leftovers:

| t | Child vel | Leftover | nconst | Natural collapse | Guard |
|---|---|---:|---:|---|---|
| 41.4672 | `(1.55, −252513)` | 2.22 m | 2 | **Never** | force ConstraintCollapse-now |
| 55.9355 | `(0,0)` | 29.18 m | 2 | ConstraintCollapse of the *other* edge | force because child speed is 0 |

Successor motion is typed in `src/wavefront/split_leftover.rs` (`pub(crate)` — not part of the public crate API). Classification is **geometry only** (no `wavefront_vertex_motion`, no `1e6` cap):

- `Finite` — `|sin θ| ≥ 1e-4`, or exact same-direction parallels. Weights do not change this.
- `NonIntersecting` — opposing lines with no usable intersection: exact parallel, non-finite meet, or `|sin θ| < 1e-6` (the 55.935 leftover). Angle test, not a speed cap.
- `Degenerate` — poorly-conditioned (`1e-6 ≤ |sin θ| < 1e-4`) with a finite intersection (the 41.467 leftover). Same class at weight 1 and 100.

Do **not** classify through `intersection_velocity` or `wavefront_vertex_motion`. Those still have the `1e6` cap for actual vertex creation only.

`leftover_schedule(nconst, motion)` is the production decision:

- 3-const / 2-const `Finite` → `LeaveNatural`
- 2-const `NonIntersecting` → `CollapseNow`
- 2-const `Degenerate` → `ComputeNatural`, then `leftover_action` collapses only if natural is `Never`
- 1-const / 0-const → `Unsupported` (treated as leave)

`schedule_leftover_collapse` validates face + child IDs first, then routes **every** case through `leftover_schedule`. On collapse-now it parks the child at the split (`pos_zero = pos`, `vel = 0`) so `stop_collapse_pair` snaps it onto the live endpoint, and caches `ConstraintCollapse` of the leftover edge at `time` on **that face only**.

Do **not** raise this into `intersection_velocity` or `compute_edge_collapse_f64`. Those global knobs already failed (see below). Do **not** bring back `SPLIT_CHILD_MAX_SPEED`.

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

Previous 10623 hunt (reverted before this session):

10. **Late `spawn_child_to_neighbor`** (ignore time, take closest in-gap neighbour): +9.06 m bogus child at the *late* parent. 31327→31328, **max still 2.33 m**. Wrong ridge.
11. **Cap `|vel| > 1e3`** in `intersection_velocity` (was 1e6): 9706 4372→4368, 10623 max **32 m**.
12. **`o <= 0` as collinear** in `compute_edge_collapse_f64` (C++-shaped): 9706 stayed green; leftover 2-const at 55.935 became TriangleCollapse-now; 10623 **31325 / max 3.29 m**. Too many faces died.

This session (keep the last one; the earlier ones were tightened, not reverted):

13. **Force leftover collapse on every ill split child**, including 3-const: 9706 4372→4371 (one 3-const leftover at t=38.907, leftover 6.7 m, speed 118 km/s).
14. **Force every 2-const ill leftover** (ignore natural collapse): 10623 31329 / max **20 m**. Seven earlier leftovers (t=9.8…30.3) still had a future collapse on the other constraint.
15. **Never-only leftover force**: closed 41.467, skipped 55.935 (natural was ConstraintCollapse of the other edge). 31328 / 31129 / max still 2.33 m.
16. **Never, or child speed == 0**: both leftovers fire. 31329 / 31130 / max 0.08 m. Kept as the typed policy: 2-const `NonIntersecting` always, 2-const `Degenerate` only when natural is `Never`.

## Next session

**Do first if continuing 10623:** the remaining **1-ridge / ~28 m length** gap (31329 vs 31330). Nodes already match. Do not widen the leftover guard — forcing extra 2-const leftovers raised max to 20 m. Look at DCEL emission after the two new leftover ConstraintCollapses: each new junction is degree-3 in C++; we may be missing one spoke off a snapped child (`spawn_child_to_neighbor` / `fill_missing_next` / the 55.935 leftover is 29 m, C++ length delta is 28 m).

**Do not do first unless asked:** 9706 0.05 s south-reflex flip, U faces, skip-∞ chords, 1-const collapse-now, per-event refine, additive weights, revive Robust, global `o` / `|vel|` knobs, late spawn, force leftover on 3-const, force leftover whenever the child is merely fast.

## Critical files

- `src/propagator.rs` — `handle_split_event`, `make_split_successor`, `schedule_leftover_collapse`, `spawn_child_to_neighbor`, `stop_collapse_pair`, `seal_stops` / `fill_missing_next`.
- `src/wavefront/split_leftover.rs` — `SuccessorMotion`, `classify_successor_motion`, `leftover_schedule`, `leftover_action` (`pub(crate)`).
- `src/wavefront/edge.rs` — `compute_edge_collapse_f64` (`o < 0` → Past; `o == 0` currently falls through to LEFT_TURN quotient — C++ would call that collinear). Do not flip this globally.
- `src/wavefront/supporting_line.rs` — `intersection_velocity` (`|vel| > 1e6` → treat as no intersection). Do not lower the cap.
- `src/triangulation/kinetic.rs` — `compute_collapse` is the only (f64) path.
- `src/dcel.rs` — `EndpointUf` from `next_vertex`/`prev_vertex`.
- Tests: `tests/gis.rs` (9706 + 10623 progress gates; ignored exact topology); `tests/support/graph_match.rs`; `tests/differential.rs`.

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

- **commit** — if asked; split by meaning (leftover guard ≠ rustfmt-only `tests/support/*`). Do not commit rustfmt-only `tests/support/*`.
- **tdd** — only if asked; `lake10623_load_and_skeleton` is the live lock (max ≤ 0.15 m; nodes 31130). `lake10623_matches_cpp_exact_interior` is the ignored exact-conformance gate (31330 segs, cyclomatic 0, edge multisets).
- **diagnosing-bugs** — remaining 1-ridge gap. Do **not** invoke design/execute-plan unless asked to re-plan the whole port.

## Voice for the next agent

You are not stuck. Robust is gone; the product is one f64 engine. lake9706 counts are green; the matcher locks a **1-ridge** topology delta against C++ double (do not hunt the south-reflex flip). lake10623 Fast finishes with **the same 31130 unique-xy nodes as C++ exact**; graph max is 8 cm, not 2.33 m. The leftover-after-split guard is typed successor motion (`Finite` / `NonIntersecting` / `Degenerate`) applied only to two-constraint leftovers. One ridge / 28 m of length is still missing. Next change is DCEL / spoke emission after those two leftover ConstraintCollapses — not a wider leftover force, not spawn-late, not global `o` / `|vel|`. Re-run 9706 and iso-simple-3 after any such change.
