ISSUES

Worktree: /home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-282f7bdcc9ba
Branch: fix/issue-1-arm-b
Base: d1a89fd66795f537b034f220ed4f838bd0fdea3b
git rev-parse HEAD: 7d3faf62ec7bd15f46365053922de2885dac87ca
git log --oneline d1a89fd66795f537b034f220ed4f838bd0fdea3b..HEAD:
7d3faf6 test: require the nine-vertex basin ridge to stay connected

The production change is uncommitted. HEAD is the red nine-vertex test. A commit of this tree would stay red because both original basins still fail bounded DCEL validation. Nothing was pushed. The main checkout was not edited. fix/issue-1-projected-basins was not committed onto or merged.

Red before, on this branch before the production edit (the test was still ignored):
cargo test --test projected_polygons basin_spoke_collapse_keeps_leftover_ridge -- --ignored --nocapture
FAILED. Construction returned Ok. tests/projected_polygons.rs assertion `left == right` failed: "exported coordinates must form one component", left 2, right 1.

Green after, debug:
cargo test --test projected_polygons --locked -- basin_spoke_collapse_keeps_leftover_ridge --test-threads=1 --nocapture
ok. One component, hole-free tree, all nine input corners present, finite positive-length arcs, DCEL contract holds.

The same nine-vertex test also passed in release inside the projected_polygons release run below.

Basins, debug (exit 101):
cargo test --test projected_polygons --locked -- basin_feature_1_point_contacts_have_connected_skeleton basin_feature_2_is_one_tree --test-threads=1 --nocapture
Feature 1: IncompleteFaceCycle, stage=bounded-validation face=1137: owns 71 halfedges but has no root.
Feature 2: IncompleteFaceCycle, stage=bounded-validation face=346: owns 7 halfedges but has no root.
Construction returned Err, so exact-coordinate connectivity was not reached. Feature 1 is not asserted to be a hole-free tree.

Basins and the rest of that file, release (exit 101):
cargo test --release --test projected_polygons --locked -- --test-threads=1 --nocapture
Same two failures, same face text (feature 1 face 1137 owns 71, feature 2 face 346 owns 7). The other five tests passed, including the nine-vertex test and the river guard. Finished in 5.86s.

River, debug (exit 0, 171.14s):
cargo test --test projected_polygons --locked -- river_polygon_keeps_connected_tree_counts --test-threads=1 --nocapture
ok. The test requires one component, 51923 arcs, 51924 coordinate nodes, cycle rank 0.
Release: the same test passed in the projected_polygons release run above.

Suite:
cargo test --release --locked -- --test-threads=2
exit 101. lib 43 passed; differential 19 passed (3 ignored); gis 19 passed, including lake10623; projected_polygons 5 passed and 2 failed (the two basin tests). Cargo stopped at that failing target, so this command did not run tests/upstream_corpus.rs or doctests.
Follow-up, not a substitute for that exit code: cargo test --release --test upstream_corpus --locked -- --test-threads=2 exit 0 (3 passed). cargo test --release --locked --doc exit 0 (1 passed).

cargo fmt --check: exit 0.
cargo clippy --all-targets --all-features --locked -- -D warnings -D clippy::dbg_macro: exit 0.

Files changed, uncommitted, against 7d3faf6:
src/triangulation/collapse.rs
src/wavefront/supporting_line.rs
tests/projected_polygons.rs
docs/diagnostics/issue-1-leftover-ridge.md
fixtures/projected/pl_basins-1.json
fixtures/projected/pl_basins-2.json
fixtures/projected/indi_pol-1.json
docs/diagnostics/issues-1-2.md was already untracked in this worktree. No existing oracle number was weakened. No src instrumentation remains.

Confirmed cause. The 1e6 speed cap in intersection_velocity treats the nine-vertex notch as a missing intersection. Edges 4 and 6 are opposing, |sin| about 1.010e-7, and the finite speed is about 1.9802574214462075e7. The cap stores a zero velocity, construction still returns Ok, and the exported coordinates form two components. Deleting the cap alone leaves the two-time subtraction, which stops an endpoint about 0.82 m short of the common point, so exact-bit ancestry still does not join the ridge. The patch that is in the tree keeps the two-time velocity whenever it is finite and at most 1e6. Above the cap, only an opposing pair with |sin| < 1e-4 keeps the normal-derived speed, and only when that motion already passes within 1 m of the birth (1e-6 at unit scale); the birth is then back-projected. A 3-constraint face keeps the earliest edge time unless a later edge time puts the three vertices much closer. Together, the fast motion stays on the event path and the later time stops it on the shared point. The global direct-normal rewrite stays rejected: it looped a spoke, changed weighted and degree-one results, broke the larger lake check, and still failed both basins. Using the normal-derived speed for every near-parallel pair also failed lake 10623 (face 11004 not reachable) and was narrowed back. Refusing far triangle collapses, on-segment filters, and bbox gates was reverted: those either moved the basin DCEL hole, split a basin into 75 coordinate components, or split the river into 144 components.

Conformance failures:
Both original basins fail in debug and in release with IncompleteFaceCycle before any connectivity assertion. Feature 1 face 1137 owns 71 halfedges and has no root. Feature 2 face 346 owns 7 halfedges and has no root. A successful skeleton that drops a ridge would not count, and these results are not successful.
The required release suite exits 101 because of those two tests.
The nine-vertex regression, the river guard (51923 / 51924 / cycle rank 0), lake 10623, fmt, and clippy are green on this uncommitted tree.
