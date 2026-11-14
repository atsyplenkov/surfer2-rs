# Handoff: issue #1 projected-basin skeleton

Next session focus: understand the remaining failure, and what has already been tried to fix it. Do not treat the reduced nine-vertex case as a resolution of the original basins.

## Goal

Users need `interior_straight_skeleton` to return a complete, finite, connected skeleton for both original EPSG:32637 basin polygons in GitHub issue #1, without simplifying, snapping, or moving input coordinates. A successful `Result` that drops a ridge does not count. The #2 river tree is a required regression guard.

The spec is the GitHub comment, not a set of child tickets:

https://github.com/atsyplenkov/surfer2-rs/issues/1#issuecomment-6873800700

A local copy of that comment body is `/tmp/spec-comment.md`. Do not paste it again. Read it.

Issue #1 is still open. Issue #2 is the river guard, also still open. Repo: `atsyplenkov/surfer2-rs`. Workspace: `/home/ats/Development/surfer2-rs`.

## Where the tree is

Main checkout is clean of the experiment. It is on `master` at `d1a89fd66795f537b034f220ed4f838bd0fdea3b` (`fix(dcel): preserve shared endpoint identity and coordinates`). That commit is the five-commit prerequisite the spec names (`c357caa`, `ee63b04`, `112438d`, `917b651`, `d1a89fd`). `master` is ahead of `origin/master` by those five commits. Nothing from this session was pushed. No PR was opened.

`fix/issue-1-projected-basins` exists in the main repo and is still `d1a89fd`. It was the empty integration branch. Do not treat it as a finished fix.

The only committed partial is in the main repo as `fix/issue-1-arm-a`:

- `046b96e` `test: require the nine-vertex basin ridge to stay connected`
- `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` `fix: keep finite fast intersections on projected ridges`

That branch was fetched from racer A's worktree into the main repo. It is not checked out on `master`.

Untracked in the main checkout, and not part of the committed partial: `docs/` and `tests/projected_polygons.rs`. The committed tests and fixtures live on `fix/issue-1-arm-a`, not in the dirty main tree. Do not assume the untracked main-checkout test file matches the branch.

## The failure, in two layers

### Layer 1, fixed on `1b6ed8e`: the nine-vertex ridge

On `d1a89fd`, construction of the nine-vertex reduction returns `Ok`, and the exported coordinates form two components. Parent reproduction, one `cargo test` process:

```text
cargo test --test projected_polygons basin_spoke_collapse_keeps_leftover_ridge -- --ignored --nocapture
assertion `left == right` failed: exported coordinates must form one component
  left: 2
 right: 1
```

The test file at that point was the untracked `tests/projected_polygons.rs` on `master`, with the test ignored. The polygon coordinates are in the spec and in that test. Mechanism, confirmed by the racers and written up on the branch in `docs/diagnostics/issue-1-projected-basins.md`: `intersection_velocity` in `src/wavefront/supporting_line.rs` treated `hypot > 1e6` as a missing intersection. The nine-vertex near-parallel pair has a finite speed near `1.98e7`. The missing case stored zero velocity and dropped the leftover ridge. Removing the cap and keeping `p(1) - p(0)` left about `0.82 m` of endpoint gap, because the subtraction cancels at UTM scale.

On `1b6ed8e` the parent re-ran this release command in racer A's worktree. It finished in 5.61 seconds and exited 101:

```text
cargo test --release --test projected_polygons --locked -- \
  basin_spoke_collapse_keeps_leftover_ridge \
  projected_basin_1_has_connected_finite_skeleton \
  projected_basin_2_is_one_tree \
  indi_pol_river_keeps_connected_tree \
  -- --test-threads=1 --nocapture
```

`basin_spoke_collapse_keeps_leftover_ridge` passed. `indi_pol_river_keeps_connected_tree` passed at one tree, 51923 arcs, 51924 coordinate nodes, cycle rank 0.

### Layer 2, still open: both original basins

Same release command, same commit:

- `projected_basin_1_has_connected_finite_skeleton` failed. `IncompleteFaceCycle`, `stage=bounded-validation face=13: owns 5 halfedges but has no root`.
- `projected_basin_2_is_one_tree` failed. `IncompleteFaceCycle`, `stage=bounded-validation face=600: owns 10 halfedges but has no root`.

Face ids are observations, not acceptance criteria. The spec says the same thing.

Racer A's account of those faces, recorded in `docs/diagnostics/issue-1-projected-basins.md` on `1b6ed8e` and in `/tmp/swarm-issue1/arm-a.md`:

- Face 600 open ends near `(463811.7053, 6272413.6268)` and `(463811.2036, 6272413.6311)`, about `0.50 m` apart, plus a `7.3 mm` stub. A 3-constraint triangle at `t=649.305412414482`. Two vertices have met. The third, speed about `4.05e5`, meets the other edges about `1.24e-6 s` later and is still `0.50 m` short.
- Face 13 includes a halfedge out to about `(613157, 13048303)`.

The continuation agent, still at `1b6ed8e` with an uncommitted experiment, refined that account. The parent did not re-run the dirty tree. Treat the numbers below as that agent's report:

- Face 600's slow edge is nearly vertical, relative velocity about `(0.017, 2.000)`. The x-only edge time is `1.24e-6 s` before the planar meet. The `5 mm` same-stop does not link the face.
- Face 13's far point is a moderate-speed normal velocity stored with the far intersection origin, so `position_at(birth)` misses the birth point by megametres.
- A separate 1-constraint split successor with `|sin|` about `5.6e-6` is `Degenerate`, its natural collapse is `Never`, and it integrates for hundreds of seconds.
- After the uncommitted patches, face 13 and face 600 went away. Feature 1 then had a valid DCEL and 58 coordinate components. Gap median length `51.67 m`. The other side of the dominant gap is an input edge of `92.8 m`. Feature 2's open ends near `(437345, 6284236)` and `(437372, 6284235)` are `27 m` apart, with a `30 km` self-loop on the same face.
- The child they named next: a constraint-collapse child whose supporting lines are `Degenerate` (`|sin|` about `1.2e-5`, speed about `1.65e5`, opposing). On feature 1 it is born near `t=25.8`, half of a `51.7 m` corridor, already `92.8 m` from the opposite vertex. The river has four children of the same class that are real ridges (opposite vertex `3.2 m` and `14.7 m` away). Zeroing every fast `Degenerate` child split the river into 4 components. Zeroing only children whose opposite vertex is more than `20 m` away kept the river and reopened feature 1's face cycle. Raising the same-time spoke repair from about `7 m` to `200 m` did the same.

So the nine-vertex cap bug and the original-basin DCEL bug are not the same defect. The first is fixed on `fix/issue-1-arm-a`. The second is not.

## What was done

Execution was a best-of race of two full-spec implementers, then one continuation. `spawn_subagent` cannot set a model. The swarm-workers line asked for `grok-4.5`. The racers ran as general-purpose agents on the parent `grok-4.7`. The continuation was `pstack:poteto-agent`, also on `grok-4.7`. Worktrees were local, not cloud. Each racer had its own git dir. They do not share objects with `/home/ats/Development/surfer2-rs/.git` unless fetched.

### Racer A, best committed partial

Worktree: `/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-28189dfa59ae`

Branch `fix/issue-1-arm-a` at `1b6ed8e`. Report: `/tmp/swarm-issue1/arm-a.md`. Status `ISSUES`.

What the commit changes, so the next agent reads the diff instead of trusting this paraphrase:

- `src/wavefront/supporting_line.rs`. Offset subtraction stays for moderate velocities that agree with the normal formula. A finite speed above `1e6`, or a projected frame (`|p0| >= 1e4`) whose formulas disagree by more than `1e-10` relative, uses the normal-line intersection. Fast births are re-anchored on the known birth point.
- `src/dcel/mod.rs`. A second kinetic halfedge on the same directed node pair is dropped. Without that, lake 10623 built an unreachable 2-cycle after the infinity-chord pass once the velocity rule changed.
- Vendored lossless fixtures under `fixtures/projected/` (`pl_basins-1.json`, `pl_basins-2.json`, `indi_pol-1.json`) plus `fixtures/projected/PROVENANCE.txt`. Points in those JSON files are `[u64 f64 bits, u64 f64 bits]`. Decode with `f64::from_bits`.
- Public tests in `tests/projected_polygons.rs`, including the two basin tests that fail and the river count guard.
- `docs/diagnostics/issue-1-projected-basins.md`.

A also reported `cargo test --release --locked -- --test-threads=2` exit 101 because the two basin tests fail, with lib, differential, and gis (both lakes) green before that target stopped the run. A follow-up `upstream_corpus` run exited 0. `cargo fmt --check` and clippy with `-D warnings -D clippy::dbg_macro` exited 0. The parent did not re-run the full suite or clippy. The parent only re-ran the four named release tests.

### Racer B, worse partial, uncommitted

Worktree: `/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-282f7bdcc9ba`

Branch `fix/issue-1-arm-b`. `HEAD` is `7d3faf62ec7bd15f46365053922de2885dac87ca`, which is only the red nine-vertex test. The production edit is still dirty: `src/triangulation/collapse.rs`, `src/wavefront/supporting_line.rs`, `tests/projected_polygons.rs`, plus untracked `docs/` and `fixtures/projected/`. This branch was not fetched into the main repo.

Its policy keeps the historical velocity, including the `1e6` cap, unless the pair is opposing and `|sin| < 1e-4`. Those pairs get the normal-derived speed, and only when the motion already passes within `1 m` of the birth (`1e-6` at unit scale). A 3-constraint face keeps the earliest edge time unless a later time puts the three vertices much closer. Reported basin failures: feature 1 face 1137 owns 71 halfedges, feature 2 face 346 owns 7. Nine-vertex and river counts were green on that dirty tree. The parent did not re-run B.

Rank A above B. B still turns a well-conditioned speed above `1e6` into a missing intersection. The spec says a genuinely finite fast intersection must not become stationary only because its speed exceeds a fixed magnitude.

### Continuation, blocked, uncommitted

Worktree: `/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d970-f5c4-7cc3-8bfd-7ec922c43176`

Branch name `fix/issue-1-continue`, but `HEAD` is still `1b6ed8e`. Dirty against that commit:

- `src/triangulation/collapse.rs`
- `src/wavefront/edge.rs`
- `src/wavefront/split_leftover.rs`
- `src/wavefront/supporting_line.rs`
- `docs/diagnostics/issue-1-projected-basins.md`

178 insertions, 28 deletions. Not a fix. Do not commit it as one. The agent reported that birth replay is pinned when the miss exceeds `1 mm`, a 3-constraint triangle collapses only when the endpoints meet, a 1-constraint `Degenerate` successor may collapse now when the natural event is `Never`, and an edge collapse uses the planar closest-approach time when the residual is at most a centimetre. Those changes removed face 13 and face 600 and left the 58-component / 27 m residue above. Full `cargo test --release`, fmt, clippy, and debug basins were not run on that tree.

## Rejected experiments

Do not ship these. They are already written up in the spec and in `docs/diagnostics/issue-1-projected-basins.md`.

- Translating the original inputs near the origin did not resolve #1.
- Cap removal alone leaves about `0.82 m` of gap on the nine-vertex ridge.
- Replacing every velocity with the direct normal formula, together with cap removal, fixed the nine-vertex case and was rolled back. It looped a spoke fixture, moved weighted `random-2` from 121 to 118 and degree-one simple from 40 to 39, broke the larger lake DCEL check, and still failed both original basins.
- Using the normal formula for every near-parallel pair failed lake 10623 (face 11004 not reachable). A relative cutoff of `0` or `1e-12` changed event order the same way. `1e-10` was the cutoff A kept.
- Splitting only the short edge, rejecting unit-weight times past the input diagonal, and refusing degenerate snaps longer than the footprint moved the basin failures and did not make either basin valid.
- Refusing far triangle collapses, on-segment filters, and bbox gates either moved the basin DCEL hole, split a basin into 75 coordinate components, or split the river into 144 components.
- Distance cutoffs on fast `Degenerate` children cannot separate basin corridors from river ridges. See the continuation report above.

Out of scope, from the spec: R bindings, GEOS, CRS gates, a new skeleton backend, exact arithmetic as the default, new public config, tolerance stitching, local-origin preprocessing as a requirement. Do not raise the event-loop limit, bypass DCEL validation, drop ridges to match a count, snap outputs, or weaken oracle expected values.

## Premise to test before another patch

Four local velocity or collapse-time patches missed the same gate. The premise they shared: a local change to intersection velocity or collapse time is enough for both original basins to validate and stay connected, while the river and the lake oracles stay put.

The failure distribution contradicts that. Closing one basin hole opens a gap about as long as an input edge, and the same `Degenerate` near-parallel constraint child is a real ridge on the river and half of a corridor on basin 1. The next investigation should name those children by constraint ancestry, or by some other identity that is not a metre cutoff, and check that the river ridges and the basin corridors fall on different sides of that name. Then fix the first wrong decision for the basin class only.

Read `principle-attack-the-premise` before writing another threshold. The principle says to write the premise down and test it before the next fix. The table above is the evidence already collected. A new observation still has to be made if the next agent wants to claim the premise is false for a different reason.

## Fixtures and API facts the next agent will need

Source JSON, read-only, not vendored on `master`:

- `/home/ats/Development/surfer2-rs/.scratch/issues-1-2/pl_basins-1.json` has 3 rings, 2900, 4, and 4 points.
- `pl_basins-2.json` has 1 ring, 3358 points.
- `indi_pol-1.json` has 1 ring, 25963 points. That is the #2 river.

`Polygon::from_flat` is in `src/input.rs`. It infers a trailing sentinel when the last offset is not `xy.len()`. Basin 1 offsets are `0, 2900, 2904, 2908`. Basin 2 and the river are a single ring at offset `0`. Basin 1 has point-contact holes. Do not force it to be a hole-free tree. Basin 2 must be a tree.

Provenance hashes are in the spec and in `fixtures/projected/PROVENANCE.txt` on `fix/issue-1-arm-a`.

Project checks, from the repo `justfile`: `cargo test --release --locked -- --test-threads=2`, and clippy `--all-targets --all-features --locked -- -D warnings -D clippy::dbg_macro`, then `cargo fmt`.

## Suggested skills

Call the Skill tool for these before acting:

- `diagnosing-bugs`, for the remaining basin DCEL failure. The nine-vertex bug is already reproduced and fixed on `fix/issue-1-arm-a`.
- `tdd`, if a new regression is added. The basin tests on `1b6ed8e` are already red. Do not weaken them.
- `implement-spec`, only if the session goes back to landing the GitHub comment. The task graph is one coupled pipeline, not a set of child issues. There is no green integration branch yet.
- `principle-attack-the-premise`, before another velocity or collapse-time threshold.
- `principle-fix-root-causes`, so a guard that hides an incomplete face is not shipped as the fix.
- `principle-prove-it-works`, before claiming either basin passes. Re-run the four release tests named above on the commit being judged.
- `gh`, if the issue comment or CI needs to be read again. The spec URL is above.

Do not start with `code-review` or opening a PR. Nothing meets the closure bar.
