# Projected-basin evidence for issue 6

This directory records four source revisions and one replay of their public basin tests, including a throwaway Arm A harness on the control commit and on the dirty Arm B engine. It does not say why a basin fails, and it does not recommend a patch.

The registry is `manifest.json`. The patches are under `diffs/`. `replay.sh` rebuilds each checkout and writes the observation rows back into the manifest. Logs from the run below are under `logs/`.

## Run the comparison again

From any working directory, run `evidence/issue-6/replay.sh`.

The script checks the bundle sha256, checks out each `bundle_ref` under `work/`, and builds only when that commit is the one recorded in `source_bundle.refs`. A mismatch exits 1 and does not open the log files, so the `observations` array and the logs it cites stay as they were. `REPLAY_CHECK_LIVE=1` also compares each provenance checkout's `HEAD` and captured diff, and the same refusal applies. Without that setting, a moved or missing provenance checkout does not stop the run. The script does not stage, commit, or edit the provenance checkouts. A second run that still matches overwrites `logs/` and the `observations` array.

Each test uses one command of this shape, with `--release` only for the release profile.

```text
cd <checkout> && CARGO_TERM_COLOR=never cargo test --release --test projected_polygons --locked -- <test> --exact --test-threads=1 --nocapture
```

A single test stops at 600 seconds. That ceiling is `CASE_TIMEOUT_S`. Raise it and run the script again if you need a result that hit the ceiling. None of the tests below hit it.

## Reconstruct the sources

`sources/revisions.bundle` is a complete git bundle, sha256 `6a1d007a1b65c0220126846c55c91a5db62f3a819a7607eebe734ebc853957cd`. A fresh clone does not need the original worktrees.

```text
git clone evidence/issue-6/sources/revisions.bundle reconstructed
git -C reconstructed switch --detach origin/evidence/control-harness
```

The refs are `origin/evidence/control`, `origin/evidence/arm-a`, `origin/evidence/arm-b-base`, `origin/evidence/arm-b-dirty`, `origin/evidence/continuation-dirty`, `origin/evidence/control-harness`, and `origin/evidence/arm-b-arm-a-harness`. `replay.sh` checks those refs out under `work/` and runs there. `sources/build-bundle.sh` is the command that read the original checkouts. It does not edit them.

`evidence/control-harness` is `d1a89fd` plus Arm A's `tests/projected_polygons.rs` and `fixtures/projected`. `evidence/arm-b-arm-a-harness` is the dirty Arm B engine with that same harness. `evidence/arm-b-dirty` keeps Arm B's own tests, including the `assert_dcel_contract` call in the reduced-ridge test.

Copied reports are under `assets/`. The manifest records each file's original path.

`assets/basin2.txt` is the debug log that was at `/tmp/basin2.txt`, sha256 `17d973ace94bafbe3a21eb406235b8c280ed4589d23c3b91e44618a4035efdd8`. It is `projected_basin_2_is_one_tree` panicking at `expect` on line 244. The `IncompleteFaceCycle` detail lists the ten halfedges of face 600, including `next_ok` and `prev_ok`. The compile line names the Arm A worktree. No preserved commit contains `next_ok`. The instrumented source revision is unknown.

## Revisions

| id | commit | dirty | checkout |
| --- | --- | --- | --- |
| `control` | `d1a89fd66795f537b034f220ed4f838bd0fdea3b` | no | `/home/ats/Development/surfer2-rs` on `master` |
| `arm_a` | `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` | no | `/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-28189dfa59ae` on `fix/issue-1-arm-a` |
| `arm_b_dirty` | `7d3faf62ec7bd15f46365053922de2885dac87ca` | dirty against that commit | `/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-282f7bdcc9ba` on `fix/issue-1-arm-b` |
| `continuation_dirty` | `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` | dirty against that commit | `/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d970-f5c4-7cc3-8bfd-7ec922c43176` on `fix/issue-1-continue` |

Captured diff sha256, from `sha256sum` on the files in `diffs/`. An empty diff is the empty-file hash, not a missing file.

| file | sha256 |
| --- | --- |
| `diffs/control.diff` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `diffs/arm-a.diff` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `diffs/arm-b.diff` | `1800a626b8b8f786702d00f81e4f4fd2b56ee983c3cb0a78db44ccbf3882f7f0` |
| `diffs/continuation.diff` | `3df8d22eb16ac6807f04448c3386384b48e93d505e778cc4324bf6ed9c2f079d` |

`control.diff` and `arm-a.diff` are zero bytes. They are `git diff HEAD` followed by `git diff --cached`, both empty. Untracked files in those checkouts are listed in the manifest under `excluded_untracked` and are not part of the revision.

`arm-b.diff` and `continuation.diff` are `git diff HEAD`, then `git diff --cached`, then `git diff --no-index` for untracked paths under `src/`, `tests/`, `docs/`, and `fixtures/`. Nothing was staged. `git diff --cached` was empty for both. Continuation had no untracked experiment files. Its tracked diff is 178 insertions and 28 deletions.

Test file sha256 (`sha256sum` of the bytes that belong to the revision).

| revision | sha256 of `tests/projected_polygons.rs` |
| --- | --- |
| control | absent from `d1a89fd`. The run uses `evidence/control-harness`, which adds Arm A's file, sha256 `b15af0682850b71cd288f05c02a5518f7d3c629b30371dafc0d2b938f16c56a5`. |
| `arm_a` | `b15af0682850b71cd288f05c02a5518f7d3c629b30371dafc0d2b938f16c56a5` |
| `arm_b_dirty` | `802b45c4a14698f7506a8b76b25c8723d368bd15c1d93d37f0bbbb1789a08cf0` (dirty file). The file at `HEAD` hashes to `7ff91b7a24384e08e0ab6ae448c952ececb8b38dff52ac7ed20c7634d66165ed` and does not define the full-basin or river tests. Its reduced-ridge test calls `assert_dcel_contract`. |
| `arm_b_arm_a_harness` | `b15af0682850b71cd288f05c02a5518f7d3c629b30371dafc0d2b938f16c56a5`, Arm A's file on the dirty Arm B engine. That reduced-ridge test does not call `assert_dcel_contract`. |
| `continuation_dirty` | `b15af0682850b71cd288f05c02a5518f7d3c629b30371dafc0d2b938f16c56a5`, same bytes as Arm A. |

The untracked `tests/projected_polygons.rs` on master hashes to `65537ce4e2b367a9b55e53d6ff5225d6c5bc2cddc89e822c8d6841c712a54052`. That is a different file from Arm A's committed test. Replay does not compile it.

## Fixtures

`pl_basins-1.json`, `pl_basins-2.json`, and `indi_pol-1.json` are byte-identical across `.scratch/issues-1-2/`, Arm A `fixtures/projected/`, the dirty Arm B `fixtures/projected/`, and the continuation `fixtures/projected/`.

| file | sha256 | rings | offsets including the end |
| --- | --- | --- | --- |
| `pl_basins-1.json` | `486185b932322440f0a773496b67c80bc238e453c83253ae9a20d2e91631bf77` | 2900, 4, 4 | 0, 2900, 2904, 2908 |
| `pl_basins-2.json` | `1923b3261f191ba581bfca71d9a244dcc08dd769143a2bbce2506743cdb4e77a` | 3358 | 0, 3358 |
| `indi_pol-1.json` | `7a03ea26d3709a8d7fde2c44d53dba472245900651b117a26fafc2df3c0f49e0` | 25963 | 0, 25963 |

Those counts come from the JSON arrays. Basin 1 is feature 1. Basin 2 is feature 2. The river file is the issue 2 polygon. Arm A and continuation also have `fixtures/projected/PROVENANCE.txt` at sha256 `2ad3bec3a7c48403d37c1440b67c1485a15556c474a0ae8dc52b5fb41bd95b0e`. Arm B has the three JSON files and no `PROVENANCE.txt`.

The reduced ridge is the nine literals in `basin_spoke_collapse_keeps_leftover_ridge`, not a separate JSON file. All nine are exact binary64 points of basin feature 2, ring 0, at indices 1273, 1274, 1279, 1320, 1321, 1322, 1323, 1324, and 1325. None of them occur in `pl_basins-1.json`.

The audit copied at `assets/2026-11-14-projected-basin-oracle-contract.md` lines 313 to 314 says the four original binary64 points match basin 2 ring-0 vertices 1321, 1322, 1323, and 1324. Those four are vertices 4 through 7 of the nine, which are the audit's edges 4 to 5 and 6 to 7. The audit does not itself claim all nine. The nine-point check is the count above.

Arm A's prose, copied at `assets/arm-a-issue-1-projected-basins.md` line 3, sha256 `e7018fc364bdab80f0e3dc2a13fb7da43a5bdc31cc19c30c3a2b057d7a22b7b1`, calls the same polygon "the nine-vertex reduction of basin feature 1". That label does not match the vertices.

## Reconciliation

Status `unrecoverable` for one arc and node count.

`assets/continue.md` line 15, sha256 `6fc6f1dc4450a9e1e87474db03e2a077d44b40ab31bd1c2b85fbf489eab3fbfd`, says feature 1 failed with 58 components, edges 5679, nodes 5730, rank 7. The copy is the file that was at `/tmp/swarm-issue1/continue.md`.

The dirty continuation diagnostic, copied at `assets/continuation-issue-1-projected-basins.md` line 19, sha256 `92c8aa39fd16494bf2cfec4137f92c0ca4292414623e22fcf57f56d48fe6c8b1`, says 58 coordinate components, 5643 arcs, 5694 nodes, rank 7. That sentence is not in the committed blob at `1b6ed8e`.

Both files say 58 components. The arc and node counts disagree. This package does not pick one.

## Coverage of this command

The command runs one named test in `tests/projected_polygons.rs`. It does not build `tests/upstream_corpus.rs`, `tests/differential.rs`, `tests/gis.rs`, or the library unit tests. No oracle or corpus check runs.

`SURFER2_TEST_DATA` was unset. `/tmp/surfer2/test-data` was absent. `SURFER2_GIS_DATA` was unset. The C++ surfer binaries were not invoked. Replay does not start the corpus, differential, or gis targets.

The Arm A tests have no `#[ignore]` on the control harness, Arm A, the continuation, or the Arm B harness. Arm B's historical tests have no `#[ignore]` either. No `#[cfg(feature)]` gate was found in the Arm A tree. Debug and release both ran. Each log says `running 1 test`.

`.scratch/issues-1-2/baseline-release`, `current-release`, and the minimizer were not used.

## Observed results

`evidence/issue-6/replay.sh` exited 0 at `2026-11-14T14:38:44Z`. `rustc 1.95.0 (59807616e 2026-04-14)`. `runner_failed` is false. Debug printed the same public signal as release for every case. Exit codes below are the `cargo test` process codes. Control ran from `evidence/control-harness`, not from the main checkout.

```text
control release basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: basin 1: IncompleteFaceCycle { detail: "stage=bounded-validation face=277: owns 25 halfedges but has no root" }
control release basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=18: owns 12 halfedges but has no root" }
control release reduced_ridge exit=101 basin_spoke_collapse_keeps_leftover_ridge: assertion `left == right` failed: exported coordinates must form one component   left: 2  right: 1
control release river exit=0 indi_pol_river_keeps_connected_tree: ok
control debug basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: basin 1: IncompleteFaceCycle { detail: "stage=bounded-validation face=277: owns 25 halfedges but has no root" }
control debug basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=18: owns 12 halfedges but has no root" }
control debug reduced_ridge exit=101 basin_spoke_collapse_keeps_leftover_ridge: assertion `left == right` failed: exported coordinates must form one component   left: 2  right: 1
control debug river exit=0 indi_pol_river_keeps_connected_tree: ok
arm_a release basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: basin 1: IncompleteFaceCycle { detail: "stage=bounded-validation face=13: owns 5 halfedges but has no root" }
arm_a release basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=600: owns 10 halfedges but has no root" }
arm_a release reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
arm_a release river exit=0 indi_pol_river_keeps_connected_tree: ok
arm_a debug basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: basin 1: IncompleteFaceCycle { detail: "stage=bounded-validation face=13: owns 5 halfedges but has no root" }
arm_a debug basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=600: owns 10 halfedges but has no root" }
arm_a debug reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
arm_a debug river exit=0 indi_pol_river_keeps_connected_tree: ok
arm_b_dirty release basin_1 exit=101 basin_feature_1_point_contacts_have_connected_skeleton: basin 1 skeleton: IncompleteFaceCycle { detail: "stage=bounded-validation face=1137: owns 71 halfedges but has no root" }
arm_b_dirty release basin_2 exit=101 basin_feature_2_is_one_tree: basin 2 skeleton: IncompleteFaceCycle { detail: "stage=bounded-validation face=346: owns 7 halfedges but has no root" }
arm_b_dirty release reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
arm_b_dirty release river exit=0 river_polygon_keeps_connected_tree_counts: ok
arm_b_dirty debug basin_1 exit=101 basin_feature_1_point_contacts_have_connected_skeleton: basin 1 skeleton: IncompleteFaceCycle { detail: "stage=bounded-validation face=1137: owns 71 halfedges but has no root" }
arm_b_dirty debug basin_2 exit=101 basin_feature_2_is_one_tree: basin 2 skeleton: IncompleteFaceCycle { detail: "stage=bounded-validation face=346: owns 7 halfedges but has no root" }
arm_b_dirty debug reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
arm_b_dirty debug river exit=0 river_polygon_keeps_connected_tree_counts: ok
arm_b_arm_a_harness release basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: basin 1: IncompleteFaceCycle { detail: "stage=bounded-validation face=1137: owns 71 halfedges but has no root" }
arm_b_arm_a_harness release basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=346: owns 7 halfedges but has no root" }
arm_b_arm_a_harness release reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
arm_b_arm_a_harness release river exit=0 indi_pol_river_keeps_connected_tree: ok
arm_b_arm_a_harness debug basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: basin 1: IncompleteFaceCycle { detail: "stage=bounded-validation face=1137: owns 71 halfedges but has no root" }
arm_b_arm_a_harness debug basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=346: owns 7 halfedges but has no root" }
arm_b_arm_a_harness debug reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
arm_b_arm_a_harness debug river exit=0 indi_pol_river_keeps_connected_tree: ok
continuation_dirty release basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: assertion `left == right` failed: edges=5679 nodes=5730 rank=7   left: 58  right: 1
continuation_dirty release basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=2268: owns 78 halfedges but has no root" }
continuation_dirty release reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
continuation_dirty release river exit=0 indi_pol_river_keeps_connected_tree: ok
continuation_dirty debug basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: assertion `left == right` failed: edges=5679 nodes=5730 rank=7   left: 58  right: 1
continuation_dirty debug basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=2268: owns 78 halfedges but has no root" }
continuation_dirty debug reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
continuation_dirty debug river exit=0 indi_pol_river_keeps_connected_tree: ok
```

`arm_b_dirty` runs Arm B's own tests. `arm_b_arm_a_harness` runs Arm A's names on that same engine. The reduced-ridge test passes in both. Only the historical test calls `assert_dcel_contract`.

This run's continuation failure string says 5679 edges and 5730 nodes. That matches `assets/continue.md` and not the copied diagnostic's 5643 and 5694. The diagnostic file was not edited by the replay. The disagreement stays unresolved.

Debug river tests finished in 233.43s on control, 233.96s on Arm A, 234.77s on Arm B, 235.85s on the Arm B harness, and 234.85s on the continuation. Those times are the `finished in` lines in the matching logs.

Executables are the `projected_polygons` test binaries cargo printed for `--no-run` in that checkout. The filename suffix is the same across checkouts. The file sha256 is not. Scratch binaries were not used.

| revision | profile | sha256 |
| --- | --- | --- |
| `control` | release | `b143ce5e28e1d6731d9c8468c965dbf572781450401665e1b708826a25f0b02a` |
| `control` | debug | `159ad92225ba6ee1f207b6297023ca74f3ebbd80a305b9704beec3d1fdf6e678` |
| `arm_a` | release | `ea842d40e86e2658ff1ee6d75e152de4cee2f35b40cf21dfde15b153ed80f90f` |
| `arm_a` | debug | `5fcc0ef1c0b86e5e1912a875c0289e232caa0f054502fb2d965fb20163b72d7a` |
| `arm_b_dirty` | release | `529f937d2c36d68a8427de08290435299b179d9c793e4d8cc141e0af44d26e63` |
| `arm_b_dirty` | debug | `f0977c394e3285bfd6bae7810f89d57675c5bc7e75d2fb72cbdab4fa826d92d4` |
| `arm_b_arm_a_harness` | release | `ef4c9ff2a2a4484c6ba44a66c1a33ea333fe12b57653ea1fb5fef1e277798719` |
| `arm_b_arm_a_harness` | debug | `b6737d933e7bf802e436cce56f719a2454f214bd39fbf7189c3bccaa84b68054` |
| `continuation_dirty` | release | `3c42dda5e51b827574c69c02b178aacdb916650c6a6fb3aa009a43fb29890668` |
| `continuation_dirty` | debug | `41ed1c0518da518cce23080dc57c9f99c8fd56e22e67f578e44da1fd971beb50` |

Full paths are in `manifest.json`. They point at `evidence/issue-6/work/`, which `replay.sh` creates from the bundle.

## Git status around this package

Before any file in `evidence/` existed, `git status --porcelain` on master was `?? docs/` and `?? tests/projected_polygons.rs`, at `d1a89fd66795f537b034f220ed4f838bd0fdea3b`.

After the replay, the same command on master is those two lines plus `?? evidence/`. `git diff` and `git diff --cached` on master are empty. `docs/` and `tests/projected_polygons.rs` were not edited.

Worktree `git rev-parse HEAD` and `git status --porcelain` after the replay match the before snapshot. `target/` is ignored by `.gitignore` line `/target`, so the new test binaries do not appear in porcelain.

## Access gaps that remain

`d1a89fd` still has no tracked `tests/projected_polygons.rs`. The control rows are that commit plus the Arm A overlay in the bundle, not an edit of the main checkout.

Arm B's historical tests keep their own names and the extra `assert_dcel_contract` call. The same engine was also run with Arm A's harness. The historical tree has no `fixtures/projected/PROVENANCE.txt`. The harness commit adds Arm A's copy.

One arc and node count for the old continuation write-up is unrecoverable, as recorded above.

`/tmp/surfer2/test-data` is absent. The replay command does not read it.

The expanded face-600 trace is in `assets/basin2.txt`. The revision that printed those halfedge fields is unknown.
