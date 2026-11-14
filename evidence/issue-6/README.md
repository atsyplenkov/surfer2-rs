# Projected-basin evidence for issue 6

This directory records four source revisions and one replay of their public basin tests. It does not say why a basin fails, and it does not recommend a patch.

The registry is `manifest.json`. The patches are under `diffs/`. `replay.sh` rebuilds each checkout and writes the observation rows back into the manifest. Logs from the run below are under `logs/`.

## Run the comparison again

From any working directory, run `evidence/issue-6/replay.sh`.

The script compares every checkout's `HEAD` and captured diff before it builds. Cargo starts only after all four match. A mismatch exits 1 and does not open the log files, so the `observations` array and the logs it cites stay as they were. It does not stage, commit, or edit source files. It does write `target/` inside a checkout whose tests it runs. A second run that still matches overwrites `logs/` and the `observations` array.

Each test uses one command of this shape, with `--release` only for the release profile.

```text
cd <checkout> && CARGO_TERM_COLOR=never cargo test --release --test projected_polygons --locked -- <test> -- --exact --test-threads=1 --nocapture
```

A single test stops at 600 seconds. That ceiling is `CASE_TIMEOUT_S`. Raise it and run the script again if you need a result that hit the ceiling. None of the tests below hit it.

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
| control | absent from `HEAD`. `git cat-file -e HEAD:tests/projected_polygons.rs` failed. |
| `arm_a` | `b15af0682850b71cd288f05c02a5518f7d3c629b30371dafc0d2b938f16c56a5` |
| `arm_b_dirty` | `802b45c4a14698f7506a8b76b25c8723d368bd15c1d93d37f0bbbb1789a08cf0` (dirty file). The file at `HEAD` hashes to `7ff91b7a24384e08e0ab6ae448c952ececb8b38dff52ac7ed20c7634d66165ed` and does not define the full-basin or river tests. |
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

The audit at `/tmp/opencode/surfer2-basin-oracle-research/docs/research/2026-11-14-projected-basin-oracle-contract.md` lines 313 to 314 says the four original binary64 points match basin 2 ring-0 vertices 1321, 1322, 1323, and 1324. Those four are vertices 4 through 7 of the nine, which are the audit's edges 4 to 5 and 6 to 7. The audit does not itself claim all nine. The nine-point check is the count above.

Arm A's prose at `docs/diagnostics/issue-1-projected-basins.md` line 3, sha256 `e7018fc364bdab80f0e3dc2a13fb7da43a5bdc31cc19c30c3a2b057d7a22b7b1`, calls the same polygon "the nine-vertex reduction of basin feature 1". That label does not match the vertices.

## Reconciliation

Status `unrecoverable` for one arc and node count.

`/tmp/swarm-issue1/continue.md` line 15, sha256 `6fc6f1dc4450a9e1e87474db03e2a077d44b40ab31bd1c2b85fbf489eab3fbfd`, says feature 1 failed with 58 components, edges 5679, nodes 5730, rank 7.

The dirty continuation file `docs/diagnostics/issue-1-projected-basins.md` line 19, sha256 `92c8aa39fd16494bf2cfec4137f92c0ca4292414623e22fcf57f56d48fe6c8b1`, says 58 coordinate components, 5643 arcs, 5694 nodes, rank 7. That sentence is not in the committed blob at `1b6ed8e`.

Both files say 58 components. The arc and node counts disagree. This package does not pick one.

## Coverage of this command

The command runs one named test in `tests/projected_polygons.rs`. It does not build `tests/upstream_corpus.rs`, `tests/differential.rs`, `tests/gis.rs`, or the library unit tests. No oracle or corpus check runs.

`SURFER2_TEST_DATA` was unset. `/tmp/surfer2/test-data` was absent. `SURFER2_GIS_DATA` was unset. The C++ surfer binaries were not invoked. Replay does not start the corpus, differential, or gis targets.

The four tests named below have no `#[ignore]` on Arm A, on the continuation tree, or in the dirty Arm B file. No `#[cfg(feature)]` gate was found in the Arm A tree. Debug and release both apply. Both were run.

`.scratch/issues-1-2/baseline-release`, `current-release`, and the minimizer were not used.

## Observed results

`evidence/issue-6/replay.sh` exited 0 at `2026-11-14T12:23:21Z`. `rustc 1.95.0 (59807616e 2026-04-14)`. `runner_failed` is false. Where a profile ran, debug printed the same public signal as release. Exit codes below are the `cargo test` process codes. Control was not compiled, so those rows have no exit code.

```text
control release basin_1 exit=None projected_basin_1_has_connected_finite_skeleton: test source absent
control release basin_2 exit=None projected_basin_2_is_one_tree: test source absent
control release reduced_ridge exit=None basin_spoke_collapse_keeps_leftover_ridge: test source absent
control release river exit=None indi_pol_river_keeps_connected_tree: test source absent
control debug basin_1 exit=None projected_basin_1_has_connected_finite_skeleton: test source absent
control debug basin_2 exit=None projected_basin_2_is_one_tree: test source absent
control debug reduced_ridge exit=None basin_spoke_collapse_keeps_leftover_ridge: test source absent
control debug river exit=None indi_pol_river_keeps_connected_tree: test source absent
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
continuation_dirty release basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: assertion `left == right` failed: edges=5679 nodes=5730 rank=7   left: 58  right: 1
continuation_dirty release basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=2268: owns 78 halfedges but has no root" }
continuation_dirty release reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
continuation_dirty release river exit=0 indi_pol_river_keeps_connected_tree: ok
continuation_dirty debug basin_1 exit=101 projected_basin_1_has_connected_finite_skeleton: assertion `left == right` failed: edges=5679 nodes=5730 rank=7   left: 58  right: 1
continuation_dirty debug basin_2 exit=101 projected_basin_2_is_one_tree: basin 2: IncompleteFaceCycle { detail: "stage=bounded-validation face=2268: owns 78 halfedges but has no root" }
continuation_dirty debug reduced_ridge exit=0 basin_spoke_collapse_keeps_leftover_ridge: ok
continuation_dirty debug river exit=0 indi_pol_river_keeps_connected_tree: ok
```

Arm B does not define `projected_basin_1_has_connected_finite_skeleton`, `projected_basin_2_is_one_tree`, or `indi_pol_river_keeps_connected_tree`. The rows above use the tests that dirty file does define for the same four cases.

This run's continuation failure string says 5679 edges and 5730 nodes. That matches `continue.md` and not the dirty diagnostic's 5643 and 5694. The diagnostic file was not edited by the replay. The disagreement recorded above stays unresolved.

Debug river tests finished in 183.44s on Arm A, 180.83s on Arm B, and 179.78s on the continuation. Those times are the `finished in` lines in the matching logs.

Executables are the `projected_polygons` test binaries cargo printed for `--no-run` in that checkout. The filename suffix is the same across checkouts. The file sha256 is not. Scratch binaries were not used.

| revision | profile | sha256 | path |
| --- | --- | --- | --- |
| `arm_a` | release | `c832dca69a52bc4a20e3c66e5867db2fff54bc737ffff1ebfb676965b9232a5d` | `.../subagent-01a1d91d-ffd1-7df2-a99d-28189dfa59ae/target/release/deps/projected_polygons-7c7d7ee88c2c916c` |
| `arm_a` | debug | `96ab7b3bfd62c305a0361119eec98d3099c50740d8aa82cbe461476d2916c2c4` | `.../28189dfa59ae/target/debug/deps/projected_polygons-ccf60e7a7329240b` |
| `arm_b_dirty` | release | `a92ab0d670dcde76ca862af4228c2d37dba6d52446f5cfd880869af6246be71f` | `.../subagent-01a1d91d-ffd1-7df2-a99d-282f7bdcc9ba/target/release/deps/projected_polygons-7c7d7ee88c2c916c` |
| `arm_b_dirty` | debug | `6d47c90672bbc82fe194caa039f4c8d8da812d0d73b4775a85614209d7ee083b` | `.../282f7bdcc9ba/target/debug/deps/projected_polygons-ccf60e7a7329240b` |
| `continuation_dirty` | release | `8c5e5c1014672cca3f7a83c87acd6632d7548f1c0d82201ff4abc96c99ada7a0` | `.../subagent-01a1d970-f5c4-7cc3-8bfd-7ec922c43176/target/release/deps/projected_polygons-7c7d7ee88c2c916c` |
| `continuation_dirty` | debug | `8c17facda69dbf742bc80e093590dbd0cae35359cdfed5a6493ac5bd9c2d34c5` | `.../7ec922c43176/target/debug/deps/projected_polygons-ccf60e7a7329240b` |

The `...` prefixes are `/home/ats/.grok/worktrees/development-surfer2-rs/`. Full paths are in `manifest.json`.

## Git status around this package

Before any file in `evidence/` existed, `git status --porcelain` on master was `?? docs/` and `?? tests/projected_polygons.rs`, at `d1a89fd66795f537b034f220ed4f838bd0fdea3b`.

After the replay, the same command on master is those two lines plus `?? evidence/`. `git diff` and `git diff --cached` on master are empty. `docs/` and `tests/projected_polygons.rs` were not edited.

Worktree `git rev-parse HEAD` and `git status --porcelain` after the replay match the before snapshot. `target/` is ignored by `.gitignore` line `/target`, so the new test binaries do not appear in porcelain.

## Access gaps that remain

Control has no tracked `tests/projected_polygons.rs`. The observation is test source absent.

Arm B's dirty tests use different names from Arm A for three of the four cases. Arm B has no `fixtures/projected/PROVENANCE.txt`.

One arc and node count for the old continuation write-up is unrecoverable, as recorded above.

`/tmp/surfer2/test-data` is absent. The replay command does not read it.
