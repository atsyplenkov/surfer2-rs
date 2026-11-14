ISSUES

Worktree: /home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-28189dfa59ae
Branch: fix/issue-1-arm-a
Base: d1a89fd66795f537b034f220ed4f838bd0fdea3b
HEAD: 1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c
Log: 1b6ed8e fix: keep finite fast intersections on projected ridges
046b96e test: require the nine-vertex basin ridge to stay connected

Nine-vertex, red before any production edit on this branch:
cargo test --test projected_polygons basin_spoke_collapse_keeps_leftover_ridge -- --ignored --nocapture
FAILED tests/projected_polygons.rs:41 assertion left 2 right 1, "exported coordinates must form one component". Construction returned Ok.
Green after the fix, debug:
cargo test --test projected_polygons basin_spoke_collapse_keeps_leftover_ridge -- --nocapture --test-threads=1
ok. The same test is ok in the release suite below.

Basins, debug:
cargo test --test projected_polygons projected_basin -- --test-threads=1 --nocapture
projected_basin_1_has_connected_finite_skeleton FAILED: IncompleteFaceCycle, face 13 owns 5 halfedges but has no root.
projected_basin_2_is_one_tree FAILED: IncompleteFaceCycle, face 600 owns 10 halfedges but has no root.
Release, same assertions, inside cargo test --release --locked -- --test-threads=2:
face 13 and face 600, same detail strings.

On the velocity policy that shipped, face 600's open ends are (463811.7053, 6272413.6268) and (463811.2036, 6272413.6311), 0.5017 m apart, plus a 7.3 mm stub at (464087.63, 6272133.01). The event is a 3-constraint triangle collapse at t=649.305412414482. Two vertices have already met. The third, speed about 4.05e5, meets the other two edges 1.24e-6 s later and is still 0.50 m short. Face 13 includes a halfedge out to about (613156.9016, 13048302.7462). Splitting only the short edge, rejecting unit-weight times past the input diagonal, and refusing degenerate snaps longer than the footprint each moved those failures and did not make either basin valid. They are not in the commit.

River, debug, in cargo test --test projected_polygons -- --test-threads=1 (the river test passed before the basin failures aborted the binary):
indi_pol_river_keeps_connected_tree ok.
Release, same suite: indi_pol_river_keeps_connected_tree ok.
The test requires one tree, 51923 arcs, 51924 coordinate nodes, cycle rank 0.

Suite: cargo test --release --locked -- --test-threads=2
exit 101.
lib 43 passed; differential 19 passed, 3 ignored; gis 19 passed, both lakes; projected_polygons 5 passed, 2 failed (the two basins). upstream_corpus was not reached.
Follow-up: cargo test --release --locked --test upstream_corpus -- --test-threads=2
exit 0, 3 passed.

cargo fmt --check: exit 0 (after cargo fmt).
cargo clippy --all-targets --all-features --locked -- -D warnings -D clippy::dbg_macro: exit 0.

Files in 1b6ed8e:
src/wavefront/supporting_line.rs
src/dcel/mod.rs
tests/projected_polygons.rs
fixtures/projected/PROVENANCE.txt
fixtures/projected/pl_basins-1.json
fixtures/projected/pl_basins-2.json
fixtures/projected/indi_pol-1.json
docs/diagnostics/issue-1-projected-basins.md
Untracked and not committed: docs/diagnostics/issues-1-2.md (older investigation). No push, no PR, main checkout untouched, nothing committed on fix/issue-1-projected-basins.

Cause. The nine-vertex near-parallel pair has a finite speed near 1.98e7. The old hypot > 1e6 cap treated that intersection as missing, stored zero velocity, and dropped the leftover ridge, so a successful skeleton had two coordinate components. Removing the cap and keeping offset subtraction left about 0.82 m of endpoint gap, because p(1) - p(0) cancels at UTM scale and the fast spoke amplifies it. The shipped rule keeps subtraction for moderate, agreeing velocities (so weighted and degree-one oracles stay) and uses the normal-line intersection when the speed exceeds 1e6 or a projected frame disagrees by more than 1e-10. The topology change drops a second kinetic halfedge on the same directed node pair. Without it, lake 10623 builds an unreachable 2-cycle after the infinity-chord pass (face 11004, halfedge 50738 duplicating 50734). Together they make the nine-vertex polygon one tree with exact binary64 joins. Replacing every velocity with the normal formula stays rejected: it fixed the nine-vertex case, looped a spoke fixture, moved weighted random-2 from 121 to 118 and degree-one simple from 40 to 39, broke the larger lake, and still failed both original basins.

Conformance failures that still hold:
- Basin 1, debug and release: bounded validation fails, face 13 owns 5 halfedges and has no root. One halfedge leaves the basin, ending near (613157, 13048303).
- Basin 2, debug and release: bounded validation fails, face 600 owns 10 halfedges and has no root. The hole-free tree check never runs. The measured miss is 0.50 m on a fast vertex stopped by a 3-constraint collapse whose other edges meet 1.24 microseconds later.
- The release command exits 101 because those two tests fail. Nine-vertex, river counts, lake gates, weighted and degree-one oracles, fmt, and clippy did not fail. No existing expected value was weakened.
